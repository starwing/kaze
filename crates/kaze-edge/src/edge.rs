use std::{borrow::Cow, net::Ipv4Addr, sync::Arc};

use anyhow::{Context, Result, bail};
use metrics::counter;
use tokio::{sync::Mutex, task::block_in_place};
use tower::{Layer, layer::layer_fn, service_fn};
use tracing::{info, warn};

use kaze_core::{Channel, OwnedWriteHalf};
use kaze_protocol::{
    bytes::Buf, message::Message, packet::BytesPool, service::MessageService,
};
use kaze_util::tower_ext::ServiceExt as _;

pub use kaze_core::Error;
pub use kaze_core::OwnedReadHalf as Receiver;

pub struct Edge {
    channel: Channel,
    ident: Ipv4Addr,
}

impl std::fmt::Display for Edge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Edge")
            .field("channel", &self.channel)
            .finish()
    }
}

impl Edge {
    pub(crate) fn new(
        prefix: impl AsRef<str>,
        ident: Ipv4Addr,
        bufsize: usize,
        unlink: bool,
    ) -> Result<Self> {
        let name = Self::get_channel_name(prefix, ident);

        if let Some((owner, user)) =
            Channel::exists(&name).context("Failed to check shm queue")?
        {
            if !unlink {
                bail!(
                    "shm queue {} already exists, previous channel owner={} user={}",
                    name,
                    owner,
                    user,
                );
            } else if let Err(e) = Channel::unlink(&name) {
                warn!(error = %e, "Failed to unlink channel");
            }
        }

        let page_size = page_size::get();
        let bufsize = Channel::aligned(bufsize, page_size);
        let channel = Channel::create(&name, bufsize)
            .context("Failed to create submission queue")?;
        Ok(Self { channel, ident })
    }

    /// Get the channel name.
    pub fn name(&self) -> Cow<'_, str> {
        self.channel.name()
    }

    /// Get the channel ident.
    pub fn ident(&self) -> Ipv4Addr {
        self.ident
    }

    pub fn into_split(self) -> (Sender, Receiver) {
        let (rx, tx) = self.channel.into_split();
        (Sender::new(tx, self.ident), rx)
    }

    pub fn unlink(prefix: impl AsRef<str>, ident: Ipv4Addr) -> Result<()> {
        let name = Self::get_channel_name(prefix, ident);
        info!("unlink queue: {}", name);
        Channel::unlink(&name).context("Failed to unlink completion queue")
    }

    fn get_channel_name(prefix: impl AsRef<str>, ident: Ipv4Addr) -> String {
        format!("{}_{}", prefix.as_ref(), ident.to_string())
    }
}

#[derive(Clone)]
pub struct Sender {
    tx: Arc<Mutex<OwnedWriteHalf>>,
    ident: Ipv4Addr,
}

impl Sender {
    fn new(tx: OwnedWriteHalf, ident: Ipv4Addr) -> Self {
        Self {
            tx: Arc::new(Mutex::new(tx)),
            ident,
        }
    }

    pub async fn ident(&self) -> u32 {
        self.ident.to_bits()
    }

    pub async fn lock(&self) -> kaze_core::ShutdownGuard {
        self.tx.lock().await.shutdown_lock()
    }

    pub async fn send_buf(&self, data: impl Buf) -> Result<()> {
        let tx = self.tx.lock().await;
        let mut ctx = tx
            .write_context(data.remaining())
            .context("Failed to create write context")?;
        if ctx.would_block() {
            counter!("kaze_submission_blocking_total").increment(1);
            block_in_place(|| ctx.wait())
                .map_err(|e| {
                    counter!("kaze_submission_blocking_errors_total")
                        .increment(1);
                    e
                })
                .context("kaze blocking wait submission error")?;
        }
        let len = ctx
            .write(data)
            .map_err(|e| {
                counter!("kaze_submission_errors_total").increment(1);
                e
            })
            .context("kaze submission error")?;
        counter!("kaze_submission_packets_total").increment(1);
        counter!("kaze_submission_bytes_total").increment(len as u64);
        Ok(())
    }

    pub fn service(self, pool: BytesPool) -> impl MessageService<()> {
        service_fn(move |item: Message| {
            let self_ref = self.clone();
            let pool_ref = pool.clone();
            async move { self_ref.send_buf(item.packet().as_buf(&pool_ref)).await }
        })
    }

    pub fn layer<S: MessageService<()>>(
        self,
        pool: BytesPool,
    ) -> impl Layer<S, Service: MessageService<()>> {
        let svc = self.service(pool);
        layer_fn(move |inner: S| {
            let svc = svc.clone();
            service_fn(move |item: Message| {
                let svc = svc.clone();
                let inner = inner.clone();
                async move {
                    if item.destination().is_local() {
                        return svc
                            .clone()
                            .ready_call(item)
                            .await
                            .context("send packet");
                    }
                    inner
                        .clone()
                        .ready_call(item)
                        .await
                        .context("failed to forward packet")
                }
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use kaze_protocol::{packet::new_bytes_pool, service::SinkMessage};
    use tower::{Layer, ServiceExt};

    use crate::Options;

    #[test]
    fn test_send() {
        let edge = Options::default().build().unwrap();
        let (tx, _rx) = edge.into_split();
        let pool = new_bytes_pool();
        tx.clone().service(pool.clone()).boxed();

        let sink = SinkMessage::new();
        tx.clone().layer(pool).layer(sink).boxed();
    }
}
