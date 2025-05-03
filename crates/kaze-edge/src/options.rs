use std::net::Ipv4Addr;

use anyhow::Result;
use kaze_plugin::clap::Args;
use kaze_plugin::serde::{Deserialize, Serialize};
use kaze_plugin::{ClapDefault, clap};

use crate::Edge;

// Host bridge configurations
#[derive(Args, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(next_help_heading = "Host bridge configurations")]
#[group(id = "EdgeOptions")]
pub struct Options {
    /// Name of the shared memory object
    #[serde(default = "default_name")]
    #[arg(short, long, default_value_t = default_name())]
    pub name: String,

    /// Identifier for the shared memory object
    #[arg(short, long, default_value_t = Ipv4Addr::UNSPECIFIED)]
    pub ident: Ipv4Addr,

    /// Size of the buffer in bytes for shared memory
    #[serde(default = "default_bufsize")]
    #[arg(long, default_value_t = default_bufsize())]
    #[arg(value_name = "BYTES")]
    pub bufsize: usize,

    /// Unlink shared memory object if it exists
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    #[arg(default_value_t = default_unlink())]
    #[serde(skip)]
    pub unlink: bool,
}

impl Options {
    /// create a new options
    pub fn new() -> Self {
        Self::default()
    }

    /// set name
    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    /// set ident
    pub fn with_ident(mut self, ident: Ipv4Addr) -> Self {
        self.ident = ident;
        self
    }

    /// set bufsize
    pub fn with_bufsize(mut self, bufsize: usize) -> Self {
        self.bufsize = bufsize;
        self
    }

    /// set unlink
    pub fn with_unlink(mut self, unlink: bool) -> Self {
        self.unlink = unlink;
        self
    }

    /// build
    pub fn build(self) -> Result<Edge> {
        Edge::new(self.name, self.ident, self.bufsize, self.unlink)
    }
}

fn default_name() -> String {
    "kaze".to_string()
}

fn default_unlink() -> bool {
    true
}

pub fn default_bufsize() -> usize {
    page_size::get() * 8
}
