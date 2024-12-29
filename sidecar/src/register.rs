use std::sync::Arc;
use std::{collections::HashMap, net::SocketAddr};

use anyhow::{anyhow, bail, Context, Error, Result};
use bytes::{BufMut, BytesMut};
use metrics::{counter, gauge};
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;
use tokio::task::block_in_place;
use tokio::{net::TcpStream, select};
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, FramedRead};
use tracing::{enabled, instrument, trace};

use crate::codec::{NetPacketCodec, NetPacketForwardCodec};
use crate::resolver::Resolver;

/// register new incomming connection into socket pool
pub struct Register {
    sock_map: Mutex<HashMap<SocketAddr, Arc<Mutex<OwnedWriteHalf>>>>,
    sq: Mutex<kaze_core::KazeState>,
    pending_timeout: u64,
    idle_timeout: u64,
}

impl Register {
    /// create a new register
    pub fn new(
        sq: kaze_core::KazeState,
        pending_timeout: u64,
        idle_timeout: u64,
    ) -> Self {
        Self {
            sock_map: Mutex::new(HashMap::new()),
            sq: Mutex::new(sq),
            pending_timeout,
            idle_timeout,
        }
    }

    /// handle incomming connection
    #[instrument(level = "trace", skip(self, resolver))]
    pub async fn handle_incomming(
        self: &Arc<Self>,
        resolver: &Resolver,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Result<()> {
        let mut transport = FramedRead::new(stream, NetPacketCodec {});

        // 1. waiting for the first packet to read
        let (hdr, mut data) = select! {
            pkg = transport.next() => if let Some(pkg) = pkg { pkg? }
                else {
                    println!("exit");
                    return Ok(());
                },
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(self.pending_timeout)) => {
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
            resolver,
            hdr.src_ident,
            transport.into_inner(),
            addr,
        )
        .await;

        Ok(())
    }

    /// find a node that matches the ident
    #[instrument(level = "trace", skip(self, resolver))]
    pub async fn find_socket(
        self: &Arc<Self>,
        resolver: &Resolver,
        ident: u32,
    ) -> Result<Arc<Mutex<OwnedWriteHalf>>> {
        if let Some(addr) = resolver.get_node(ident).await {
            if let Some(sock) = self.sock_map.lock().await.get(&addr) {
                Ok(sock.clone())
            } else {
                self.try_connect(resolver, ident, addr)
                    .await
                    .context("Failed to connect")
            }
        } else {
            bail!("node not found ident={}", ident);
        }
    }

    async fn try_connect(
        self: &Arc<Self>,
        resolver: &Resolver,
        ident: u32,
        addr: SocketAddr,
    ) -> Result<Arc<Mutex<OwnedWriteHalf>>> {
        let stream = TcpStream::connect(addr).await;
        match stream {
            Ok(stream) => {
                Ok(self.add_connection(resolver, ident, stream, addr).await)
            }
            Err(e) => {
                counter!("kaze_connect_errors_total").increment(1);
                bail!("connect error: {}", e);
            }
        }
    }

    async fn add_connection(
        self: &Arc<Self>,
        resolver: &Resolver,
        ident: u32,
        conn: TcpStream,
        addr: SocketAddr,
    ) -> Arc<Mutex<OwnedWriteHalf>> {
        let (read_half, write_half) = conn.into_split();
        let write_half = Arc::new(Mutex::new(write_half));

        // 1. add connection to the map
        resolver.add_node(ident, addr).await;
        self.sock_map.lock().await.insert(addr, write_half.clone());

        // 2. spawn a new task to handle send to this socket
        if enabled!(tracing::Level::TRACE) {
            tokio::spawn(
                self.clone().handle_connection_tracing(read_half, addr),
            );
        } else {
            tokio::spawn(self.clone().handle_connection(read_half, addr));
        }
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
        Ok(())
    }

    async fn handle_connection(
        self: Arc<Self>,
        read_half: OwnedReadHalf,
        addr: SocketAddr,
    ) -> Result<()> {
        let mut transport =
            Some(FramedRead::new(read_half, NetPacketForwardCodec {}));
        loop {
            let data = self.read_timeout(&mut transport, addr).await?;
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
        self: Arc<Self>,
        read_half: OwnedReadHalf,
        addr: SocketAddr,
    ) -> Result<()> {
        let mut transport =
            Some(FramedRead::new(read_half, NetPacketCodec {}));
        loop {
            let data = self.read_timeout(&mut transport, addr).await?;
            if let Some((hdr, mut data)) = data {
                trace!(hdr = ?hdr, len = data.len(), "transfer packet");
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
        transport: &mut Option<FramedRead<OwnedReadHalf, D>>,
        addr: SocketAddr,
    ) -> Result<Option<<D as Decoder>::Item>>
    where
        D: Decoder,
        <D as Decoder>::Error: Into<Error>,
    {
        let pkg = select! {
            pkg = transport.as_mut().unwrap().next() => {
                if let Some(pkg) = pkg {
                    Some(pkg.map_err(Into::into)?)
                } else {
                    counter!("kaze_read_closed_total").increment(1);
                    None
                }
            },
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(self.idle_timeout)) => {
                counter!("kaze_read_timeout_total").increment(1);
                None
            }
        };
        if pkg.is_none() {
            self.close_connection(
                transport.take().unwrap().into_inner(),
                addr,
            )
            .await
            .context("Failed to close connection")?;
        }
        Ok(pkg)
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
