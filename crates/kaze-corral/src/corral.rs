use {
    anyhow::{bail, Context, Error, Result},
    futures::{Sink, SinkExt, StreamExt},
    lru::LruCache,
    metrics::{counter, gauge},
    pin_project_lite::pin_project,
    std::{
        error::Error as StdError, net::SocketAddr, num::NonZeroUsize,
        sync::Arc, time::Duration,
    },
    tokio::{
        net::{
            tcp::{OwnedReadHalf, OwnedWriteHalf},
            TcpStream,
        },
        select,
        sync::{Mutex, Notify},
        task::JoinSet,
    },
    tokio_util::codec::{Decoder, Encoder, FramedRead, FramedWrite},
    tracing::{error, instrument},
};

use kaze_util::singleflight::Group;

use super::options::Options;

type ConnValue<T, E> = Arc<Mutex<ConnSink<T, E>>>;

pub struct Corral<Item, Codec, S> {
    options: Options,
    codec: Codec,
    sink: S,
    group: Group<SocketAddr, ConnValue<Item, Codec>, Error>,
    sock_map: Arc<Mutex<LruCache<SocketAddr, ConnValue<Item, Codec>>>>,
    join_set: Mutex<JoinSet<()>>,
    exit: Notify,
}

impl<Item, Codec, S> Corral<Item, Codec, S> {
    pub fn new(options: Options, codec: Codec, sink: S) -> Self {
        let limit = options.limit;
        Self {
            options,
            sink,
            codec,
            group: Group::new(),
            sock_map: Arc::new(Mutex::new(
                limit
                    .and_then(NonZeroUsize::new)
                    .map(|l| LruCache::new(l))
                    .unwrap_or(LruCache::unbounded()),
            )),
            join_set: Mutex::new(JoinSet::new()),
            exit: Notify::new(),
        }
    }

    /// get the sink
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// get the mutable sink
    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
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
}

impl<Item: Send + 'static, Codec: Send + 'static, S> Corral<Item, Codec, S> {
    fn remove_connection(&self, addr: SocketAddr) {
        let sock_map = self.sock_map.clone();
        tokio::spawn(async move {
            sock_map.lock().await.pop(&addr);
        });
        gauge!("kaze_connections_total").decrement(1);
    }
}

type ItemWithAddr<Item> = (Item, Option<SocketAddr>);

impl<Item, Codec, S> Corral<Item, Codec, S>
where
    Item: Send + 'static,
    Codec:
        Encoder<Item> + Decoder<Item = Item> + Clone + Sync + Send + 'static,
    S: Sink<ItemWithAddr<Item>> + Clone + Send + Sync + Unpin + 'static,
    <Codec as Decoder>::Error: StdError + Sync + Send + 'static,
    <S as Sink<ItemWithAddr<Item>>>::Error: StdError + Sync + Send + 'static,
{
    pub async fn find_or_connect(
        self: &Arc<Self>,
        addr: SocketAddr,
    ) -> Result<Option<ConnValue<Item, Codec>>> {
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
    ) -> Result<ConnValue<Item, Codec>> {
        self.add_connection_with_pending(
            conn,
            addr,
            Some(self.pending_timeout()),
        )
        .await
    }

    #[instrument(level = "trace", skip(self))]
    async fn connect(
        self: &Arc<Self>,
        addr: SocketAddr,
    ) -> Result<ConnValue<Item, Codec>> {
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
    ) -> Result<ConnValue<Item, Codec>> {
        let (read_half, write_half) = conn.into_split();
        let conn =
            ConnSource::new(read_half, self.codec.clone(), addr, self.clone());
        let fut = conn.main_loop(pending);
        self.join_set.lock().await.spawn(fut);
        gauge!("kaze_connections_total").increment(1);

        let conn = Arc::new(Mutex::new(ConnSink::new(
            write_half,
            self.codec.clone(),
        )));
        self.sock_map.lock().await.put(addr, conn.clone());
        Ok(conn)
    }
}

pin_project! {
    pub struct ConnSink<Item, E> {
        #[pin]
        inner: FramedWrite<OwnedWriteHalf, E>,
        _marker: std::marker::PhantomData<Item>,
    }
}

impl<Item, E: Encoder<Item>> ConnSink<Item, E> {
    pub fn new(conn: OwnedWriteHalf, encoder: E) -> Self {
        Self {
            inner: FramedWrite::new(conn, encoder),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Item, E: Encoder<Item>> Sink<Item> for ConnSink<Item, E> {
    type Error = E::Error;

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(
        self: std::pin::Pin<&mut Self>,
        item: Item,
    ) -> std::result::Result<(), Self::Error> {
        self.project().inner.start_send(item)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

pub struct ConnSource<Item: Send + 'static, D: Send + 'static, S> {
    inner: FramedRead<OwnedReadHalf, D>,
    addr: SocketAddr,
    corral: Arc<Corral<Item, D, S>>,
}

impl<Item: Send + 'static, D: Send + 'static, S> Drop
    for ConnSource<Item, D, S>
{
    fn drop(&mut self) {
        self.corral.remove_connection(self.addr);
    }
}

impl<Item: Send + 'static, D: Decoder + Send + 'static, S>
    ConnSource<Item, D, S>
{
    fn new(
        read_half: OwnedReadHalf,
        decoder: D,
        addr: SocketAddr,
        corral: Arc<Corral<Item, D, S>>,
    ) -> Self {
        Self {
            inner: FramedRead::new(read_half, decoder),
            addr,
            corral,
        }
    }
}

impl<Item, D, S> ConnSource<Item, D, S>
where
    Item: Send + 'static,
    D: Decoder<Item = Item> + Encoder<Item> + Send + 'static,
    S: Sink<ItemWithAddr<Item>> + Unpin + Clone,
    <D as Decoder>::Error: StdError + Sync + Send + 'static,
    <S as Sink<ItemWithAddr<Item>>>::Error: StdError + Sync + Send + 'static,
{
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
            sink.send((
                pkg.context("Failed to read packet")?,
                Some(self.addr),
            ))
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
        self.corral
            .sink
            .clone()
            .send((pkg.context("Failed to read packet")?, Some(self.addr)))
            .await
            .context("Failed to sink packet")?;
        return Ok(true);
    }
}
