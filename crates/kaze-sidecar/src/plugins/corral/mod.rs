mod options;
mod service;

pub use options::Options;

use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;
use std::{net::SocketAddr, sync::OnceLock};

use anyhow::{bail, Context as _, Error, Result};
use lru::LruCache;
use metrics::{counter, gauge};
use tokio::net::TcpListener;
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    select,
    sync::Mutex,
    task::JoinSet,
};
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;
use tracing::{error, info, instrument};

use kaze_plugin::{
    protocol::{codec::NetPacketCodec, packet::BytesPool},
    util::singleflight::Group,
    Context, Plugin,
};

type ConnSink = Arc<Mutex<OwnedWriteHalf>>;

#[derive(Clone)]
pub struct Corral {
    inner: Arc<Inner>,
}

struct Inner {
    ctx: OnceLock<Context>,
    options: Options,
    decoder: OnceLock<NetPacketCodec>,
    group: Group<SocketAddr, ConnSink, Error>,
    sock_map: Mutex<LruCache<SocketAddr, ConnSink>>,
    join_set: Mutex<JoinSet<()>>,
}

impl Plugin for Corral {
    fn init(&self, ctx: Context) {
        self.inner
            .decoder
            .set(NetPacketCodec::new(ctx.pool().clone()))
            .unwrap();
        self.inner.ctx.set(ctx).unwrap();
    }
    fn context_storage(&self) -> Option<&OnceLock<Context>> {
        Some(&self.inner.ctx)
    }
    fn run(
        &self,
    ) -> Option<
        std::pin::Pin<
            Box<dyn Future<Output = anyhow::Result<()>> + Send + 'static>,
        >,
    > {
        let corral = self.clone();
        Some(Box::pin(async move {
            let listener =
                TcpListener::bind(&corral.inner.options.listen).await?;
            info!(addr = %corral.inner.options.listen, "Corral listening");
            loop {
                let (conn, addr) = select! {
                    r = listener.accept() => r?,
                    _ = corral.context().exiting() => break,
                };
                corral.add_connection(conn, addr).await?;
            }
            info!("Corral exiting");
            corral.graceful_exit().await?;
            info!("Corral exited");
            Ok(())
        }))
    }
}

impl Corral {
    fn new(options: &Options) -> Self {
        let limit = options.limit;
        Self {
            inner: Arc::new(Inner {
                ctx: OnceLock::new(),
                options: options.clone(),
                decoder: OnceLock::new(),
                group: Group::new(),
                sock_map: Mutex::new(
                    limit
                        .and_then(NonZeroUsize::new)
                        .map(|l| LruCache::new(l))
                        .unwrap_or(LruCache::unbounded()),
                ),
                join_set: Mutex::new(JoinSet::new()),
            }),
        }
    }

    /// gracefully exit
    async fn graceful_exit(&self) -> Result<()> {
        // wait for all tasks to finish
        let mut join_set = self.inner.join_set.lock().await;
        while let Some(res) = join_set.join_next().await {
            if let Err(e) = res {
                error!(error = %e, "Failed to join connection handling task");
            }
        }
        Ok(())
    }

    /// get the connection idle timeout
    pub fn idle_timeout(&self) -> Duration {
        self.inner.options.idle_timeout.into()
    }

    /// get the connection pending timeout
    pub fn pending_timeout(&self) -> Duration {
        self.inner.options.pending_timeout.into()
    }

    /// get the bytes pool
    pub fn bytes_pool(&self) -> &BytesPool {
        self.context().pool()
    }

    /// remove the connection
    async fn remove_connection(&self, addr: SocketAddr) {
        self.clone().inner.sock_map.lock().await.pop(&addr);
        gauge!("kaze_connections_total").decrement(1);
    }
}

impl Corral {
    pub async fn find_or_connect(
        &self,
        addr: SocketAddr,
    ) -> Result<Option<ConnSink>> {
        if let Some(sock) = self.inner.sock_map.lock().await.get(&addr) {
            return Ok(Some(sock.clone()));
        }
        match self.inner.group.work(addr, self.connect(addr)).await {
            Ok(Ok(conn)) => Ok(Some(conn)),
            Err(Some(conn)) => Ok(Some(conn)),
            Err(None) => Ok(None),
            Ok(Err(err)) => {
                bail!("Failed to connect to addr={}: {}", addr, err)
            }
        }
    }

    pub async fn add_connection(
        &self,
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
    async fn connect(&self, addr: SocketAddr) -> Result<ConnSink> {
        let sock = tokio::net::TcpStream::connect(addr)
            .await
            .context("Failed to connect")?;
        self.add_connection_with_pending(sock, addr, None).await
    }

    async fn add_connection_with_pending(
        &self,
        conn: TcpStream,
        addr: SocketAddr,
        pending: Option<Duration>,
    ) -> Result<ConnSink> {
        let (read_half, write_half) = conn.into_split();
        let conn = ConnSource::new(read_half, addr, self.clone());
        let fut = conn.main_loop(pending);
        self.inner.join_set.lock().await.spawn(fut);
        gauge!("kaze_connections_total").increment(1);

        let conn = Arc::new(Mutex::new(write_half));
        self.inner.sock_map.lock().await.put(addr, conn.clone());
        Ok(conn)
    }
}

pub struct ConnSource {
    inner: FramedRead<OwnedReadHalf, NetPacketCodec>,
    addr: SocketAddr,
    corral: Corral,
}

impl Drop for ConnSource {
    fn drop(&mut self) {
        let corral = self.corral.clone();
        let addr = self.addr;
        tokio::spawn(async move {
            corral.remove_connection(addr).await;
        });
    }
}

impl ConnSource {
    fn new(
        read_half: OwnedReadHalf,
        addr: SocketAddr,
        corral: Corral,
    ) -> Self {
        Self {
            inner: FramedRead::new(
                read_half,
                corral.inner.decoder.get().unwrap().clone(),
            ),
            addr,
            corral,
        }
    }
}

impl ConnSource {
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
        let ctx = self.corral.context();
        let timeout = self.corral.idle_timeout();
        while let Ok(pkg) = select! {
            pkg = tokio::time::timeout(timeout, self.inner.next()) => pkg,
            _ = ctx.shutdwon_triggered() => return Ok(()),
        } {
            let Some(pkg) = pkg else {
                counter!("kaze_read_closed_total").increment(1);
                return Ok(());
            };
            ctx.send((pkg.context("Failed to read packet")?, Some(self.addr)))
                .await
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
            _ = self.corral.context().shutdwon_triggered() => return Ok(false)
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
        self.corral
            .context()
            .send((pkg.context("Failed to read packet")?, Some(self.addr)))
            .await
            .context("Failed to transfer packet")?;
        return Ok(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaze_plugin::PluginFactory as _;
    use std::net::SocketAddr;
    use std::str::FromStr;
    use tokio::net::TcpStream;
    use tokio::time::timeout;

    fn create_test_corral() -> Corral {
        let corral = Options {
            listen: "127.0.0.1:0".into(),
            idle_timeout: Duration::from_secs(1).into(),
            pending_timeout: Duration::from_secs(1).into(),
            limit: Some(100),
        }
        .build()
        .unwrap();
        Context::builder().register(corral.clone()).build();
        corral
    }

    #[tokio::test]
    async fn test_find_or_connect_nonexistent() {
        let corral = create_test_corral();

        // Attempt to connect to a non-listening port
        let addr = SocketAddr::from_str("127.0.0.1:1").unwrap();
        let result = corral.find_or_connect(addr).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_connection_cache() {
        let corral = create_test_corral();

        // Create a server that we can connect to
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        // Connect to the server
        let conn1 = TcpStream::connect(server_addr).await.unwrap();
        let _conn_sink = corral
            .clone()
            .add_connection(conn1, server_addr)
            .await
            .unwrap();

        // Verify the connection is in the cache
        let cached_conn = corral.find_or_connect(server_addr).await.unwrap();
        assert!(cached_conn.is_some());
    }

    #[tokio::test]
    async fn test_connection_limit() {
        let corral = Options {
            listen: "127.0.0.1:0".into(),
            idle_timeout: Duration::from_secs(1).into(),
            pending_timeout: Duration::from_secs(1).into(),
            limit: Some(2), // Set a very small limit
        }
        .build()
        .unwrap();

        // Mock context initialization
        Context::builder().register(corral.clone()).build();

        // Create a server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        // Add three connections - should evict the first one due to LRU
        let conn1 = TcpStream::connect(server_addr).await.unwrap();
        let addr1 = SocketAddr::from_str("127.0.0.1:10001").unwrap();
        corral
            .add_connection_with_pending(conn1, addr1, None)
            .await
            .unwrap();

        let conn2 = TcpStream::connect(server_addr).await.unwrap();
        let addr2 = SocketAddr::from_str("127.0.0.1:10002").unwrap();
        corral
            .add_connection_with_pending(conn2, addr2, None)
            .await
            .unwrap();

        let conn3 = TcpStream::connect(server_addr).await.unwrap();
        let addr3 = SocketAddr::from_str("127.0.0.1:10003").unwrap();
        corral
            .add_connection_with_pending(conn3, addr3, None)
            .await
            .unwrap();

        // Check that addr1 was evicted
        let mut cache = corral.inner.sock_map.lock().await;
        assert!(cache.get(&addr2).is_some());
        assert!(cache.get(&addr3).is_some());
        assert!(cache.get(&addr1).is_none());
    }

    #[tokio::test]
    async fn test_pending_timeout() {
        let corral = create_test_corral();

        // Create a server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        // Connect to the server with a very short pending timeout
        let conn = TcpStream::connect(server_addr).await.unwrap();
        let pending_timeout = Duration::from_millis(10);

        // This should time out because no data is received
        let result = timeout(
            Duration::from_millis(100),
            corral.add_connection_with_pending(
                conn,
                server_addr,
                Some(pending_timeout),
            ),
        )
        .await
        .unwrap();

        // The connection should still be added
        assert!(result.is_ok());
    }
}
