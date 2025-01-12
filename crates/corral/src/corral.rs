use std::time::Duration;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use metrics::{counter, gauge};
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::TcpStream;
use tokio::select;
use tokio::{
    net::TcpListener,
    sync::{Mutex, Notify},
    task::JoinSet,
};
use tokio_util::bytes::Buf;
use tokio_util::codec::FramedRead;
use tracing::{error, info, trace};

use kaze_edge::Receiver;
use kaze_protocol::{NetPacketCodec, Packet, RetCode};
use kaze_resolver::Resolver;

use crate::rpc_hub::{Direction, RpcHub};
use crate::{dispatcher::Dispatcher, ReadConn, WriteConn};
use crate::{options::Options, Builder, RateLimit};

/// corral: the incomming Socket manager
pub struct Corral<R: Resolver> {
    options: Options,
    sock_map: Mutex<HashMap<SocketAddr, WriteConn>>,
    join_set: Mutex<JoinSet<()>>,
    rate_limit: Option<RateLimit>,
    resolver: R,
    dispatcher: Dispatcher,
    rpc_hub: RpcHub,
    sender: kaze_edge::Sender,
    exit: Notify,
}

/// creation and exit
impl<R: Resolver> Corral<R> {
    /// create a new corral object
    pub(crate) fn new(builder: Builder<R>, ident: u32) -> Self {
        Self {
            options: builder.options,
            sock_map: Mutex::new(HashMap::new()),
            join_set: Mutex::new(JoinSet::new()),
            rate_limit: builder.rate_limit,
            resolver: builder.resolver,
            dispatcher: Dispatcher::new(),
            rpc_hub: RpcHub::new(ident),
            sender: builder.sender,
            exit: Notify::new(),
        }
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
        // shutdown submission queue, do not allow new requests
        self.close_sender().await;
        info!("submission queue closed");

        // wait for all tasks to finish
        let mut join_set = self.join_set.lock().await;
        while let Some(res) = join_set.join_next().await {
            if let Err(e) = res {
                error!(error = %e, "Failed to join connection handling task");
            }
        }
        Ok(())
    }

    pub async fn close_sender(self: &Arc<Self>) {
        self.sender.lock().await;
    }
}

/// send packet to edge
impl<R: Resolver> Corral<R> {
    /// send packet into host, record the RPC info.
    ///
    /// `data` is the packet data to send, inclding the length prefixed header
    /// and the body.
    pub async fn send(&self, data: &Packet<'_>) -> Result<()> {
        // TODO should remove clone
        self.rpc_hub
            .record(data.hdr().clone(), Direction::ToHost)
            .await;
        self.raw_send(&mut data.as_buf()).await
    }

    /// send packet into host and do not record the RPC info.
    ///
    /// `data` is the packet data to send, inclding the length prefixed header
    /// and the body.
    pub async fn raw_send(&self, data: impl Buf) -> Result<()> {
        self.sender.send(data).await
    }

    /// send packet into connections with dispatcher
    ///
    /// `data` is the packet data to send, inclding the length prefixed header
    /// and the body.
    pub async fn dispatch(self: &Arc<Self>, data: &Packet<'_>) -> Result<()> {
        self.dispatcher.dispatch(data, &self).await
    }

    #[tracing::instrument(level = "trace", skip(self, receiver))]
    pub async fn handle_completion(
        self: &Arc<Self>,
        mut receiver: Receiver,
    ) -> Result<()> {
        while let Some(ctx) = receiver.recv().await? {
            let data = Packet::from_host(ctx.buffer())?;
            if let Err(e) = self.dispatch(&data).await {
                counter!("kaze_dispatch_errors_total").increment(1);
                error!("Error dispatching packet: {e}");
                // continue running
            }
            trace!(hdr = ?data.hdr(), len = data.body_len(), "dispatch packet");
            ctx.commit();
            counter!("kaze_completion_packets_total").increment(1);
        }
        Ok(())
    }
}

/// connection manager
impl<R: Resolver> Corral<R> {
    /// handle listener
    #[tracing::instrument(level = "trace", skip(self, listener))]
    pub async fn handle_listener(
        self: &Arc<Self>,
        listener: &TcpListener,
    ) -> Result<()> {
        loop {
            let (socket, addr) = select! {
                _ = self.wait_exit() => {
                    info!("stop listening");
                    return Ok(());
                },
                r = listener.accept() => r?,
            };
            info!(addr = %addr, "Accepted connection");
            self.handle_incomming(socket, addr).await?;
        }
    }

    /// handle incomming connection
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn handle_incomming(
        self: &Arc<Self>,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Result<()> {
        let mut transport = FramedRead::new(stream, NetPacketCodec {});

        // 1. waiting for the first packet to read
        let data = select! {
            pkg = transport.next() => if let Some(pkg) = pkg { pkg? }
                else {
                    return Ok(());
                },
            _ = tokio::time::sleep(self.options.pending_timeout.into()) => {
                counter!("kaze_read_timeout_total").increment(1);
                return Ok(());
            }
        };

        // 2. transfer the packet
        trace!(hdr = ?data.hdr(), len = data.body_len(), "transfer packet");
        self.send(&data).await?;

        // 3. add valid connection
        self.add_connection(
            transport.into_inner(),
            data.hdr().src_ident,
            addr,
        )
        .await;

        Ok(())
    }

    /// find a node that matches the ident
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn find_or_connect(
        self: &Arc<Self>,
        ident: u32,
    ) -> Result<Option<WriteConn>> {
        let r = if let Some(addr) = self.resolver.get_node(ident).await {
            Some(if let Some(sock) = self.sock_map.lock().await.get(&addr) {
                sock.clone()
            } else {
                self.try_connect(ident, addr)
                    .await
                    .context("Failed to connect")?
            })
        } else {
            None
        };
        Ok(r)
    }

    async fn try_connect(
        self: &Arc<Self>,
        ident: u32,
        addr: SocketAddr,
    ) -> Result<WriteConn> {
        let stream = TcpStream::connect(addr).await;
        match stream {
            Ok(stream) => Ok(self.add_connection(stream, ident, addr).await),
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
    ) -> WriteConn {
        let (read_half, write_half) = stream.into_split();

        // 1. add connection to the map
        let write_conn = WriteConn::new(write_half, addr);
        self.resolver.add_node(ident, addr).await;
        self.sock_map.lock().await.insert(addr, write_conn.clone());

        // 2. spawn a new task to handle send to this socket
        let read_conn = ReadConn::new(read_half, addr);
        let mut join_set = self.join_set.lock().await;
        join_set.spawn(read_conn.main_loop(self.clone()));
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

/// rpc manager
impl<R: Resolver> Corral<R> {
    /// handle rpc
    pub async fn handle_rpc(self: &Arc<Self>) -> Result<()> {
        while let Some(rpc) = select! {
            rpc = self.rpc_hub.poll() => rpc,
            _ = self.wait_exit() => return Ok(()),
        } {
            let packet =
                Packet::from_retcode(rpc.hdr.clone(), RetCode::RetTimeout)?;
            match rpc.direction {
                Direction::ToHost => {
                    counter!("kaze_rpc_to_host_timeout_total").increment(1);
                    self.send(&packet).await?;
                }
                Direction::ToNode => {
                    counter!("kaze_rpc_to_node_timeout_total").increment(1);
                    self.dispatch(&packet).await?;
                }
            }
        }
        Ok(())
    }
}

/// utils
impl<R: Resolver> Corral<R> {
    /// get the connection idle timeout
    pub fn idle_timeout(&self) -> Duration {
        self.options.idle_timeout.into()
    }

    /// get the connection pending timeout
    pub fn pending_timeout(&self) -> Duration {
        self.options.pending_timeout.into()
    }

    /// get ident
    pub fn local_ident(&self) -> u32 {
        self.rpc_hub.local_ident()
    }

    /// get resolver
    pub fn resolver(&self) -> &R {
        &self.resolver
    }

    /// waiting for sending rate limit
    pub async fn acqure_token(&self, ident: u32, body_type: &String) {
        if let Some(limiter) = &self.rate_limit {
            limiter.acquire_one(ident, body_type).await;
        }
    }
}
