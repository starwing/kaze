use leaky_bucket::RateLimiter;
use std::collections::HashMap;
use tokio::sync::Mutex;

use crate::config::RateLimitConfig;

pub struct RateLimit {
    total: Option<RateLimiter>,
    per_msg: Mutex<HashMap<LimitKey, RateLimiter>>,
}

impl RateLimit {
    pub fn new(conf: &RateLimitConfig) -> Self {
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

#[derive(Hash, Eq, PartialEq)]
struct LimitKey(Option<u32>, Option<String>);
