use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::LazyLock,
};

use anyhow::{bail, Context, Result};
use clap::{crate_version, Args, CommandFactory, FromArgMatches, Parser};
use clap_merge::ClapMerge;
use kaze_utils::DurationString;
use serde::{Deserialize, Serialize};
use tracing::info;
use tracing_appender::{
    non_blocking::{NonBlocking, WorkerGuard},
    rolling::Rotation,
};

pub fn parse_args() -> Result<Config> {
    let args = Config::command().get_matches();

    let args = if let Some(cfg_path) =
        args.get_one::<PathBuf>("config").filter(|p| p.exists())
    {
        info!("use config file {}", cfg_path.display());
        let mut config: Config = toml::from_str(
            &std::fs::read_to_string(&cfg_path)
                .context("Failed to read config file")?,
        )
        .context("Failed to parse config file")?;
        config.merge(&args);
        config
    } else {
        Config::from_arg_matches(&args).context("Failed to parse config")?
    };

    validate_args(&args)?;
    Ok(args)
}

fn validate_args(args: &Config) -> Result<()> {
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
#[command(version = VERSION.as_str(), about)]
pub struct Config {
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
    pub log: Option<LogFileOptions>,

    /// Name of the shared memory object
    #[command(flatten)]
    pub edge: kaze_edge::Options,

    /// corral config
    #[command(flatten)]
    pub corral: kaze_corral::Options,

    /// rate limit for incomming packets
    #[command(flatten)]
    pub rate_limit: Option<kaze_corral::ratelimit::Options>,

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

/// log file configuration
#[derive(ClapMerge, Args, Serialize, Deserialize, Clone, Debug)]
#[command(next_help_heading = "Log file configurations")]
pub struct LogFileOptions {
    /// log file directory
    #[serde(default = "default_log_dir")]
    #[arg(long = "log-dir", default_value = default_log_dir().into_os_string())]
    #[arg(value_name = "PATH")]
    pub directory: PathBuf,

    /// log file prefix
    #[arg(long = "log", default_value = "{name}_{ident}")]
    pub prefix: String,

    /// log file rotation
    #[serde(default = "default_rotation")]
    #[serde(with = "serde_rotation")]
    #[arg(long = "log-rotation", value_parser = parse_rotation, default_value = "never")]
    pub rotation: Rotation,

    /// log file suffix
    #[arg(long = "log-suffix", default_value = default_suffix().unwrap())]
    #[serde(default = "default_suffix")]
    pub suffix: Option<String>,

    /// log file minimum level
    #[arg(long = "log-level")]
    #[arg(value_name = "LEVEL", default_missing_value = "trace")]
    pub level: Option<String>,

    /// max log file count
    #[arg(long = "log-max-count")]
    #[arg(value_name = "N")]
    pub max_count: Option<usize>,
}

fn default_log_dir() -> PathBuf {
    PathBuf::from("logs")
}

fn default_suffix() -> Option<String> {
    Some(".log".to_string())
}

fn default_rotation() -> Rotation {
    Rotation::NEVER
}

fn parse_rotation(s: &str) -> Result<Rotation> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "daily" => Rotation::DAILY,
        "hourly" => Rotation::HOURLY,
        "minutely" => Rotation::MINUTELY,
        "never" => Rotation::NEVER,
        _ => bail!("invalid rotation {}", s),
    })
}

mod serde_rotation {
    use serde::{de::Visitor, Deserializer, Serializer};
    use tracing_appender::rolling::Rotation;

    pub fn serialize<S>(
        rotation: &Rotation,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = match rotation {
            &Rotation::DAILY => "daily",
            &Rotation::HOURLY => "hourly",
            &Rotation::MINUTELY => "minutely",
            &Rotation::NEVER => "never",
        };
        serializer.serialize_str(str)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Rotation, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(RotationVisitor)
    }

    struct RotationVisitor;

    impl<'de> Visitor<'de> for RotationVisitor {
        type Value = Rotation;

        fn expecting(
            &self,
            formatter: &mut std::fmt::Formatter,
        ) -> std::fmt::Result {
            formatter.write_str("daily | hourly | minutely | never")
        }

        fn visit_str<E>(self, value: &str) -> Result<Rotation, E>
        where
            E: serde::de::Error,
        {
            super::parse_rotation(value).map_err(serde::de::Error::custom)
        }
    }
}

impl LogFileOptions {
    /// build non-blocking writer from configuration
    pub fn build_writer(
        root: &Config,
        conf: Option<&Self>,
    ) -> Result<(Option<NonBlocking>, Option<WorkerGuard>)> {
        Ok(conf
            .and_then(|conf| {
                let mut builder = tracing_appender::rolling::Builder::new();
                if let Some(suffix) = &conf.suffix {
                    builder = builder.filename_suffix(suffix);
                }
                if let Some(size) = conf.max_count {
                    builder = builder.max_log_files(size);
                }
                Some(
                    builder
                        .filename_prefix(Self::format_log_name(
                            root,
                            &conf.prefix,
                        ))
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

    fn format_log_name(root: &Config, prefix: &str) -> String {
        prefix
            .replace("{name}", root.edge.name.as_str())
            .replace("{ident}", &root.edge.ident.to_string())
            .replace("{version}", VERSION.as_str())
    }
}

#[derive(ClapMerge, Args, Serialize, Deserialize, Clone, Debug)]
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
#[command(next_help_heading = "Prometheus metrics configurations")]
pub struct PromethusConfig {
    /// prometheus metrics endpoint
    #[arg(
        id = "metrics",
        long = "metrics",
        default_missing_value = default_metrics_listening().to_string()
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
    Config::command().debug_assert();
}
