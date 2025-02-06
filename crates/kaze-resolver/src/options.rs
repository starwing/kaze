use std::{
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};

use kaze_plugin::{
    clap::Args,
    clap_merge::ClapMerge,
    serde::{Deserialize, Serialize},
    util::duration::{parse_duration, DurationString},
};

use crate::{cached::Cached, local::Local, Resolver};

#[derive(ClapMerge, Args, Serialize, Deserialize, Clone, Debug)]
#[command(next_help_heading = "Local resolver configurations")]
pub struct Options {
    /// Size of resolver mask cache
    #[serde(default = "default_local_cache_size")]
    #[arg(long, default_value_t = default_local_cache_size())]
    #[arg(value_name = "BYTES")]
    pub cache_size: usize,

    /// Live time of entries in resolver mask cache
    #[serde(default = "default_local_livetime")]
    #[arg(long, value_parser = parse_duration, default_value_t = default_local_livetime())]
    #[arg(value_name = "DURATION")]
    pub live_time: DurationString,

    /// local ident mapping
    #[arg(skip)]
    pub nodes: Vec<Node>,
}

impl Options {
    pub async fn build(self) -> impl Resolver {
        let resolver =
            Cached::new(Local::new(), self.cache_size, self.live_time);
        for node in self.nodes {
            resolver.add_node(node.ident.to_bits(), node.addr).await;
        }
        resolver
    }
}

/// local ident -> node address mapping
#[derive(Serialize, Deserialize, Clone, Debug)]
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
