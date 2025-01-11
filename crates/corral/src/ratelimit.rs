use clap::Args;
use clap_merge::ClapMerge;
use kaze_utils::{parse_duration, DurationString};
use leaky_bucket::RateLimiter;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::Ipv4Addr, str::FromStr};
use tokio::sync::Mutex;

#[derive(ClapMerge, Args, Serialize, Deserialize, Clone, Debug)]
#[command(next_help_heading = "Rate limit configurations")]
#[group(id = "RateLimitOptions")]
pub struct Options {
    /// max requests per duration
    #[arg(long = "total-max", value_name = "N")]
    pub max: Option<usize>,

    /// initial requests when initialized
    #[arg(long = "total-initial", value_name = "N", default_value_t = 0)]
    pub initial: usize,

    /// refill requests count per duration
    #[arg(long = "total-refill", value_name = "N", default_value_t = 0)]
    pub refill: usize,

    /// refill interval
    #[serde(default = "default_interval")]
    #[arg(id = "rate_limit_interval", long = "total-interval")]
    #[arg(value_parser = parse_duration, default_value_t = default_interval())]
    #[arg(value_name = "DURATION")]
    pub interval: DurationString,

    #[arg(skip)]
    pub per_msg: Vec<PerMsgLimitInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PerMsgLimitInfo {
    pub ident: Option<Ipv4Addr>,
    pub body_type: Option<String>,

    pub max: usize,
    pub initial: usize,
    pub refill: usize,

    #[serde(default = "default_interval")]
    pub interval: DurationString,
}

fn default_interval() -> DurationString {
    DurationString::from_str("100ms").unwrap()
}

pub struct RateLimit {
    total: Option<RateLimiter>,
    per_msg: Mutex<HashMap<LimitKey, RateLimiter>>,
}

impl RateLimit {
    pub fn new(conf: &Options) -> Self {
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
