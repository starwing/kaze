use leaky_bucket::RateLimiter;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use tokio::sync::Mutex;

use crate::config::RateLimitInfo;

pub struct RateLimit {
    total: RateLimiter,
    per_msg: Mutex<HashMap<LimitKey, RateLimiter>>,
}

impl RateLimit {
    pub fn new(conf: &RateLimitInfo) -> Self {
        Self {
            total: RateLimiter::builder()
                .max(conf.max)
                .initial(conf.initial)
                .refill(conf.refill)
                .interval(conf.interval)
                .build(),
            per_msg: Mutex::new(
                conf.per_msg
                    .iter()
                    .map(|info| {
                        (
                            LimitKey(info.ident, info.body_type.clone()),
                            RateLimiter::builder()
                                .max(info.max)
                                .initial(info.initial)
                                .refill(info.refill)
                                .interval(info.interval)
                                .build(),
                        )
                    })
                    .collect(),
            ),
        }
    }

    pub async fn acquire_one(&self, ident: &Ipv4Addr, body_type: &String) {
        self.total.acquire_one().await;
        let ident = Some(*ident);
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

#[derive(Hash, Eq, PartialEq)]
struct LimitKey(Option<Ipv4Addr>, Option<String>);
