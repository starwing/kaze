mod options;

use leaky_bucket::RateLimiter;
use std::sync::Arc;
use tower::service_fn;

use kaze_plugin::{
    protocol::{message::Message, service::MessageService},
    util::DurationString,
};

pub use options::Options;

pub struct RateLimit {
    total: Option<RateLimiter>,
    per_msg: papaya::HashMap<LimitKey, RateLimiter>,
}

impl RateLimit {
    fn new(conf: &Options) -> Self {
        Self {
            total: conf.max.map(|max| {
                let initial =
                    if conf.initial == 0 { max } else { conf.initial };
                let refill = if conf.refill == 0 { max } else { conf.refill };
                Self::new_limiter(max, initial, refill, conf.interval)
            }),
            per_msg: conf
                .per_msg
                .iter()
                .map(|info| {
                    (
                        LimitKey(
                            info.ident.map(|ident| ident.to_bits()),
                            info.body_type.clone(),
                        ),
                        Self::new_limiter(
                            info.max,
                            info.initial,
                            info.refill,
                            info.interval,
                        ),
                    )
                })
                .collect(),
        }
    }

    fn new_limiter(
        max: usize,
        initial: usize,
        refill: usize,
        interval: DurationString,
    ) -> RateLimiter {
        RateLimiter::builder()
            .max(max)
            .initial(initial)
            .refill(refill)
            .interval(interval.into())
            .build()
    }

    pub async fn acquire_one(&self, ident: u32, body_type: &String) {
        if let Some(limiter) = &self.total {
            limiter.acquire_one().await;
        }
        let map = self.per_msg.pin_owned();
        if map.len() == 0 {
            return;
        }
        let ident = Some(ident);
        let body_type = Some(body_type.clone());
        let key1 = LimitKey(ident, None);
        if let Some(limiter) = map.get(&key1) {
            limiter.acquire_one().await;
        }
        let key2 = LimitKey(None, body_type);
        if let Some(limiter) = map.get(&key2) {
            limiter.acquire_one().await;
        }
        let key3 = LimitKey(key1.0, key2.1);
        if let Some(limiter) = map.get(&key3) {
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
