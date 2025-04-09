use std::net::Ipv4Addr;

use anyhow::Result;
use clap::Args;
use clap_merge::ClapMerge;
use serde::{Deserialize, Serialize};

use crate::Edge;

/// Host bridge configurations
#[derive(ClapMerge, Args, Serialize, Deserialize, Clone, Debug)]
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

pub fn default_bufsize() -> usize {
    println!("default sq_bufsize: {}", page_size::get() * 8);
    page_size::get() * 8
}
