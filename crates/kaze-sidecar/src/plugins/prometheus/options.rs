use std::net::SocketAddr;

use kaze_plugin::clap::Args;
use kaze_plugin::serde::{Deserialize, Serialize};
use kaze_plugin::util::DurationString;

/// prometheus push gateway configuration
#[derive(Args, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(next_help_heading = "Prometheus metrics configurations")]
pub struct Options {
    /// prometheus metrics endpoint
    #[arg(
        id = "metrics",
        long = "metrics",
        default_missing_value = &default_metrics_listening().to_string()
    )]
    #[arg(value_name = "ADDR")]
    pub listen: Option<SocketAddr>,

    /// prometheus push endpoint
    #[arg(long = "metrics-push-endpoint")]
    #[arg(value_name = "ADDR")]
    pub endpoint: Option<String>,

    /// prometheus push interval
    #[arg(long = "metrics-push-interval")]
    #[arg(value_name = "DURATION")]
    pub interval: Option<DurationString>,

    /// prometheus push username
    #[arg(long = "metrics-push-username")]
    pub username: Option<String>,

    /// prometheus push password
    #[arg(long = "metrics-push-password")]
    pub password: Option<String>,
}

fn default_metrics_listening() -> SocketAddr {
    "127.0.0.1:9090".parse().unwrap()
}
