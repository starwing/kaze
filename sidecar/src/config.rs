use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
    sync::LazyLock,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use clap::{crate_version, Parser};
use clap_serde_derive::ClapSerde;
use serde::Deserialize;
use tracing::info;
use tracing_appender::{
    non_blocking::{NonBlocking, WorkerGuard},
    rolling::Rotation,
};

type OptArgs = <Args as ClapSerde>::Opt;

/// parse command line arguments
pub fn parse_args() -> Result<Args> {
    let mut args = Args::default();
    let opt_args = OptArgs::parse();

    let default_config_file = PathBuf::from_str("sidecar.toml").unwrap();
    let config_file = opt_args
        .config
        .as_ref()
        .or(Some(&default_config_file))
        .filter(|p| p.exists());

    if let Some(config) = config_file {
        info!("use config file {}", config.display());
        let file_args: OptArgs = toml::from_str(
            &std::fs::read_to_string(config)
                .context("Failed to read config file")?,
        )
        .context("Failed to parse config file")?;
        args = args.merge(file_args);
    }

    args = args.merge(opt_args);

    args.validate().context("Failed to validate config")?;
    Ok(args)
}

static VERSION: LazyLock<String> = LazyLock::new(|| {
    let git_version = bugreport::git_version!(fallback = "");

    if git_version.is_empty() {
        crate_version!().to_string()
    } else {
        format!("{} ({})", crate_version!(), git_version)
    }
});

#[derive(ClapSerde, Parser, Deserialize, Debug)]
#[command(version = VERSION.as_str(), about)]
pub struct Args {
    /// Name of config file (default: sidecar.toml)
    #[arg(short = 'f', long = "config")]
    #[default(PathBuf::new())]
    pub config: PathBuf,

    /// Unlink shared memory object if it exists
    #[arg(short = 'u', long = "unlink", action = clap::ArgAction::SetTrue)]
    pub unlink: bool,

    /// Name of the shared memory object
    #[arg(short = 'n', long = "name")]
    pub shmfile: String,

    /// Identifier for the shared memory object
    #[arg(short, long)]
    #[default(Ipv4Addr::new(0, 0, 0, 0))]
    pub ident: Ipv4Addr,

    /// listen address for the mesh endpoint
    #[arg(short, long)]
    #[default(":6081".to_owned())]
    pub listen: String,

    /// location of consul server
    #[arg(short = 'r', long = "resolve")]
    #[default("".to_owned())]
    // not support for uri now
    // #[serde(with = "http_serde::uri")]
    // pub consul: http::uri::Uri,
    pub consul: String,

    /// Size of the request (sidecar to host) buffer for shared memory
    #[arg(short = 's', long = "sq")]
    #[default(get_page_size())]
    pub sq_bufsize: usize,

    /// Size of the response (host to sidecar) buffer for shared memory
    #[arg(short = 'c', long = "cq")]
    #[default(get_page_size())]
    pub cq_bufsize: usize,

    /// Count of worker threads (0 means autodetect)
    #[arg(short = 'j', long)]
    #[default(0)]
    pub threads: usize,

    /// Size of resolver mask cache
    #[arg(long)]
    #[default(10000)]
    pub resolver_cache: usize,

    /// live time (as second) of resolver mask cache
    #[arg(long)]
    #[default(1)]
    pub resolver_livetime: u64,

    /// timeout (as millisecond) for pending connection
    #[arg(long)]
    #[default(500)]
    pub pending_timeout: u64,

    /// timeout (as millisecond) for idle connection
    #[arg(long)]
    #[default(60_000)]
    pub idle_timeout: u64,

    /// prometheus endpoint
    #[arg(short = 'p', long = "prometheus")]
    pub prometheus: Option<SocketAddr>,

    /// prometheus push gateway
    #[arg(skip)]
    pub prometheus_push: Option<PrometheusPush>,

    /// local ident mapping
    #[arg(skip)]
    pub nodes: Vec<Node>,

    /// log file path
    #[arg(skip)]
    pub log: Option<LogFileInfo>,

    #[arg(skip)]
    pub rate_limit: Option<RateLimitInfo>,

    /// host command line to run after sidecar started
    #[arg(last = false)]
    #[default(vec![])]
    pub host_cmd: Vec<String>,
}

impl Args {
    pub fn validate(&self) -> Result<()> {
        if self.ident.to_bits() == 0 {
            bail!("ident required");
        }
        if self.shmfile.is_empty() {
            bail!("shmfile required");
        }
        Ok(())
    }
}

/// local ident -> node address mapping
#[derive(Deserialize, Clone, Debug)]
pub struct Node {
    pub ident: Ipv4Addr,
    pub addr: SocketAddr,
}

/// prometheus push gateway configuration
#[derive(Deserialize, Clone, Debug)]
pub struct PrometheusPush {
    pub endpoint: String,
    pub interval: Duration,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// log file configuration
#[derive(Deserialize, Clone, Debug)]
pub struct LogFileInfo {
    pub directory: PathBuf,
    pub prefix: String,
    pub suffix: Option<String>,
    pub max_count: usize,
    #[serde(deserialize_with = "parse_rotation")]
    pub rotation: Rotation,
}

impl LogFileInfo {
    /// build non-blocking writer from configuration
    pub fn build_writer(
        conf: Option<&Self>,
    ) -> Result<(Option<NonBlocking>, Option<WorkerGuard>)> {
        Ok(conf
            .and_then(|conf| {
                let mut builder = tracing_appender::rolling::Builder::new();
                if let Some(suffix) = &conf.suffix {
                    builder = builder.filename_suffix(suffix);
                }
                Some(
                    builder
                        .filename_prefix(conf.prefix.clone())
                        .max_log_files(conf.max_count)
                        .rotation(conf.rotation.clone())
                        .build(conf.directory.as_path())
                        .context("Failed to build appender"),
                )
            })
            .map_or(Ok(None), |r| r.map(Some))?
            .map(|appender| NonBlocking::new(appender))
            .map(|(non_block, guard)| (Some(non_block), Some(guard)))
            .unwrap_or((None, None)))
    }
}

pub fn parse_rotation<'de, D>(deserializer: D) -> Result<Rotation, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Rotation;

        fn expecting(
            &self,
            formatter: &mut std::fmt::Formatter,
        ) -> std::fmt::Result {
            formatter.write_str("daily | hourly | minutely | never")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            match v.to_lowercase().as_str() {
                "daily" => Ok(Rotation::DAILY),
                "hourly" => Ok(Rotation::HOURLY),
                "minutely" => Ok(Rotation::MINUTELY),
                "never" => Ok(Rotation::NEVER),
                _ => Err(serde::de::Error::custom(format!(
                    "Invalid rotation: {}",
                    v
                ))),
            }
        }
    }

    deserializer.deserialize_str(Visitor)
}

#[derive(Deserialize, Clone, Debug)]
pub struct RateLimitInfo {
    pub max: usize,
    pub initial: usize,
    pub refill: usize,

    #[serde(default = "default_interval")]
    pub interval: Duration,

    pub per_msg: Vec<PerMsgLimitInfo>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PerMsgLimitInfo {
    pub ident: Option<Ipv4Addr>,
    pub body_type: Option<String>,

    pub max: usize,
    pub initial: usize,
    pub refill: usize,

    #[serde(default = "default_interval")]
    pub interval: Duration,
}

fn default_interval() -> Duration {
    Duration::from_secs(1)
}

fn get_page_size() -> usize {
    page_size::get()
}
