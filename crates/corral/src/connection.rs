use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::StreamExt;
use metrics::counter;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf, ReuniteError};
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::Mutex;
use tokio_util::codec::{Decoder, FramedRead};
use tracing::{error, trace};

use kaze_protocol::{NetPacketCodec, Packet};
use kaze_resolver::Resolver;

use crate::Corral;

pub struct ReadConn {
    inner: OwnedReadHalf,
    addr: SocketAddr,
}

#[derive(Clone)]
pub struct WriteConn {
    inner: Arc<Mutex<OwnedWriteHalf>>,
    addr: SocketAddr,
}

impl WriteConn {
    pub fn new(stream: OwnedWriteHalf, addr: SocketAddr) -> Self {
        let inner = Arc::new(Mutex::new(stream));
        Self { inner, addr }
    }

    pub async fn reunite(
        self,
        read_half: OwnedReadHalf,
    ) -> std::result::Result<TcpStream, (OwnedReadHalf, Self)> {
        let addr = self.addr;
        match Arc::try_unwrap(self.inner) {
            Ok(inner) => inner.into_inner().reunite(read_half).map_err(
                |ReuniteError(read_half, write_half)| {
                    (read_half, Self::new(write_half, addr))
                },
            ),
            Err(inner) => Err((read_half, WriteConn { inner, addr })),
        }
    }

    pub async fn send(self, data: &Packet<'_>) -> Result<()> {
        let mut iovec = data.as_iovec();
        let written = self
            .inner
            .lock()
            .await
            .write_vectored(&iovec.to_iovec())
            .await
            .context("Failed to write to socket")?;
        counter!("kaze_write_packets_total").increment(written as u64);
        Ok(())
    }
}

impl ReadConn {
    pub fn new(read_half: OwnedReadHalf, addr: SocketAddr) -> Self {
        let inner = read_half;
        Self { inner, addr }
    }

    pub async fn main_loop<R: Resolver>(mut self, corral: Arc<Corral<R>>) {
        let r = self.main_loop_tracing(corral.clone()).await;

        if let Err(e) = r {
            counter!("kaze_connection_errors_total").increment(1);
            error!(error = %e, "Failed to handle connection");
        }

        if let Err(e) = corral.close_connection(self.inner, self.addr).await {
            counter!("kaze_close_errors_total").increment(1);
            error!(error = %e, "Failed to close connection");
        }
    }

    async fn main_loop_tracing<R: Resolver>(
        &mut self,
        reg: Arc<Corral<R>>,
    ) -> Result<()> {
        let mut transport =
            FramedRead::new(&mut self.inner, NetPacketCodec {});
        while let Some(data) = Self::read_timeout::<NetPacketCodec>(
            &mut transport,
            reg.idle_timeout(),
        )
        .await?
        {
            let hdr = data.hdr();
            trace!(hdr = ?hdr, len = data.body_len(), "transfer packet");
            reg.acqure_token(hdr.src_ident, &hdr.body_type).await;
            reg.send(&data).await.context("Failed to transfer packet")?;
        }
        Ok(())
    }

    async fn read_timeout<D>(
        mut transport: impl StreamExt<Item = Result<<D as Decoder>::Item>> + Unpin,
        idle_timeout: Duration,
    ) -> Result<Option<<D as Decoder>::Item>>
    where
        D: Decoder,
        <D as Decoder>::Error: Into<anyhow::Error>,
    {
        let r = select! {
            pkg = transport.next() => {
                if let Some(pkg) = pkg {
                    Some(pkg.context("Failed to read packet")?)
                } else {
                    counter!("kaze_read_closed_total").increment(1);
                    None
                }
            },
            _ = tokio::time::sleep(idle_timeout) => {
                counter!("kaze_read_timeout_total").increment(1);
                None
            }
        };
        Ok(r)
    }
}
