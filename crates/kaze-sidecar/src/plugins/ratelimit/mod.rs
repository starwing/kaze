mod options;

use leaky_bucket::RateLimiter;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use tower::service_fn;

use kaze_plugin::protocol::{message::Message, service::MessageService};

pub use options::Options;

pub struct RateLimit {
    total: Option<RateLimiter>,
    per_msg: Mutex<HashMap<LimitKey, RateLimiter>>,
}

impl RateLimit {
    fn new(conf: &Options) -> Self {
        Self {
            total: conf.max.map(|max| {
                let initial =
                    if conf.initial == 0 { max } else { conf.initial };
                let refill = if conf.refill == 0 { max } else { conf.refill };
                RateLimiter::builder()
                    .max(max)
                    .initial(initial)
                    .refill(refill)
                    .interval(conf.interval.into())
                    .build()
            }),
            per_msg: Mutex::new(
                conf.per_msg
                    .iter()
                    .map(|info| {
                        (
                            LimitKey(
                                info.ident.map(|ident| ident.to_bits()),
                                info.body_type.clone(),
                            ),
                            RateLimiter::builder()
                                .max(info.max)
                                .initial(info.initial)
                                .refill(info.refill)
                                .interval(info.interval.into())
                                .build(),
                        )
                    })
                    .collect(),
            ),
        }
    }

    pub async fn acquire_one(&self, ident: u32, body_type: &String) {
        if let Some(limiter) = &self.total {
            limiter.acquire_one().await;
        }
        let ident = Some(ident);
        let body_type = Some(body_type.clone());
        let key1 = LimitKey(ident, None);
        if let Some(limiter) = self.per_msg.lock().await.get(&key1) {
            limiter.acquire_one().await;
        }
        let key2 = LimitKey(None, body_type);
        if let Some(limiter) = self.per_msg.lock().await.get(&key2) {
            limiter.acquire_one().await;
        }
        let key3 = LimitKey(key1.0, key2.1);
        if let Some(limiter) = self.per_msg.lock().await.get(&key3) {
            limiter.acquire_one().await;
        }
    }
}

impl RateLimit {
    pub fn service(self: Arc<Self>) -> impl MessageService<Message> {
        service_fn(move |req: Message| self.clone().handle_request(req))
    }

    async fn handle_request(
        self: Arc<Self>,
        req: Message,
    ) -> Result<Message, anyhow::Error> {
        if !req.destination().is_local() {
            return Ok(req);
        }
        let ident = req.source().ident();
        let body_type = &req.packet().hdr().body_type;
        self.acquire_one(ident, body_type).await;
        Ok(req)
    }
}

#[derive(Hash, Eq, PartialEq)]
struct LimitKey(Option<u32>, Option<String>);

#[cfg(test)]
mod tests {
    use tower::ServiceExt as _;

    use super::*;

    #[test]
    fn test_send() {
        let rl = Options::default().build();
        rl.service().boxed();
    }
}
