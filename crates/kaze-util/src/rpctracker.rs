use std::error::Error as StdError;
use std::sync::{atomic::AtomicU32, Arc};
use std::time::Duration;

use anyhow::Result;
use futures::StreamExt;
use metrics::counter;
use papaya::HashMap;
use thingbuf::mpsc::{Receiver, Sender};
use thingbuf::Recycle;
use tokio::{
    select,
    sync::{Mutex, Notify},
};
use tokio_util::time::delay_queue::Expired;
use tokio_util::time::{delay_queue::Key, DelayQueue};
use tower::{
    layer::{layer_fn, util::Stack},
    service_fn, Layer, Service,
};
use tracing::error;

use kaze_protocol::{
    message::{Destination, Message, Source},
    packet::Packet,
    proto::{hdr, Hdr, RetCode},
    sink::Sink,
};

/// RpcTracker is a service that tracks rpc info.
///
/// It is used to track rpc info and send response when timeout.
pub struct RpcTracker<S> {
    rpc_map: HashMap<u32, Info>,
    seq: AtomicU32,
    tx: Sender<Action, ActionRecycler>,
    sink: Mutex<S>,
}

enum Action {
    None,
    Insert(Hdr, Destination),
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

impl<S> RpcTracker<S>
where
    S: Sink<Message> + Unpin + Send + Sync + 'static,
    <S as Sink<Message>>::Future: Sync + Send + 'static,
    <S as Sink<Message>>::Error: AsRef<dyn StdError> + Sync + Send + 'static,
{
    pub fn new(capacity: usize, sink: S, notify: Notify) -> Arc<Self> {
        let (tx, rx) = thingbuf::mpsc::with_recycle(capacity, ActionRecycler);
        let obj = Arc::new(Self {
            rpc_map: HashMap::new(),
            seq: AtomicU32::new(114514),
            tx,
            sink: Mutex::new(sink),
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
                Some(Action::Insert(hdr, to)) => {
                    let timeout = Duration::from_millis(hdr.timeout as u64);
                    let Some(seq) = hdr.seq() else {
                        continue;
                    };
                    let key = queue.insert(seq, timeout);
                    self.rpc_map.pin().insert(seq, Info { hdr, key, to });
                }
                Some(Action::Remove(seq)) => {
                    if let Some(rpc_info) = self.rpc_map.pin().remove(&seq) {
                        queue.remove(&rpc_info.key);
                    }
                }
                Some(Action::Expired(key)) => {
                    self.handle_expired(key.get_ref()).await
                }
            }
        }
    }

    async fn handle_expired(&self, key: &u32) {
        let msg = {
            let rpc_map = self.rpc_map.pin();
            let Some(rpc_info) = rpc_map.get(key) else {
                return;
            };
            Message::new_with_destination(
                Packet::from_retcode(
                    rpc_info.hdr.clone(),
                    RetCode::RetTimeout,
                ),
                Source::from_local(),
                rpc_info.to.clone(),
            )
        };
        if let Err(e) = self.sink.lock().await.send(msg).await {
            counter!("kaze_send_timeout_errors_total").increment(1);
            error!(error = %e.as_ref(), "Error sending timeout response");
        }
    }
}

impl<S> RpcTracker<S> {
    pub fn service<'a>(
        self: Arc<Self>,
    ) -> impl Service<Message, Response = Message, Error = anyhow::Error> {
        service_fn(move |req: Message| self.clone().record(req))
    }

    pub fn layer(self: Arc<Self>) -> impl Layer<S> {
        layer_fn(move |inner: S| {
            let svc = self.clone().service();
            Stack::new(svc, inner)
        })
    }

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
                    .send(Action::Insert(
                        req.packet().hdr().clone(),
                        req.destination().clone(),
                    ))
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
    to: Destination,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hdr::RpcType;
    use kaze_protocol::sink::sink_fn;

    #[tokio::test]
    async fn test_rpc_tracker() {
        let notify = Arc::new(Notify::new());
        let ntf_clone = notify.clone();
        let sink = sink_fn(move |m: Message| {
            let ntf_clone = ntf_clone.clone();
            async move {
                println!("message: {:?}", m);
                assert_eq!(
                    m.packet().hdr().ret_code,
                    RetCode::RetTimeout as u32
                );
                ntf_clone.notify_one();
                Ok::<_, anyhow::Error>(())
            }
        });
        let tracker = RpcTracker::new(10, sink, Notify::new());
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
