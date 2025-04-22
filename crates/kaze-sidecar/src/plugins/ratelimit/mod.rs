mod options;

use futures::future::OptionFuture;
use leaky_bucket::RateLimiter;
use std::{sync::Arc, time::Duration};
use tokio::{join, select};
use tracing::error;

use kaze_plugin::{
    protocol::message::Message, service::OwnedAsyncService,
    util::parser::DurationString, ArcPlugin,
};

pub use options::Options;

pub struct RateLimit {
    total: Option<RateLimitInfo>,
    per_msg: papaya::HashMap<LimitKey, RateLimitInfo>,
}

pub struct RateLimitInfo {
    lim: RateLimiter,
    timeout: Duration,
}

impl RateLimitInfo {
    async fn acquire_one(&self) -> bool {
        select! {
            _ = tokio::time::sleep(self.timeout) => {
                false // timeout
            }
            _ = self.lim.acquire_one() => {
                true // acquired
            }
        }
    }
}

impl RateLimit {
    fn new(opt: &Options) -> Self {
        Self {
            total: opt.max.map(|max| {
                let initial = if opt.initial == 0 { max } else { opt.initial };
                let refill = if opt.refill == 0 { max } else { opt.refill };
                Self::new_limiter(
                    max,
                    initial,
                    refill,
                    opt.interval,
                    opt.timeout,
                )
            }),
            per_msg: opt
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
                            info.timeout,
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
        timeout: DurationString,
    ) -> RateLimitInfo {
        RateLimitInfo {
            lim: RateLimiter::builder()
                .max(max)
                .initial(initial)
                .refill(refill)
                .interval(interval.into())
                .build(),
            timeout: timeout.into(),
        }
    }

    pub async fn acquire_one(&self, ident: u32, body_type: &String) -> bool {
        if let Some(info) = &self.total {
            if !info.acquire_one().await {
                return false; // timeout
            }
        }
        let map = self.per_msg.pin_owned();
        if map.len() == 0 {
            return true;
        }
        let ident = Some(ident);
        let body_type = Some(body_type.clone());
        let key1 = LimitKey(ident, None);
        let fut1 = map.get(&key1).map(|limiter| limiter.acquire_one());
        let key2 = LimitKey(None, body_type);
        let fut2 = map.get(&key2).map(|limiter| limiter.acquire_one());
        let key3 = LimitKey(key1.0, key2.1);
        let fut3 = map.get(&key3).map(|limiter| limiter.acquire_one());
        let (r1, r2, r3) = join!(
            OptionFuture::from(fut1),
            OptionFuture::from(fut2),
            OptionFuture::from(fut3)
        );
        r1 != Some(false) && r2 != Some(false) && r3 != Some(false)
    }
}

impl OwnedAsyncService<Message> for RateLimit {
    type Response = Option<Message>;
    type Error = anyhow::Error;

    async fn serve(
        self: Arc<Self>,
        msg: Message,
    ) -> anyhow::Result<Self::Response> {
        if !msg.destination().is_local() {
            return Ok(Some(msg));
        }
        let ident = msg.source().ident();
        let body_type = &msg.packet().hdr().body_type;
        if self.acquire_one(ident, body_type).await {
            Ok(Some(msg))
        } else {
            error!(
                ident = ?ident,
                body_type = ?body_type,
                "Rate limit timeout"
            );
            Ok(None)
        }
    }
}

impl ArcPlugin for RateLimit {
    fn init(self: &Arc<Self>, _ctx: kaze_plugin::Context) {}
    fn context(self: &Arc<Self>) -> &kaze_plugin::Context {
        unimplemented!("RateLimit does not provide context")
    }
}

#[derive(Hash, Eq, PartialEq)]
struct LimitKey(Option<u32>, Option<String>);
#[cfg(test)]
mod tests {
    use tokio::time;
    use super::*;

    #[tokio::test]
    async fn test_rate_limit_info_acquire_one() {
        let info = RateLimitInfo {
            lim: RateLimiter::builder()
                .max(1)
                .initial(1)
                .refill(1)
                .interval(Duration::from_secs(1))
                .build(),
            timeout: Duration::from_secs(5),
        };

        // First acquire should succeed
        assert!(info.acquire_one().await);

        // Second acquire should timeout or succeed after refill
        let now = std::time::Instant::now();
        let result = info.acquire_one().await;
        let elapsed = now.elapsed();

        if result {
            // If succeeded, should have taken close to refill interval
            assert!(elapsed.as_millis() >= 990);
        } else {
            // If failed, timeout should have been triggered
            assert!(elapsed.as_secs() >= 4);
        }
    }

    #[tokio::test]
    async fn test_rate_limit_total_only() {
        let opt = Options {
            max: Some(2),
            initial: 2,
            refill: 1,
            interval: DurationString::from(Duration::from_millis(500)),
            timeout: DurationString::from(Duration::from_millis(100)),
            per_msg: vec![],
        };

        let rate_limit = RateLimit::new(&opt);

        // First two requests should pass
        assert!(rate_limit.acquire_one(1, &"test".to_string()).await);
        assert!(rate_limit.acquire_one(2, &"test".to_string()).await);

        // Third request should fail due to timeout
        assert!(!rate_limit.acquire_one(3, &"test".to_string()).await);

        // After waiting, request should succeed again
        time::sleep(Duration::from_millis(500)).await;
        assert!(rate_limit.acquire_one(4, &"test".to_string()).await);
    }

    #[tokio::test]
    async fn test_rate_limit_per_msg() {
        let opt = Options {
            max: None,
            initial: 0,
            refill: 0,
            interval: DurationString::from(Duration::from_secs(1)),
            timeout: DurationString::from(Duration::from_millis(100)),
            per_msg: vec![
                options::PerMsgLimitInfo {
                    ident: "0.0.0.1".parse().ok(),
                    body_type: None,
                    max: 1,
                    initial: 1,
                    refill: 1,
                    interval: DurationString::from(Duration::from_millis(500)),
                    timeout: DurationString::from(Duration::from_millis(100)),
                },
                options::PerMsgLimitInfo {
                    ident: None,
                    body_type: Some("test_type".to_string()),
                    max: 1,
                    initial: 1,
                    refill: 1,
                    interval: DurationString::from(Duration::from_millis(500)),
                    timeout: DurationString::from(Duration::from_millis(100)),
                },
            ],
        };

        let rate_limit = RateLimit::new(&opt);

        // First request should pass for client 1
        assert!(rate_limit.acquire_one(1, &"test_type".to_string()).await);

        // Second request should fail due to per-client limit
        assert!(!rate_limit.acquire_one(1, &"test_type".to_string()).await);

        // Request from different client but same type should fail due to per-type limit
        assert!(!rate_limit.acquire_one(2, &"test_type".to_string()).await);

        // Request with different type should pass
        assert!(rate_limit.acquire_one(2, &"other_type".to_string()).await);

        // After waiting, requests should succeed again
        time::sleep(Duration::from_millis(500)).await;
        assert!(rate_limit.acquire_one(1, &"test_type".to_string()).await);
    }
}
