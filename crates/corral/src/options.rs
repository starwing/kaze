use clap::Args;
use clap_merge::ClapMerge;
use kaze_utils::{parse_duration, DurationString};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// options for corral
#[derive(ClapMerge, Args, Serialize, Deserialize, Clone, Debug)]
#[command(next_help_heading = "Corral configurations")]
#[group(id = "CorralOptions")]
pub struct Options {
    /// timeout for pending connection
    #[serde(default = "default_pending_timeout")]
    #[arg(long, value_parser = parse_duration, default_value_t = default_pending_timeout())]
    #[arg(value_name = "DURATION")]
    pub pending_timeout: DurationString,

    /// timeout for idle connection
    #[serde(default = "default_idle_timeout")]
    #[arg(long, value_parser = parse_duration, default_value_t = default_idle_timeout())]
    #[arg(value_name = "DURATION")]
    pub idle_timeout: DurationString,
}

fn default_pending_timeout() -> DurationString {
    Duration::from_millis(500).into()
}

fn default_idle_timeout() -> DurationString {
    Duration::from_millis(60_000).into()
}
