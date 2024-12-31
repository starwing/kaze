use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
    sync::LazyLock,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use clap::{crate_version, Parser};
use serde::{Deserialize, Serialize};
use tracing::info;
use tracing_appender::{
    non_blocking::{NonBlocking, WorkerGuard},
    rolling::Rotation,
};

/// parse command line arguments
pub fn parse_args() -> Result<Args> {
    type OptArgs = <Args as ClapSerde>::Opt;
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

#[derive(Parser)]
#[command(version = VERSION.as_str(), about)]
pub struct Args {
    /// Name of config file (default: sidecar.toml)
    #[arg(short, long, default_value = "sidecar.toml")]
    pub config: PathBuf,

    /// Unlink shared memory object if it exists
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    pub unlink: bool,

    /// Name of the shared memory object
    #[arg(short, long)]
    pub name: Option<String>,

    /// listen address for the mesh endpoint
    #[arg(short, long)]
    pub listen: Option<String>,

    /// Count of worker threads (0 means autodetect)
    pub threads: Option<usize>,

    /// host command line to run after sidecar started
    #[arg(last = false)]
    pub host_cmd: Vec<String>,
}

pub fn parse_rotation(s: &str) -> Result<Rotation> {
    match s.to_lowercase().as_str() {
        "daily" => Ok(Rotation::DAILY),
        "hourly" => Ok(Rotation::HOURLY),
        "minutely" => Ok(Rotation::MINUTELY),
        "never" => Ok(Rotation::NEVER),
        _ => bail!("Invalid rotation: {}", s),
    }
}

pub fn serde_parse_rotation<'de, D>(
    deserializer: D,
) -> Result<Rotation, D::Error>
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
            parse_rotation(v).map_err(serde::de::Error::custom)
        }
    }

    deserializer.deserialize_str(Visitor)
}

fn get_page_size() -> usize {
    page_size::get()
}
