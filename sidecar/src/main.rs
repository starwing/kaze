mod clap_args;
mod codec;
mod dispatcher;
mod resolver;
mod socket_pool;
mod kaze {
    include!(concat!(env!("OUT_DIR"), "/kaze.rs"));
}

use std::io::{self};

use clap::Parser;
use kaze_core::KazeState;
use log::{error, info};
use metrics::counter;
use socket_pool::{Register, SocketPool};
use tokio::net::TcpListener;
use tokio::task::block_in_place;

#[tokio::main]
async fn main() -> io::Result<()> {
    let app = clap_args::Args::parse();

    let listener = TcpListener::bind((app.host, app.port)).await?;
    info!("Listening on {}:{}", app.host, app.port);

    let (sq, cq) =
        new_kaze_pair(app.shmfile, app.ident, app.sq_bufsize, app.cq_bufsize)?;
    info!(
        "create kaze shm ident={} (sq={}, cq={})",
        sq.ident(),
        app.sq_bufsize,
        app.cq_bufsize
    );
    let pool = SocketPool::new(sq);
    let (reg, mut dispatcher) = pool.split();

    async fn handle_listener(
        listener: TcpListener,
        mut reg: Register,
    ) -> io::Result<()> {
        loop {
            let (socket, addr) = listener.accept().await?;
            info!("Accepted connection from {}", addr);
            counter!("kaze-connections").increment(1);
            reg.incomming(socket, addr).await?;
        }
    }

    tokio::spawn(handle_listener(listener, reg));

    // main loop
    let mut cq = cq;
    loop {
        let ctx = match cq.try_pop() {
            Ok(ctx) => ctx,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                counter!("kaze-write-blocking").increment(1);
                block_in_place(|| cq.pop()).map_err(|e| {
                    counter!("kaze-read-blocking-errors").increment(1);
                    error!("Error reading from blocking kaze: {e}");
                    e
                })?
            }
            Err(e) => {
                counter!("kaze-read-errors").increment(1);
                error!("Error reading from kaze: {e}");
                return Err(e);
            }
        };

        let mut buf = ctx.buffer();
        let hdr = codec::decode_packet(&mut buf)?;
        counter!("kaze-packets").increment(1);
        dispatcher.dispatch(hdr, buf)?;
        ctx.commit()
    }
}

fn new_kaze_pair(
    shm_name: impl AsRef<std::path::Path>,
    ident: u32,
    sq_bufsize: usize,
    cq_bufsize: usize,
) -> io::Result<(KazeState, KazeState)> {
    let sq = KazeState::new(shm_name.as_ref(), ident, sq_bufsize)?;
    let cq = KazeState::new(shm_name.as_ref(), ident, cq_bufsize)?;
    Ok((sq, cq))
}
