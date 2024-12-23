mod clap_args;
mod ringbuf;
mod shm;
mod tokio_shm;

use clap::Parser;
use log::{error, info};
use metrics::counter;
use std::io::IoSlice;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::{self, net::TcpListener};
use tokio_shm::TokioShm;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = clap_args::Args::parse();

    let listener = TcpListener::bind((app.host, app.port)).await?;
    info!("Listening on {}:{}", app.host, app.port);

    let shm = shm::Shm::new(
        app.shm_name,
        app.ident,
        app.net_bufsize,
        app.host_bufsize,
    )?;
    let shm = Arc::new(TokioShm::new(shm));

    loop {
        let (mut socket, _) = listener.accept().await?;
        info!("Accepted connection from {}", socket.peer_addr()?);
        counter!("connections").increment(1);
        let shm = shm.clone();

        tokio::spawn(async move {
            let mut buf = Vec::with_capacity(app.bufsize);

            loop {
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n == 0 => return,
                    Ok(n) => n,
                    Err(e) => {
                        error!("Error reading from socket: {e}");
                        counter!("errors").increment(1);
                        return;
                    }
                };

                shm.push(&buf[..n]).await;
                let data = shm.pop().await;
                let data = data.as_slice();
                let data = [IoSlice::new(data.0), IoSlice::new(data.1)];

                if let Err(e) = socket.write_vectored(&data).await {
                    error!("Error writing to socket: {e}");
                    return;
                }
            }
        });
    }
}
