use std::time::Duration;

use documented_toml::DocumentedToml;
use kaze_plugin::{
    clap::Args,
    serde::{Deserialize, Serialize},
    util::parser::DurationString,
    PluginFactory,
};

use super::RpcTracker;

/// RPC call tracker configurations
#[derive(Args, Serialize, Deserialize, DocumentedToml, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(next_help_heading = "Tracker configurations")]
#[group(id = "TrackerOptions")]
pub struct Options {
    /// listen address for the mesh endpoint
    #[serde(default = "default_tracker_queue_size")]
    #[arg(short, long, default_value_t = default_tracker_queue_size())]
    #[arg(value_name = "LEN")]
    pub tracker_queue_size: usize,

    /// timeout for the RPC call waiting when gracefully exiting
    #[serde(default = "default_exit_timeout")]
    #[arg(short, long, default_value_t = default_exit_timeout())]
    #[arg(value_name = "TIMEOUT")]
    pub exit_timeout: DurationString,
}

impl PluginFactory for Options {
    type Plugin = RpcTracker;

    fn build(&self) -> anyhow::Result<Self::Plugin> {
        Ok(RpcTracker::new(self))
    }
}

fn default_tracker_queue_size() -> usize {
    1024
}

fn default_exit_timeout() -> DurationString {
    DurationString::new(Duration::from_secs(1))
}
