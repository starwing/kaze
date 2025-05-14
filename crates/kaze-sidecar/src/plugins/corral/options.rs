use std::time::Duration;

use documented_toml::DocumentedToml;
use kaze_plugin::{
    clap::Args,
    serde::{Deserialize, Serialize},
    util::parser::{self, DurationString},
    PluginFactory,
};

use super::Corral;

/// corral configurations
#[derive(Args, Serialize, Deserialize, DocumentedToml, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(next_help_heading = "Corral configurations")]
#[group(id = "CorralOptions")]
pub struct Options {
    /// listen address for the mesh endpoint
    #[serde(default = "default_listen")]
    #[arg(short, long, default_value_t = default_listen())]
    #[arg(value_name = "ADDR")]
    pub listen: String,

    /// limit count for connections
    #[arg(long = "corral-limit")]
    #[arg(value_name = "COUNT")]
    pub limit: Option<usize>,

    /// timeout for pending connection
    #[serde(default = "default_pending_timeout")]
    #[arg(long, value_parser = parser::parse_duration, default_value_t = default_pending_timeout())]
    #[arg(value_name = "DURATION")]
    pub pending_timeout: DurationString,

    /// timeout for idle connection
    #[serde(default = "default_idle_timeout")]
    #[arg(long, value_parser = parser::parse_duration, default_value_t = default_idle_timeout())]
    #[arg(value_name = "DURATION")]
    pub idle_timeout: DurationString,
}

impl PluginFactory for Options {
    type Plugin = Corral;

    fn build(&self) -> anyhow::Result<Self::Plugin> {
        Ok(Corral::new(self))
    }
}

fn default_listen() -> String {
    "0.0.0.0:6081".to_string()
}

fn default_pending_timeout() -> DurationString {
    Duration::from_millis(500).into()
}

fn default_idle_timeout() -> DurationString {
    Duration::from_millis(60_000).into()
}
