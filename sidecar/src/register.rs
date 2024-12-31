use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashMap, net::SocketAddr};

use anyhow::{anyhow, bail, Context, Error, Result};
use bytes::{BufMut, BytesMut};
use metrics::{counter, gauge};
use tokio::{
    io::AsyncWriteExt,
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    select,
    sync::Mutex,
    task::{block_in_place, JoinSet},
};
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, FramedRead};
use tracing::{enabled, error, trace};

use crate::codec::{NetPacketCodec, NetPacketForwardCodec};
use crate::kaze::Hdr;
use crate::ratelimit::RateLimit;
use crate::resolver::Resolver;

/// register new incomming connection into socket pool
pub struct Register {
    sock_map: Mutex<HashMap<SocketAddr, Arc<Mutex<OwnedWriteHalf>>>>,
    sq: Mutex<kaze_core::KazeState>,
    pending_timeout: Duration,
    idle_timeout: Duration,
    rate_limit: Option<RateLimit>,
    join_set: Mutex<JoinSet<()>>,
}

impl Register {
    /// create a new register
    pub fn new(
        sq: kaze_core::KazeState,
        rate_limit: Option<RateLimit>,
        pending_timeout: impl Into<Duration>,
        idle_timeout: impl Into<Duration>,
    ) -> Self {
        Self {
            sock_map: Mutex::new(HashMap::new()),
            sq: Mutex::new(sq),
            rate_limit,
            pending_timeout: pending_timeout.into(),
            idle_timeout: idle_timeout.into(),
            join_set: Mutex::new(JoinSet::new()),
        }
    }

    /// gracefully exit
    pub async fn graceful_exit(self: Arc<Self>) -> Result<()> {
        let mut join_set = self.join_set.lock().await;
        while let Some(res) = join_set.join_next().await {
            if let Err(e) = res {
                error!(error = %e, "Failed to join connection handling task");
            }
        }
        Ok(())
    }

    /// handle incomming connection
    #[tracing::instrument(level = "trace", skip(self, resolver))]
    pub async fn handle_incomming(
        self: &Arc<Self>,
        stream: TcpStream,
        addr: SocketAddr,
        resolver: &Resolver,
    ) -> Result<()> {
        let mut transport = FramedRead::new(stream, NetPacketCodec {});

        // 1. waiting for the first packet to read
        let (hdr, mut data) = select! {
            pkg = transport.next() => if let Some(pkg) = pkg { pkg? }
                else {
                    println!("exit");
                    return Ok(());
                },
            _ = tokio::time::sleep(self.pending_timeout) => {
                counter!("kaze_read_timeout_total").increment(1);
                println!("timeout");
                return Ok(());
            }
        };

        // 2. transfer the packet
        trace!(hdr = ?hdr, len = data.len(), "transfer packet");
        self.transfer_pkg(&mut data).await?;

        // 3. add valid connection
        self.add_connection(
            transport.into_inner(),
            hdr.src_ident,
            addr,
            resolver,
        )
        .await;

        Ok(())
    }

    /// find a node that matches the ident
    #[tracing::instrument(level = "trace", skip(self, resolver))]
    pub async fn find_socket(
        self: &Arc<Self>,
        ident: u32,
        resolver: &Resolver,
    ) -> Result<Arc<Mutex<OwnedWriteHalf>>> {
        if let Some(addr) = resolver.get_node(ident).await {
            if let Some(sock) = self.sock_map.lock().await.get(&addr) {
                Ok(sock.clone())
            } else {
                self.try_connect(ident, addr, resolver)
                    .await
                    .context("Failed to connect")
            }
        } else {
            bail!("node not found ident={}", ident);
        }
    }

    async fn try_connect(
        self: &Arc<Self>,
        ident: u32,
        addr: SocketAddr,
        resolver: &Resolver,
    ) -> Result<Arc<Mutex<OwnedWriteHalf>>> {
        let stream = TcpStream::connect(addr).await;
        match stream {
            Ok(stream) => {
                Ok(self.add_connection(stream, ident, addr, resolver).await)
            }
            Err(e) => {
                counter!("kaze_connect_errors_total").increment(1);
                bail!("connect error: {}", e);
            }
        }
    }

    async fn add_connection(
        self: &Arc<Self>,
        stream: TcpStream,
        ident: u32,
        addr: SocketAddr,
        resolver: &Resolver,
    ) -> Arc<Mutex<OwnedWriteHalf>> {
        let (read_half, write_half) = stream.into_split();
        let write_half = Arc::new(Mutex::new(write_half));

        // 1. add connection to the map
        resolver.add_node(ident, addr).await;
        self.sock_map.lock().await.insert(addr, write_half.clone());

        // 2. spawn a new task to handle send to this socket
        let mut join_set = self.join_set.lock().await;
        join_set.spawn(self.clone().handle_connection(read_half, addr));
        gauge!("kaze_connections_total").increment(1);
        write_half
    }

    async fn close_connection(
        &self,
        read_half: OwnedReadHalf,
        addr: SocketAddr,
    ) -> Result<()> {
        gauge!("kaze_connections_total").decrement(1);
        if let Some(write_half) = self.sock_map.lock().await.remove(&addr) {
            // if the arc's strong count is not 1, just drop it, otherwise, try to
            // shutdown the connection
            if let Ok(write_half) = Arc::try_unwrap(write_half) {
                let mut stream = read_half
                    .reunite(write_half.into_inner())
                    .context("Failed to reunite")?;
                stream
                    .shutdown()
                    .await
                    .context("Failed to shutdown connection")?;

                // do not remove from resolver, may reconnect to the same node later
                // self.resolver.remove_node(ident).await;
            }
        }

        // now try to pop some from the join_set
        let mut join_set = self.join_set.lock().await;
        while let Some(r) = join_set.try_join_next() {
            if let Err(e) = r {
                error!(error = %e, "Failed to join connection handling task");
            }
        }
        Ok(())
    }

    async fn handle_connection(
        self: Arc<Self>,
        mut read_half: OwnedReadHalf,
        addr: SocketAddr,
    ) {
        let r = if enabled!(tracing::Level::TRACE) || self.rate_limit.is_some()
        {
            self.clone().handle_connection_tracing(&mut read_half).await
        } else {
            self.clone().handle_connection_forward(&mut read_half).await
        };

        if let Err(e) = r {
            counter!("kaze_connection_errors_total").increment(1);
            error!(error = %e, "Failed to handle connection");
        }

        if let Err(e) = self.clone().close_connection(read_half, addr).await {
            counter!("kaze_close_errors_total").increment(1);
            error!(error = %e, "Failed to close connection");
        }
    }

    async fn handle_connection_forward(
        &self,
        read_half: &mut OwnedReadHalf,
    ) -> Result<()> {
        let mut transport =
            FramedRead::new(read_half, NetPacketForwardCodec {});
        loop {
            let data = self
                .read_timeout::<NetPacketForwardCodec>(&mut transport)
                .await?;
            if let Some(mut data) = data {
                self.transfer_pkg(&mut data)
                    .await
                    .context("Failed to transfer packet")?
            } else {
                return Ok(());
            }
        }
    }

    async fn handle_connection_tracing(
        &self,
        read_half: &mut OwnedReadHalf,
    ) -> Result<()> {
        let mut transport = FramedRead::new(read_half, NetPacketCodec {});
        loop {
            let data: Option<(Hdr, BytesMut)> =
                self.read_timeout::<NetPacketCodec>(&mut transport).await?;
            if let Some((hdr, mut data)) = data {
                trace!(hdr = ?hdr, len = data.len(), "transfer packet");
                if let Some(limiter) = &self.rate_limit {
                    limiter.acquire_one(hdr.src_ident, &hdr.body_type).await;
                }
                self.transfer_pkg(&mut data)
                    .await
                    .context("Failed to transfer packet")?;
            } else {
                return Ok(());
            }
        }
    }

    async fn read_timeout<D>(
        &self,
        mut transport: impl StreamExt<Item = Result<<D as Decoder>::Item>> + Unpin,
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
            _ = tokio::time::sleep(self.idle_timeout) => {
                counter!("kaze_read_timeout_total").increment(1);
                None
            }
        })
    }

    async fn transfer_pkg(&self, data: &mut BytesMut) -> Result<()> {
        let mut sq = self.sq.lock().await;
        let mut ctx = match sq.try_push(data.len()) {
            Ok(ctx) => ctx,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                counter!("kaze_push_blocking_total").increment(1);
                block_in_place(|| sq.push(data.len())).map_err(|e| {
                    counter!("kaze_push_blocking_errors_total").increment(1);
                    anyhow!("kaze blocking push error: {}", e)
                })?
            }
            Err(e) => {
                counter!("kaze_push_errors_total").increment(1);
                bail!("kaze push error: {}", e);
            }
        };
        let len = data.len() as usize;
        let mut buf = ctx.buffer_mut();
        buf.put_u32_le(len as u32);
        buf.put(data);
        ctx.commit(len)?;
        counter!("kaze_submission_packets_total").increment(1);
        Ok(())
    }
}
