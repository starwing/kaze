use anyhow::Context as _;
use kaze_plugin::tokio_graceful::Shutdown;
use tokio::task::JoinSet;
use tower::layer::util::Identity;
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;

use kaze_plugin::Context;

use crate::options::{Options, OptionsBuilder};

pub struct Sidecar {
    ctx: Context,
    options: Options,
    _unlink_guard: kaze_edge::UnlinkGuard,
    _log_guard: Option<WorkerGuard>,
}

impl Sidecar {
    pub fn options() -> OptionsBuilder<Identity> {
        Options::builder()
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
