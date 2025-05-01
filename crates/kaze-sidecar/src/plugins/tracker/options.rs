use kaze_plugin::{
    clap::Args,
    serde::{Deserialize, Serialize},
    PluginFactory,
};

use super::RpcTracker;

/// corral configurations
#[derive(Args, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(next_help_heading = "Tracker configurations")]
#[group(id = "TrackerOptions")]
pub struct Options {
    /// listen address for the mesh endpoint
    #[serde(default = "default_tracker_queue_size")]
    #[arg(short, long, default_value_t = default_tracker_queue_size())]
    #[arg(value_name = "LEN")]
    pub tracker_queue_size: usize,
}

impl PluginFactory for Options {
    type Plugin = RpcTracker;

    fn build(self) -> anyhow::Result<Self::Plugin> {
        Ok(RpcTracker::new(self.tracker_queue_size))
    }
}

fn default_tracker_queue_size() -> usize {
    1024
}
