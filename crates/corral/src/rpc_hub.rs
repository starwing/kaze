use std::{collections::HashMap, time::Duration};

use tokio::sync::Mutex;
use tokio::sync::MutexGuard;
use tokio_stream::StreamExt;
use tokio_util::time::{delay_queue::Key, DelayQueue};

use kaze_protocol::Hdr;

/// record the rpc info, handling response, timeout, etc.
///
/// there are four directions of packets:
///
/// - host req -> node
/// - node rsp -> host
/// - node req -> host
/// - host rsp -> node
///
/// for response, RpcHub will find the request sequence and remove it from
/// RpcHub, and cancel the timeout.
///
/// for request, RpcHub will record the request sequence and the timeout, and
/// when the request timeout, it will be removed from RpcHub, and send a timeout
/// response to target.
pub struct RpcHub {
    ident: u32,
    rpc_map: Mutex<HashMap<u32, RpcInfo>>,
    queue: Mutex<DelayQueue<u32>>,
}

#[derive(Clone)]
pub struct RpcInfo {
    pub hdr: Hdr,
    pub direction: Direction,
    delay_key: Key,
}

#[derive(Copy, Clone)]
pub enum Direction {
    ToHost,
    ToNode,
}

impl RpcHub {
    pub fn new(ident: u32) -> Self {
        Self {
            ident,
            rpc_map: Mutex::new(HashMap::new()),
            queue: Mutex::new(DelayQueue::new()),
        }
    }

    pub fn local_ident(&self) -> u32 {
        self.ident
    }

    async fn lock(
        &self,
    ) -> (
        MutexGuard<'_, HashMap<u32, RpcInfo>>,
        MutexGuard<'_, DelayQueue<u32>>,
    ) {
        (self.rpc_map.lock().await, self.queue.lock().await)
    }

    pub async fn poll(&self) -> Option<RpcInfo> {
        let (rpc_map, mut queue) = self.lock().await;
        let seq = queue.next().await?;
        rpc_map.get(seq.get_ref()).cloned()
    }

    pub async fn record(&self, hdr: Hdr, direction: Direction) {
        if hdr.is_rsp() {
            return self.handle_rsp(hdr).await;
        }

        self.handle_req(hdr, direction).await;
    }

    async fn handle_req(&self, hdr: Hdr, direction: Direction) {
        let (mut rpc_map, mut queue) = self.lock().await;
        let Some(seq) = hdr.seq() else { return };
        let timeout = if hdr.timeout > 0 {
            Duration::from_millis(hdr.timeout as u64)
        } else {
            return;
        };

        let delay_key = queue.insert(seq, timeout);
        let info = RpcInfo {
            hdr,
            direction,
            delay_key,
        };
        rpc_map.insert(seq, info);
    }

    async fn handle_rsp(&self, hdr: Hdr) {
        let (mut rpc_map, mut queue) = self.lock().await;
        let Some(seq) = hdr.seq() else { return };
        if let Some(info) = rpc_map.remove(&seq) {
            queue.remove(&info.delay_key);
        }
    }
}
