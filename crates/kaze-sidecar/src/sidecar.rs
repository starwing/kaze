use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::Context as _;
use kaze_plugin::clap::{crate_version, Parser};
use kaze_plugin::tokio_graceful::Shutdown;
use tokio::task::JoinSet;
use tower::layer::util::Identity;
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;

use kaze_plugin::serde::{Deserialize, Serialize};
use kaze_plugin::Context;

use crate::builder::{SidecarBuilder, StateFilter};

/// The kaze sidecar for host
#[derive(Parser, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "kaze_plugin::serde")]
#[command(version = VERSION.as_str(), about)]
pub struct Options {
    /// Name of config file (default: sidecar.toml)
    #[arg(short, long)]
    #[arg(value_name = "PATH")]
    #[serde(skip)]
    pub config: Option<PathBuf>,

    /// host command line to run after sidecar started
    #[arg(trailing_var_arg = true)]
    #[serde(skip)]
    pub host_cmd: Vec<String>,

    /// Count of worker threads (0 means autodetect)
    #[arg(short = 'j', long)]
    #[arg(value_name = "N")]
    pub threads: Option<usize>,
}

pub struct Sidecar {
    ctx: Context,
    options: Options,
    _unlink_guard: kaze_edge::UnlinkGuard,
    _log_guard: Option<WorkerGuard>,
}

impl Sidecar {
    pub fn builder() -> SidecarBuilder<StateFilter<Identity>> {
        SidecarBuilder::new()
    }

    pub(crate) fn new(
        ctx: Context,
        options: Options,
        _unlink_guard: kaze_edge::UnlinkGuard,
        _log_guard: Option<WorkerGuard>,
    ) -> Self {
        Self {
            ctx,
            options,
            _unlink_guard,
            _log_guard,
        }
    }
    /// get the thread count
    pub fn thread_count(&self) -> Option<usize> {
        self.options.threads
    }

    /// Run the sidecar
    pub async fn run(self) -> anyhow::Result<()> {
        let mut set = JoinSet::new();
        for plugin in self.ctx.components() {
            if let Some(fut) = plugin.run() {
                info!("plugin {} started", plugin.name());
                set.spawn(fut);
            }
        }

        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    return Err(e).context("plugin error");
                }
                Err(e) => {
                    return Err(anyhow::Error::new(e)).context("plugin panic");
                }
            }
        }

        // TODO: delete this after using tracker for exit state.
        info!("sidecar shutdown receiver");
        self.ctx.get::<kaze_edge::Receiver>().unwrap().shutdown()?;
        Ok(())
    }

    /// run the sidecar with shutdown
    pub async fn run_with_shutdown(
        self,
        shutdown: Shutdown,
    ) -> anyhow::Result<()> {
        if let Err(_) = self.ctx.set_shutdown_guard(shutdown.guard()) {
            return Err(anyhow::anyhow!("failed to set shutdown guard"));
        }
        self.run().await
    }
}

pub(crate) static VERSION: LazyLock<String> = LazyLock::new(|| {
    let git_version = bugreport::git_version!(fallback = "");

    if git_version.is_empty() {
        crate_version!().to_string()
    } else {
        format!("{} ({})", crate_version!(), git_version)
    }
});
