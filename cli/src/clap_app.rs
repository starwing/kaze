use std::net::IpAddr;

use clap::{Args, Parser};
use serde::Deserialize;

/// test cli config
#[derive(Args, Debug)]
#[command(about, version)]
pub struct Config {
    /// host to connecting
    #[arg(short = 'l', long)]
    pub host: IpAddr,

    /// port to connecting
    #[arg(short, long)]
    pub port: Option<u16>,

    /// my config
    #[command(flatten)]
    pub my_config: Option<MyConfig>,
}

#[derive(Parser, Deserialize, Debug)]
pub struct MyConfig {
    /// my name
    #[arg(short, long)]
    pub name: String,
}
