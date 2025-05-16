use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use documented_toml::DocumentedToml;
use kaze_plugin::{
    Plugin,
    clap::Args,
    serde::{Deserialize, Serialize},
    util::parser::DurationString,
};

use crate::{Resolver, ResolverNoPlugin, cached::Cached, local::Local};

/// local resolver configurations
#[derive(Args, Serialize, Deserialize, DocumentedToml, Clone, Debug)]
#[command(next_help_heading = "Local resolver configurations")]
#[group(id = "LocalOptions")]
pub struct Options {
    /// Size of resolver mask cache
    #[serde(default = "default_local_cache_size")]
    #[arg(long, default_value_t = default_local_cache_size())]
    #[arg(value_name = "BYTES")]
    pub cache_size: usize,

    /// Live time of entries in resolver mask cache
    #[serde(default = "default_local_livetime")]
    #[arg(long, default_value_t = default_local_livetime())]
    #[arg(value_name = "DURATION")]
    pub live_time: DurationString,

    /// local ident mapping
    #[arg(skip)]
    pub nodes: Vec<Node>,
}

impl Options {
    pub async fn build(&self) -> impl Resolver + Plugin + Clone {
        let resolver =
            Cached::new(Local::new(), self.cache_size, self.live_time);
        for node in &self.nodes {
            resolver.add_node(node.ident.to_bits(), node.addr).await;
        }
        ResolverNoPlugin::new(Arc::new(resolver))
    }
}

/// local ident -> node address mapping
#[derive(Serialize, Deserialize, DocumentedToml, Clone, Debug)]
pub struct Node {
    pub ident: Ipv4Addr,
    pub addr: SocketAddr,
}

fn default_local_cache_size() -> usize {
    114514
}

fn default_local_livetime() -> DurationString {
    Duration::from_secs(1).into()
}
