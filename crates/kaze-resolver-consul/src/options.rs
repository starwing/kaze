use std::net::SocketAddr;
use std::time::Duration;

use kaze_plugin::PluginFactory;
use kaze_plugin::clap::{self, Args};
use kaze_plugin::documented_toml::{self, DocumentedToml};
use kaze_plugin::serde::{Deserialize, Serialize};
use kaze_plugin::util::parser::DurationString;

use crate::ConsulResolver;

/// consul resolver configurations
#[derive(Args, Serialize, Deserialize, DocumentedToml, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(next_help_heading = "Consul resolver configurations")]
#[group(id = "ConsulOptions")]
pub struct Options {
    /// consul address
    #[arg(long = "consul", required = false)]
    pub addr: Option<String>,

    /// consul token
    #[arg(long = "consul-token")]
    pub consul_token: Option<String>,

    /// service name
    #[arg(long, default_value_t = default_service_name())]
    pub service_name: String,

    /// service listen address
    #[arg(long = "register-addr")]
    pub register_addr: SocketAddr,

    /// keep alive interval
    #[arg(long = "consul-refresh-interval", default_value_t = default_register_interval())]
    pub register_interval: DurationString,
}

impl PluginFactory for Options {
    type Plugin = ConsulResolver;

    fn build(&self) -> anyhow::Result<Self::Plugin> {
        let mut config = rs_consul::Config::from_env();
        if let Some(addr) = &self.addr {
            config.address = addr.clone();
        }
        if let Some(token) = &self.consul_token {
            config.token = Some(token.clone());
        }
        let client = rs_consul::Consul::new(config);
        Ok(ConsulResolver::new(client))
    }
}

fn default_service_name() -> String {
    "kaze-service".to_string()
}

fn default_register_interval() -> DurationString {
    Duration::from_secs(10).into()
}
