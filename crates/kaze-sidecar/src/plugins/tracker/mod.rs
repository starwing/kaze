use std::sync::{atomic::AtomicU32, Arc};
use std::time::Duration;

use anyhow::Result;
use futures::StreamExt;
use kaze_plugin::service::OwnedAsyncService;
use kaze_plugin::{PipelineCell, PipelineRequired};
use metrics::counter;
use papaya::HashMap;
use thingbuf::mpsc::{Receiver, Sender};
use thingbuf::Recycle;
use tokio::{select, sync::Notify};
use tokio_util::time::delay_queue::Expired;
use tokio_util::time::{delay_queue::Key, DelayQueue};
use tracing::error;

use kaze_plugin::{
    local_node,
    protocol::{
        message::Message,
        packet::Packet,
        proto::{
            hdr::{self, RouteType},
            Hdr, RetCode,
        },
    },
    util::tower_ext::ServiceExt,
};

/// RpcTracker is a service that tracks rpc info.
///
/// It is used to track rpc info and send response when timeout.
pub struct RpcTracker {
    rpc_map: HashMap<u32, Info>,
    seq: AtomicU32,
    tx: Sender<Action, ActionRecycler>,
    sink: PipelineCell,
}

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

impl PipelineRequired for RpcTracker {
    fn sink(&self) -> &PipelineCell {
        &self.sink
    }
}

impl RpcTracker {
    pub fn new(capacity: usize, notify: Notify) -> Arc<Self> {
        let (tx, rx) = thingbuf::mpsc::with_recycle(capacity, ActionRecycler);
        let obj = Arc::new(Self {
            rpc_map: HashMap::new(),
            seq: AtomicU32::new(114514),
            tx,
            sink: PipelineCell::new(),
        });
        tokio::spawn(obj.clone().run(rx, notify));
        obj
    }

    async fn run(
        self: Arc<Self>,
        rx: Receiver<Action, ActionRecycler>,
        notify: Notify,
    ) {
        let mut queue = DelayQueue::new();
        loop {
            let action = select! {
                insert = rx.recv() => insert,
                expired = queue.next() => expired.map(|e| Action::Expired(e)),
                _ = notify.notified() => break,
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
                    self.rpc_map.pin().insert(seq, Info { hdr, key });
                }
                Some(Action::Remove(seq)) => {
                    if let Some(rpc_info) = self.rpc_map.pin().remove(&seq) {
                        queue.remove(&rpc_info.key);
                    }
                }
                Some(Action::Expired(key)) => {
                    self.handle_expired(self.sink.clone(), key.get_ref()).await
                }
            }
        }
    }

    async fn handle_expired(&self, mut sink: PipelineCell, key: &u32) {
        let msg = {
            let rpc_map = self.rpc_map.pin();
            let Some(rpc_info) = rpc_map.get(key) else {
                return;
            };
            let mut hdr = rpc_info.hdr.clone();
            let local_ident = local_node().ident;
            hdr.src_ident = local_ident;
            hdr.route_type = Some(RouteType::DstIdent(local_ident));
            Packet::from_retcode(hdr, RetCode::RetTimeout)
        };
        if let Err(e) = sink.ready_call((msg, None)).await {
            counter!("kaze_send_timeout_errors_total").increment(1);
            error!(error = %e, "Error sending timeout response");
        }
    }
}

impl OwnedAsyncService<Message> for RpcTracker {
    type Response = Option<Message>;
    type Error = anyhow::Error;

    async fn serve(
        self: Arc<Self>,
        msg: Message,
    ) -> anyhow::Result<Self::Response> {
        let res = self.record(msg).await;
        if let Err(e) = res {
            error!(error = %e, "Failed to record rpc info");
        }
        Ok(None)
    }
}

impl RpcTracker {
    async fn record(self: Arc<Self>, mut req: Message) -> Result<Message> {
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
            hdr::RpcType::Req(_) => {
                self.assign_seq(req.packet_mut().hdr_mut());
                self.tx
                    .send(Action::Insert(req.packet().hdr().clone()))
                    .await
            }
            hdr::RpcType::Rsp(seq) => self.tx.send(Action::Remove(seq)).await,
        };
        if let Err(e) = r {
            error!(error = %e, "Failed to record rpc info");
        }
        Ok(req)
    }

    fn assign_seq(&self, hdr: &mut Hdr) {
        match &hdr.rpc_type {
            Some(hdr::RpcType::Req(seq)) => {
                let mut seq = *seq;
                if seq == 0 {
                    seq = self
                        .seq
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                hdr.rpc_type = Some(hdr::RpcType::Req(seq));
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

    use tower::util::BoxCloneSyncService;

    use hdr::RpcType;
    use kaze_plugin::{
        protocol::message::{Destination, PacketWithAddr, Source},
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
        let tracker = RpcTracker::new(10, Notify::new());
        tracker.sink().set(sink);
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
        let res = tracker.record(msg).await;
        assert!(res.is_ok());
        notify.notified().await;
    }
}
