use std::{io::IoSlice, net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{Context, Error, Result};
use bytes::BytesMut;
use metrics::counter;
use tokio::{
    io::AsyncWriteExt,
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf, ReuniteError},
        TcpStream,
    },
    select,
    sync::Mutex,
};
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, FramedRead};
use tracing::{enabled, error, trace};

use crate::{
    codec::{NetPacketCodec, NetPacketForwardCodec},
    corral::Corral,
    edge::Sender,
    kaze::Hdr,
};

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

    pub async fn dispatch(self, data: &kaze_core::Bytes<'_>) -> Result<()> {
        let size_buf = (data.len() as u32).to_le_bytes();
        let (s1, s2) = data.as_slice();
        let written = self
            .inner
            .lock()
            .await
            .write_vectored(&[
                IoSlice::new(&size_buf),
                IoSlice::new(s1),
                IoSlice::new(s2),
            ])
            .await
            .context("Failed to write to socket")?;
        counter!("kaze_write_packets_total").increment(written as u64);
        Ok(())
    }
}

pub struct ReadConn {
    inner: OwnedReadHalf,
    addr: SocketAddr,
}

impl ReadConn {
    pub fn new(read_half: OwnedReadHalf, addr: SocketAddr) -> Self {
        let inner = read_half;
        Self { inner, addr }
    }

    pub async fn main_loop(mut self, reg: Arc<Corral>, sender: Sender) {
        let r = if enabled!(tracing::Level::TRACE) || reg.rate_limit.is_some()
        {
            self.main_loop_tracing(reg.clone(), sender).await
        } else {
            self.main_loop_forward(reg.clone(), sender).await
        };

        if let Err(e) = r {
            counter!("kaze_connection_errors_total").increment(1);
            error!(error = %e, "Failed to handle connection");
        }

        if let Err(e) = reg.close_connection(self.inner, self.addr).await {
            counter!("kaze_close_errors_total").increment(1);
            error!(error = %e, "Failed to close connection");
        }
    }

    async fn main_loop_forward(
        &mut self,
        reg: Arc<Corral>,
        sender: Sender,
    ) -> Result<()> {
        let mut transport =
            FramedRead::new(&mut self.inner, NetPacketForwardCodec {});
        loop {
            let data = Self::read_timeout::<NetPacketForwardCodec>(
                &mut transport,
                reg.idle_timeout,
            )
            .await?;
            if let Some(mut data) = data {
                sender
                    .send(&mut data)
                    .await
                    .context("Failed to transfer packet")?
            } else {
                return Ok(());
            }
        }
    }

    async fn main_loop_tracing(
        &mut self,
        reg: Arc<Corral>,
        sender: Sender,
    ) -> Result<()> {
        let mut transport =
            FramedRead::new(&mut self.inner, NetPacketCodec {});
        loop {
            let data: Option<(Hdr, BytesMut)> =
                Self::read_timeout::<NetPacketCodec>(
                    &mut transport,
                    reg.idle_timeout,
                )
                .await?;
            if let Some((hdr, mut data)) = data {
                trace!(hdr = ?hdr, len = data.len(), "transfer packet");
                if let Some(limiter) = &reg.rate_limit {
                    limiter.acquire_one(hdr.src_ident, &hdr.body_type).await;
                }
                sender
                    .send(&mut data)
                    .await
                    .context("Failed to transfer packet")?;
            } else {
                return Ok(());
            }
        }
    }

    async fn read_timeout<D>(
        mut transport: impl StreamExt<Item = Result<<D as Decoder>::Item>> + Unpin,
        idle_timeout: Duration,
    ) -> Result<Option<<D as Decoder>::Item>>
    where
        D: Decoder,
        <D as Decoder>::Error: Into<Error>,
    {
        Ok(select! {
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
        })
    }
}
