use kaze_plugin::clap::Args;
use kaze_plugin::clap_merge::ClapMerge;
use kaze_plugin::serde::{Deserialize, Serialize};

#[derive(ClapMerge, Args, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(next_help_heading = "Consul resolver configurations")]
pub struct Options {
    #[serde(default = "default_consul_addr")]
    #[arg(long = "consol", required = false, default_missing_value = default_consul_addr())]
    pub addr: String,

    /// consul token
    #[arg(long = "consul-token")]
    pub consul_token: Option<String>,
}

fn default_consul_addr() -> String {
    "127.0.0.1:8500".to_string()
}
