use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context as _, Error, Result};
use kaze_protocol::{
    codec::NetPacketCodec, message::PacketWithAddr, packet::BytesPool,
};
use lru::LruCache;
use metrics::{counter, gauge};
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    select,
    sync::{Mutex, Notify},
    task::JoinSet,
};
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;
use tower::Service;
use tracing::{error, instrument};

use kaze_util::singleflight::Group;
use kaze_util::tower_ext::ServiceExt as _;

use super::options::Options;

pub trait CorralSink:
    Service<
        PacketWithAddr,
        Response = (),
        Error: Into<anyhow::Error> + Sync + Send + 'static,
        Future: Send,
    > + Send
    + 'static
{
}

impl<S> CorralSink for S where
    S: Service<
            PacketWithAddr,
            Response = (),
            Error: Into<anyhow::Error> + Sync + Send + 'static,
            Future: Send,
        > + Send
        + 'static
{
}

type ConnSink = Arc<Mutex<OwnedWriteHalf>>;

pub struct Corral<S> {
    options: Options,
    decoder: NetPacketCodec,
    sink: S,
    group: Group<SocketAddr, ConnSink, Error>,
    sock_map: Mutex<LruCache<SocketAddr, ConnSink>>,
    join_set: Mutex<JoinSet<()>>,
    exit: Notify,
}

unsafe impl<S> Send for Corral<S> where S: Send {}
unsafe impl<S> Sync for Corral<S> where S: Send {}

impl<S> Corral<S> {
    pub fn new(options: Options, pool: BytesPool, sink: S) -> Self {
        let limit = options.limit;
        Self {
            options,
            decoder: NetPacketCodec::new(pool),
            sink,
            group: Group::new(),
            sock_map: Mutex::new(
                limit
                    .and_then(NonZeroUsize::new)
                    .map(|l| LruCache::new(l))
                    .unwrap_or(LruCache::unbounded()),
            ),
            join_set: Mutex::new(JoinSet::new()),
            exit: Notify::new(),
        }
    }

    /// get the sink
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// notify all tasks to exit
    pub fn notify_exit(&self) {
        self.exit.notify_waiters();
    }

    /// wait for all tasks to exit
    pub async fn wait_exit(&self) {
        self.exit.notified().await;
    }

    /// gracefully exit
    pub async fn graceful_exit(self: Arc<Self>) -> Result<()> {
        // wait for all tasks to finish
        let mut join_set = self.join_set.lock().await;
        while let Some(res) = join_set.join_next().await {
            if let Err(e) = res {
                error!(error = %e, "Failed to join connection handling task");
            }
        }
        Ok(())
    }

    /// get the connection idle timeout
    pub fn idle_timeout(&self) -> Duration {
        self.options.idle_timeout.into()
    }

    /// get the connection pending timeout
    pub fn pending_timeout(&self) -> Duration {
        self.options.pending_timeout.into()
    }

    /// get the bytes pool
    pub fn bytes_pool(&self) -> &BytesPool {
        self.decoder.pool()
    }

    /// remove the connection
    async fn remove_connection(self: Arc<Self>, addr: SocketAddr) {
        self.clone().sock_map.lock().await.pop(&addr);
        gauge!("kaze_connections_total").decrement(1);
    }
}

impl<S: CorralSink + Clone> Corral<S> {
    pub async fn find_or_connect(
        self: &Arc<Self>,
        addr: SocketAddr,
    ) -> Result<Option<ConnSink>> {
        if let Some(sock) = self.sock_map.lock().await.get(&addr) {
            return Ok(Some(sock.clone()));
        }
        match self.group.work(addr, self.connect(addr)).await {
            Ok(Ok(conn)) => Ok(Some(conn)),
            Err(Some(conn)) => Ok(Some(conn)),
            Err(None) => Ok(None),
            Ok(Err(err)) => {
                bail!("Failed to connect to addr={}: {}", addr, err)
            }
        }
    }

    pub async fn add_connection(
        self: &Arc<Self>,
        conn: TcpStream,
        addr: SocketAddr,
    ) -> Result<ConnSink> {
        self.add_connection_with_pending(
            conn,
            addr,
            Some(self.pending_timeout()),
        )
        .await
    }

    #[instrument(level = "trace", skip(self))]
    async fn connect(self: &Arc<Self>, addr: SocketAddr) -> Result<ConnSink> {
        let sock = tokio::net::TcpStream::connect(addr)
            .await
            .context("Failed to connect")?;
        self.add_connection_with_pending(sock, addr, None).await
    }

    async fn add_connection_with_pending(
        self: &Arc<Self>,
        conn: TcpStream,
        addr: SocketAddr,
        pending: Option<Duration>,
    ) -> Result<ConnSink> {
        let (read_half, write_half) = conn.into_split();
        let conn = ConnSource::new(read_half, addr, self.clone());
        let fut = conn.main_loop(pending);
        self.join_set.lock().await.spawn(fut);
        gauge!("kaze_connections_total").increment(1);

        let conn = Arc::new(Mutex::new(write_half));
        self.sock_map.lock().await.put(addr, conn.clone());
        Ok(conn)
    }
}

pub struct ConnSource<S: CorralSink> {
    inner: FramedRead<OwnedReadHalf, NetPacketCodec>,
    addr: SocketAddr,
    corral: Arc<Corral<S>>,
}

impl<S: CorralSink> Drop for ConnSource<S> {
    fn drop(&mut self) {
        let corral = self.corral.clone();
        let addr = self.addr;
        tokio::spawn(async move {
            corral.remove_connection(addr).await;
        });
    }
}

impl<S: CorralSink> ConnSource<S> {
    fn new(
        read_half: OwnedReadHalf,
        addr: SocketAddr,
        corral: Arc<Corral<S>>,
    ) -> Self {
        Self {
            inner: FramedRead::new(read_half, corral.decoder.clone()),
            addr,
            corral,
        }
    }
}

impl<S: CorralSink + Clone> ConnSource<S> {
    #[instrument(level = "trace", skip(self))]
    async fn main_loop(self, pending: Option<Duration>) {
        if let Err(e) = self.main_loop_inner(pending).await {
            error!(error = %e, "Failed to handle connection");
        }
    }

    async fn main_loop_inner(
        mut self,
        pending: Option<Duration>,
    ) -> Result<()> {
        if !self
            .first_request(pending)
            .await
            .context("Failed to handle first request")?
        {
            return Ok(());
        }
        let timeout = self.corral.idle_timeout();
        let mut sink = self.corral.sink.clone();
        while let Ok(pkg) = select! {
            pkg = tokio::time::timeout(timeout, self.inner.next()) => pkg,
            _ = self.corral.wait_exit() => return Ok(()),
        } {
            let Some(pkg) = pkg else {
                counter!("kaze_read_closed_total").increment(1);
                return Ok(());
            };
            sink.ready_call((
                pkg.context("Failed to read packet")?,
                Some(self.addr),
            ))
            .await
            .map_err(Into::into)
            .context("Failed to transfer packet")?;
        }
        counter!("kaze_read_idle_timeout_total").increment(1);
        Ok(())
    }

    async fn first_request(
        &mut self,
        pending: Option<Duration>,
    ) -> Result<bool> {
        let Some(pending) = pending else {
            // do not wait for the first request
            return Ok(true);
        };
        let pkg = select! {
            pkg = tokio::time::timeout(pending, self.inner.next()) => pkg,
            _ = self.corral.wait_exit() => return Ok(false)
        };
        if let Err(_) = pkg {
            // timeout
            counter!("kaze_read_pending_timeout_total").increment(1);
            return Ok(false);
        }
        let Ok(Some(pkg)) = pkg else {
            // connection closed
            counter!("kaze_read_closed_total").increment(1);
            return Ok(false);
        };
        let mut sink = self.corral.sink.clone();
        sink.ready_call((
            pkg.context("Failed to read packet")?,
            Some(self.addr),
        ))
        .await
        .map_err(Into::into)
        .context("Failed to sink packet")?;
        return Ok(true);
    }
}
