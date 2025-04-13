use std::path::PathBuf;

use anyhow::Context as _;
use kaze_plugin::clap::Args;
use kaze_plugin::serde::{Deserialize, Serialize};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::Rotation;

/// log file configuration
#[derive(Args, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(next_help_heading = "Log file configurations")]
pub struct Options {
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

fn parse_rotation(s: &str) -> anyhow::Result<Rotation> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "daily" => Rotation::DAILY,
        "hourly" => Rotation::HOURLY,
        "minutely" => Rotation::MINUTELY,
        "never" => Rotation::NEVER,
        _ => anyhow::bail!("invalid rotation {}", s),
    })
}

mod serde_rotation {
    use kaze_plugin::serde::{self, de::Visitor, Deserializer, Serializer};
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

impl Options {
    /// build non-blocking writer from configuration
    pub fn build_writer(
        conf: Option<&Self>,
        expander: impl FnOnce(&str) -> String,
    ) -> anyhow::Result<(Option<NonBlocking>, Option<WorkerGuard>)> {
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
                        .filename_prefix(expander(&conf.prefix))
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
