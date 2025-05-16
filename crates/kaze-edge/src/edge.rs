use std::{
    borrow::Cow,
    net::Ipv4Addr,
    sync::{Arc, OnceLock},
};

use anyhow::{Context, Result, bail};
use kaze_core::bytes::BufMut;
use kaze_plugin::{
    Plugin, service::AsyncService, util::tower_ext::ServiceExt as _,
};
use kaze_protocol::packet::Packet;
use metrics::counter;
use tokio::{select, sync::Mutex, task::spawn_blocking};
use tracing::{info, trace, warn};

use kaze_core::{Channel, OwnedReadHalf, OwnedWriteHalf};
use kaze_plugin::protocol::{bytes::Buf, message::Message};

pub use kaze_core::Error;
pub use kaze_core::ShutdownGuard;
pub use kaze_core::UnlinkGuard;

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
    #[inline]
    pub fn name(&self) -> Cow<'_, str> {
        self.channel.name()
    }

    /// Get the channel ident.
    #[inline]
    pub fn ident(&self) -> Ipv4Addr {
        self.ident
    }

    pub fn into_split(self) -> (Sender, Receiver) {
        let (rx, tx) = self.channel.into_split();
        (Sender::new(tx, self.ident), Receiver::new(rx))
    }

    pub fn unlink(prefix: impl AsRef<str>, ident: Ipv4Addr) -> Result<()> {
        let name = Self::get_channel_name(prefix, ident);
        info!("unlink queue: {}", name);
        Channel::unlink(&name).context("Failed to unlink completion queue")
    }

    #[inline]
    pub fn unlink_guard(&self) -> kaze_core::UnlinkGuard {
        self.channel.unlink_guard()
    }

    fn get_channel_name(prefix: impl AsRef<str>, ident: Ipv4Addr) -> String {
        format!("{}_{}", prefix.as_ref(), ident.to_string())
    }
}

#[derive(Clone)]
pub struct Receiver {
    ctx: OnceLock<kaze_plugin::Context>,
    rx: OwnedReadHalf,
}

impl Receiver {
    pub fn new(rx: OwnedReadHalf) -> Self {
        Self {
            rx,
            ctx: OnceLock::new(),
        }
    }

    #[inline]
    pub fn shutdown(&self) -> anyhow::Result<()> {
        self.rx.shutdown().map_err(Into::into)
    }

    pub async fn read_packet(&mut self) -> Result<Packet> {
        let mut ctx = self
            .rx
            .read_context()
            .context("Failed to create read context")?;
        if ctx.would_block() {
            counter!("kaze_completion_blocking_total").increment(1);
            // SAFETY: we use it only in spawn_blocking.
            let spawn_ctx = unsafe { ctx.into_static() };
            ctx = spawn_blocking(move || spawn_ctx.wait())
                .await?
                .map_err(|e| {
                    counter!("kaze_completion_blocking_errors_total")
                        .increment(1);
                    e
                })
                .context("kaze blocking wait completion error")?;
        }
        let mut bytes = self.context().pool().pull_owned();
        let buf = ctx.buffer();
        let len = buf.len();
        bytes.rewind();
        bytes.as_inner_mut().clear();
        bytes.as_inner_mut().reserve(len);
        bytes.as_inner_mut().put_slice(buf);
        ctx.commit(len)
            .map_err(|e| {
                counter!("kaze_commpletion_errors_total").increment(1);
                e
            })
            .context("kaze completion error")?;
        counter!("kaze_completion_packets_total").increment(1);
        counter!("kaze_completion_bytes_total").increment(len as u64);
        let packet = Packet::from_host(bytes)
            .context("kaze host packet parse error")?;
        Ok(packet)
    }
}

impl Plugin for Receiver {
    #[inline]
    fn context_storage(&self) -> Option<&OnceLock<kaze_plugin::Context>> {
        Some(&self.ctx)
    }

    fn run(&self) -> Option<kaze_plugin::PluginRunFuture> {
        let mut rx = self.clone();
        let ctx = self.context().clone();
        Some(Box::pin(async move {
            let mut sink = rx.context().sink().clone();
            loop {
                let packet = select! {
                    pkt = rx.read_packet() => pkt.context("Failed to read packet")?,
                    _ = ctx.exiting() => break,
                };
                sink.ready_call((packet, None)).await?;
            }
            info!("Receiver exiting");
            Ok(())
        }))
    }
}

#[derive(Clone)]
pub struct Sender {
    inner: Arc<SenderInner>,
    ident: Ipv4Addr,
}

struct SenderInner {
    ctx: OnceLock<kaze_plugin::Context>,
    tx: Mutex<OwnedWriteHalf>,
}

impl Sender {
    fn new(tx: OwnedWriteHalf, ident: Ipv4Addr) -> Self {
        Self {
            ident,
            inner: Arc::new(SenderInner {
                ctx: OnceLock::new(),
                tx: Mutex::new(tx),
            }),
        }
    }

    #[inline]
    pub async fn ident(&self) -> u32 {
        self.ident.to_bits()
    }

    pub async fn lock(&self) -> kaze_core::ShutdownGuard {
        self.inner.tx.lock().await.shutdown_lock()
    }

    pub async fn send_buf(&self, data: impl Buf) -> Result<()> {
        let tx = self.inner.tx.lock().await;
        let mut ctx = tx
            .write_context(data.remaining())
            .context("Failed to create write context")?;
        if ctx.would_block() {
            counter!("kaze_submission_blocking_total").increment(1);
            // SAFETY: we use it only in spawn_blocking.
            let spawn_ctx = unsafe { ctx.into_static() };
            ctx = spawn_blocking(|| spawn_ctx.wait())
                .await?
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
}

impl AsyncService<Message> for Sender {
    type Response = Option<Message>;
    type Error = anyhow::Error;

    async fn serve(
        &self,
        msg: Message,
    ) -> std::result::Result<Self::Response, Self::Error> {
        if msg.destination().is_local() {
            self.send_buf(
                msg.packet().as_buf(&self.inner.ctx.get().unwrap().pool()),
            )
            .await?;
            trace!(hdr = ?msg.packet().hdr(), "send packet to host");
            return Ok(None);
        }
        Ok(Some(msg))
    }
}

impl Plugin for Sender {
    #[inline]
    fn context_storage(&self) -> Option<&OnceLock<kaze_plugin::Context>> {
        Some(&self.inner.ctx)
    }
}

#[cfg(test)]
mod tests {
    use kaze_plugin::{config_map::ConfigMap, service::AsyncService};
    use kaze_protocol::{
        message::{Destination, Message, Source},
        packet::Packet,
        proto::{Hdr, RetCode},
    };

    use crate::Options;

    #[tokio::test]
    async fn test_send() {
        let edge = Options::new().with_unlink(true).build().unwrap();
        let _guard = edge.unlink_guard();
        let (tx, _rx) = edge.into_split();
        kaze_plugin::Context::builder()
            .register(tx.clone())
            .build(ConfigMap::mock());
        let r = tx
            .serve(Message::new_with_destination(
                Packet::from_retcode(Hdr::default(), RetCode::RetOk),
                Source::Host,
                Destination::Host,
            ))
            .await
            .unwrap();
        assert!(matches!(r, None));

        let r = tx
            .serve(Message::new_with_destination(
                Packet::from_retcode(Hdr::default(), RetCode::RetOk),
                Source::Host,
                Destination::Drop,
            ))
            .await
            .unwrap();
        assert!(matches!(r, Some(_)));
    }
}
