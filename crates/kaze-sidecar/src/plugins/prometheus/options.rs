use std::net::SocketAddr;

use documented_toml::DocumentedToml;
use kaze_plugin::{
    clap::Args,
    serde::{Deserialize, Serialize},
    util::parser::DurationString,
    PluginFactory,
};

use super::PrometheusService;

/// prometheus push gateway configuration
#[derive(Args, Serialize, Deserialize, DocumentedToml, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[group(id = "PrometheusOptions")]
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

impl PluginFactory for Options {
    type Plugin = PrometheusService;

    fn build(&self) -> anyhow::Result<Self::Plugin> {
        Ok(PrometheusService)
    }
}

fn default_metrics_listening() -> SocketAddr {
    "127.0.0.1:9090".parse().unwrap()
}
