use clap::Args;
use clap_merge::ClapMerge;
use kaze_protocol::packet::BytesPool;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use kaze_util::duration::{parse_duration, DurationString};

use super::corral::Corral;

/// options for corral
#[derive(ClapMerge, Args, Serialize, Deserialize, Clone, Debug)]
#[command(next_help_heading = "Corral configurations")]
#[group(id = "CorralOptions")]
pub struct Options {
    /// limit count for connections
    #[arg(long = "corral-limit")]
    #[arg(value_name = "COUNT")]
    pub limit: Option<usize>,

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

impl Options {
    pub fn build<Sink>(self, pool: BytesPool, sink: Sink) -> Corral<Sink> {
        Corral::new(self, pool, sink)
    }
}

fn default_pending_timeout() -> DurationString {
    Duration::from_millis(500).into()
}

fn default_idle_timeout() -> DurationString {
    Duration::from_millis(60_000).into()
}
