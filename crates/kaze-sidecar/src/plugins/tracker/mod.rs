mod options;

pub use options::Options;

use std::sync::OnceLock;
use std::sync::{atomic::AtomicU32, Arc};
use std::time::Duration;

use anyhow::Result;
use futures::StreamExt;
use metrics::counter;
use papaya::HashMap;
use parking_lot::Mutex;
use thingbuf::mpsc::{Receiver, Sender};
use thingbuf::Recycle;
use tokio::select;
use tokio_util::time::delay_queue::Expired;
use tokio_util::time::{delay_queue::Key, DelayQueue};
use tracing::{error, info};

use kaze_plugin::{
    protocol::{
        message::Message,
        packet::Packet,
        proto::{hdr::RpcType, Hdr, RetCode},
    },
    service::AsyncService,
};
use kaze_plugin::{Context, Plugin};

/// RpcTracker is a service that tracks rpc info.
///
/// It is used to track rpc info and send response when timeout.
#[derive(Clone)]
pub struct RpcTracker {
    inner: Arc<Inner>,
}

struct Inner {
    rpc_map: HashMap<u32, Info>,
    seq: AtomicU32,
    exit_timeout: Duration,
    tx: Sender<Action, ActionRecycler>,
    rx: Mutex<Option<Receiver<Action, ActionRecycler>>>,
    ctx: OnceLock<Context>,
}

#[derive(Debug)]
enum Action {
    None,
    Insert(Hdr),
    Remove(u32),
    Expired(Expired<u32>),
}

struct ActionRecycler;

impl Recycle<Action> for ActionRecycler {
    fn new_element(&self) -> Action {
        Action::None
    }
    fn recycle(&self, value: &mut Action) {
        *value = Action::None;
    }
}

impl Plugin for RpcTracker {
    fn context_storage(&self) -> Option<&OnceLock<Context>> {
        Some(&self.inner.ctx)
    }
    fn run(&self) -> Option<kaze_plugin::PluginRunFuture> {
        let rx = self.inner.rx.lock().take().unwrap();
        let tracker = self.clone();
        let ctx = self.context().clone();
        Some(Box::pin(async move {
            tracker.main_loop(rx).await?;
            info!("RpcTracker exiting");
            ctx.trigger_exiting();
            Ok(())
        }))
    }
}

impl RpcTracker {
    pub fn new(opt: &Options) -> Self {
        let (tx, rx) = thingbuf::mpsc::with_recycle(
            opt.tracker_queue_size,
            ActionRecycler,
        );
        let obj = Self {
            inner: Arc::new(Inner {
                ctx: OnceLock::new(),
                rpc_map: HashMap::new(),
                seq: AtomicU32::new(114514),
                exit_timeout: opt.exit_timeout.into(),
                tx,
                rx: Mutex::new(Some(rx)),
            }),
        };
        obj
    }

    async fn main_loop(
        self,
        rx: Receiver<Action, ActionRecycler>,
    ) -> anyhow::Result<()> {
        let mut queue = DelayQueue::new();
        let mut exit_send = false;
        loop {
            let action = if queue.is_empty() {
                select! {
                    insert = rx.recv() => insert,
                    _ = self.context().shutdwon_triggered() => {
                        if exit_send {
                            break;
                        }
                        // shutdown triggered, wait and break the loop
                        self.send_exit().await?;
                        exit_send = true;
                        continue;
                    }
                }
            } else {
                select! {
                    insert = rx.recv() => insert,
                    expired = queue.next() =>
                        expired.map(|e| Action::Expired(e)),
                }
            };
            match action {
                None => break,
                Some(Action::None) => (),
                Some(Action::Insert(hdr)) => {
                    let timeout = Duration::from_millis(hdr.timeout as u64);
                    let Some(seq) = hdr.seq() else {
                        continue;
                    };
                    let key = queue.insert(seq, timeout);
                    self.inner.rpc_map.pin().insert(seq, Info { hdr, key });
                }
                Some(Action::Remove(seq)) => {
                    if let Some(rpc_info) =
                        self.inner.rpc_map.pin().remove(&seq)
                    {
                        queue.try_remove(&rpc_info.key);
                    }
                }
                Some(Action::Expired(key)) => {
                    self.handle_expired(self.context(), key.get_ref()).await
                }
            }
        }
        Ok(())
    }

    async fn handle_expired(&self, ctx: &Context, key: &u32) {
        let msg = {
            let rpc_map = self.inner.rpc_map.pin();
            let Some(rpc_info) = rpc_map.get(key) else {
                return;
            };
            Packet::from_retcode(rpc_info.hdr.clone(), RetCode::RetTimeout)
        };
        if let Err(e) = ctx.send_local(msg).await {
            counter!("kaze_send_timeout_errors_total").increment(1);
            error!(error = %e, "Error sending timeout response");
        }
    }

    async fn send_exit(&self) -> anyhow::Result<()> {
        self.context()
            .send_local(Packet::from_hdr(Hdr {
                body_type: "exit".to_string(),
                rpc_type: Some(RpcType::Req(0)),
                timeout: self.inner.exit_timeout.as_millis() as u32,
                ..Hdr::default()
            }))
            .await?;
        Ok(())
    }
}

impl AsyncService<Message> for RpcTracker {
    type Response = Option<Message>;
    type Error = anyhow::Error;

    async fn serve(&self, msg: Message) -> anyhow::Result<Self::Response> {
        let msg = self.record(msg).await?;
        Ok(Some(msg))
    }
}

impl RpcTracker {
    async fn record(&self, mut req: Message) -> Result<Message> {
        // 1. return req as res if no timeout
        let timeout = req.packet().hdr().timeout;
        if timeout == 0 {
            return Ok(req);
        }
        // 2. return req as res if no rpc_type
        let Some(rpc_type) = req.packet().hdr().rpc_type.clone() else {
            return Ok(req);
        };
        // 3. record rpc info
        let r = match rpc_type {
            RpcType::Req(_) => {
                self.assign_seq(req.packet_mut().hdr_mut());
                self.inner
                    .tx
                    .send(Action::Insert(req.packet().hdr().clone()))
                    .await
            }
            RpcType::Rsp(seq) => self.inner.tx.send(Action::Remove(seq)).await,
        };
        if let Err(e) = r {
            error!(error = %e, "Failed to record rpc info");
        }
        Ok(req)
    }

    fn assign_seq(&self, hdr: &mut Hdr) {
        match &hdr.rpc_type {
            Some(RpcType::Req(seq)) => {
                let mut seq = *seq;
                if seq == 0 {
                    seq = self
                        .inner
                        .seq
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                hdr.rpc_type = Some(RpcType::Req(seq));
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
struct Info {
    hdr: Hdr,
    key: Key,
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::Notify;
    use tower::util::BoxCloneSyncService;

    use kaze_plugin::{
        config_map::ConfigMap,
        protocol::{
            message::{Destination, PacketWithAddr, Source},
            proto::RetCode,
        },
        service::{async_service_fn, ServiceExt},
    };

    #[tokio::test]
    async fn test_rpc_tracker() {
        let notify = Arc::new(Notify::new());
        let ntf_clone = notify.clone();
        let sink = async_service_fn(move |m: PacketWithAddr| {
            let ntf_clone = ntf_clone.clone();
            async move {
                let (packet, _) = m;
                println!("message: {:?}", packet);
                assert_eq!(packet.hdr().ret_code, RetCode::RetTimeout as u32);
                ntf_clone.notify_one();
                Ok::<_, anyhow::Error>(())
            }
        })
        .into_tower();
        let sink = BoxCloneSyncService::new(sink);
        let tracker = RpcTracker::new(&Options {
            tracker_queue_size: 10,
            exit_timeout: "5s".parse().unwrap(),
        });
        let ctx = Context::builder()
            .register(tracker.clone())
            .build(ConfigMap::mock());
        ctx.sink().set(sink);
        let msg = Message::new_with_destination(
            Packet::from_hdr(Hdr {
                body_type: "test".into(),
                rpc_type: Some(RpcType::Req(0)),
                timeout: 1,
                ..Default::default()
            }),
            Source::from_local(),
            Destination::to_local(),
        );
        let tracker_clone = tracker.clone();
        tokio::spawn(async move {
            let _ = tracker_clone.run().unwrap().await;
        });
        let res = tracker.serve(msg).await;
        assert!(res.is_ok());
        notify.notified().await;
    }
}
