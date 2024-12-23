use std::net::IpAddr;

use clap::Parser;

#[derive(Parser)]
#[command(about)]
pub struct Args {
    /// host to connecting
    #[arg(short, long)]
    pub host: IpAddr,

    /// port to connecting
    #[arg(short, long)]
    pub port: u16,
}
