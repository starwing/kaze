use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashMap, net::SocketAddr};

use anyhow::{bail, Context, Result};
use metrics::{counter, gauge};
use tokio::{
    io::AsyncWriteExt,
    net::{tcp::OwnedReadHalf, TcpStream},
    select,
    sync::Mutex,
    task::JoinSet,
};
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;
use tracing::{error, trace};

use crate::codec::NetPacketCodec;
use crate::connection::{ReadConn, WriteConn};
use crate::edge;
use crate::ratelimit::RateLimit;
use crate::resolver::Resolver;

/// register new incomming connection into socket pool
pub struct Corral {
    sock_map: Mutex<HashMap<SocketAddr, WriteConn>>,
    pub(crate) pending_timeout: Duration,
    pub(crate) idle_timeout: Duration,
    pub(crate) rate_limit: Option<RateLimit>,
    join_set: Mutex<JoinSet<()>>,
}

impl Corral {
    /// create a new register
    pub fn new(
        rate_limit: Option<RateLimit>,
        pending_timeout: impl Into<Duration>,
        idle_timeout: impl Into<Duration>,
    ) -> Self {
        Self {
            sock_map: Mutex::new(HashMap::new()),
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
    #[tracing::instrument(level = "trace", skip(self, resolver, sender))]
    pub async fn handle_incomming(
        self: &Arc<Self>,
        stream: TcpStream,
        addr: SocketAddr,
        resolver: &Resolver,
        sender: &edge::Sender,
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
        sender.send(&mut data).await?;

        // 3. add valid connection
        self.add_connection(
            transport.into_inner(),
            hdr.src_ident,
            addr,
            resolver,
            sender,
        )
        .await;

        Ok(())
    }

    /// find a node that matches the ident
    #[tracing::instrument(level = "trace", skip(self, resolver, sender))]
    pub async fn find_or_create(
        self: &Arc<Self>,
        ident: u32,
        resolver: &Resolver,
        sender: &edge::Sender,
    ) -> Result<WriteConn> {
        if let Some(addr) = resolver.get_node(ident).await {
            if let Some(sock) = self.sock_map.lock().await.get(&addr) {
                Ok(sock.clone())
            } else {
                self.try_connect(ident, addr, resolver, sender)
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
        sender: &edge::Sender,
    ) -> Result<WriteConn> {
        let stream = TcpStream::connect(addr).await;
        match stream {
            Ok(stream) => Ok(self
                .add_connection(stream, ident, addr, resolver, sender)
                .await),
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
        sender: &edge::Sender,
    ) -> WriteConn {
        let (read_half, write_half) = stream.into_split();

        // 1. add connection to the map
        let write_conn = WriteConn::new(write_half, addr);
        resolver.add_node(ident, addr).await;
        self.sock_map.lock().await.insert(addr, write_conn.clone());

        // 2. spawn a new task to handle send to this socket
        let read_conn = ReadConn::new(read_half, addr);
        let mut join_set = self.join_set.lock().await;
        join_set.spawn(read_conn.main_loop(self.clone(), sender.clone()));
        gauge!("kaze_connections_total").increment(1);
        write_conn
    }

    pub(crate) async fn close_connection(
        &self,
        read_half: OwnedReadHalf,
        addr: SocketAddr,
    ) -> Result<()> {
        gauge!("kaze_connections_total").decrement(1);
        if let Some(write_half) = self.sock_map.lock().await.remove(&addr) {
            // if the arc's strong count is not 1, just drop it, otherwise, try to
            // shutdown the connection
            match write_half.reunite(read_half).await {
                Ok(mut stream) => {
                    stream
                        .shutdown()
                        .await
                        .context("Failed to shutdown connection")?;
                }
                Err((read_half, write_half)) => {
                    counter!("kaze_reunite_error_total").increment(1);
                    error!("Failed to reunite connection: addr={addr}");
                    // do not remove from resolver, may reconnect to the same node later
                    // self.resolver.remove_node(ident).await;
                    drop(read_half);
                    drop(write_half);
                }
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
}
