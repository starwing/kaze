use std::{net::Ipv4Addr, str::FromStr, sync::Arc};

use kaze_plugin::{
    clap::Args,
    serde::{Deserialize, Serialize},
    util::parser::DurationString,
};

use super::RateLimit;

/// rate limit configurations
#[derive(Args, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
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
    #[arg(default_value_t = default_interval())]
    #[arg(value_name = "DURATION")]
    pub interval: DurationString,

    #[arg(skip)]
    pub per_msg: Vec<PerMsgLimitInfo>,
}

impl Options {
    /// build a RateLimit instance
    pub fn build(&self) -> Arc<RateLimit> {
        Arc::new(RateLimit::new(self))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
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
