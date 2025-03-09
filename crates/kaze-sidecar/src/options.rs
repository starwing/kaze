use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::LazyLock,
};

use anyhow::{Context, Result, bail};
use clap::{Args, CommandFactory, FromArgMatches, Parser, crate_version};
use tracing::info;

use kaze_plugin::clap_merge::ClapMerge;
use kaze_plugin::serde::{Deserialize, Serialize};
use kaze_plugin::util::DurationString;

use crate::plugins::{corral, log, ratelimit};

pub fn parse_args() -> Result<Options> {
    let args = Options::command().get_matches();

    let args = match args.get_one::<PathBuf>("config").filter(|p| p.exists()) {
        Some(cfg_path) => {
            info!("use config file {}", cfg_path.display());
            let mut config: Options = toml::from_str(
                &std::fs::read_to_string(&cfg_path)
                    .context("Failed to read config file")?,
            )
            .context("Failed to parse config file")?;
            config.merge(&args);
            config
        }
        _ => Options::from_arg_matches(&args)
            .context("Failed to parse config")?,
    };

    validate_args(&args)?;
    Ok(args)
}

fn validate_args(args: &Options) -> Result<()> {
    if args.edge.ident == Ipv4Addr::UNSPECIFIED {
        bail!("ident must be specified");
    }
    Ok(())
}

static VERSION: LazyLock<String> = LazyLock::new(|| {
    let git_version = bugreport::git_version!(fallback = "");

    if git_version.is_empty() {
        crate_version!().to_string()
    } else {
        format!("{} ({})", crate_version!(), git_version)
    }
});

/// the kaze sidecar for host
#[derive(ClapMerge, Parser, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(version = VERSION.as_str(), about)]
pub struct Options {
    /// Name of config file (default: sidecar.toml)
    #[arg(short, long, default_value = "sidecar.toml")]
    #[arg(value_name = "PATH")]
    #[serde(skip)]
    pub config: PathBuf,

    /// host command line to run after sidecar started
    #[arg(trailing_var_arg = true)]
    #[serde(skip)]
    pub host_cmd: Vec<String>,

    /// listen address for the mesh endpoint
    #[serde(default = "default_listen")]
    #[arg(short, long, default_value_t = default_listen())]
    #[arg(value_name = "ADDR")]
    pub listen: String,

    /// Count of worker threads (0 means autodetect)
    #[arg(short = 'j', long)]
    #[arg(value_name = "N")]
    pub threads: Option<usize>,

    /// log file path
    #[command(flatten)]
    pub log: Option<log::Options>,

    /// Name of the shared memory object
    #[command(flatten)]
    pub edge: kaze_edge::Options,

    /// corral config
    #[command(flatten)]
    pub corral: corral::Options,

    /// rate limit for incomming packets
    #[command(flatten)]
    pub rate_limit: Option<ratelimit::Options>,

    /// resolver config
    #[command(flatten)]
    pub local: kaze_resolver::LocalOptions,

    /// location of consul server
    #[command(flatten)]
    pub consul: Option<ConsulConfig>,

    /// prometheus push gateway
    #[command(flatten)]
    pub prometheus: Option<PromethusConfig>,
}

fn default_listen() -> String {
    "0.0.0.0:6081".to_string()
}

#[derive(ClapMerge, Args, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(next_help_heading = "Consul resolver configurations")]
pub struct ConsulConfig {
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

/// prometheus push gateway configuration
#[derive(ClapMerge, Args, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(next_help_heading = "Prometheus metrics configurations")]
pub struct PromethusConfig {
    /// prometheus metrics endpoint
    #[arg(
        id = "metrics",
        long = "metrics",
        default_missing_value = &default_metrics_listening().to_string()
    )]
    #[arg(value_name = "ADDR")]
    pub listen: Option<SocketAddr>,

    /// prometheus push endpoint
    #[arg(long = "metrics-push-endpoint")]
    #[arg(value_name = "ADDR")]
    pub endpoint: Option<String>,

    /// prometheus push interval
    #[arg(long = "metrics-push-interval")]
    #[arg(value_name = "DURATION")]
    pub interval: Option<DurationString>,

    /// prometheus push username
    #[arg(long = "metrics-push-username")]
    pub username: Option<String>,

    /// prometheus push password
    #[arg(long = "metrics-push-password")]
    pub password: Option<String>,
}

fn default_metrics_listening() -> SocketAddr {
    "127.0.0.1:9090".parse().unwrap()
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Options::command().debug_assert();
}
