use anyhow::Context as _;
use kaze_plugin::tokio_graceful::Shutdown;
use tokio::task::JoinSet;
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;

use kaze_plugin::Context;

use crate::options::{FilterEnd, Options, OptionsBuilder};

pub struct Sidecar {
    ctx: Context,
    options: Options,
    _unlink_guard: Option<kaze_edge::UnlinkGuard>,
    _log_guard: Option<WorkerGuard>,
}

impl Sidecar {
    pub fn options() -> OptionsBuilder<FilterEnd> {
        Options::builder()
    }

    pub(crate) fn new(
        ctx: Context,
        options: Options,
        _unlink_guard: Option<kaze_edge::UnlinkGuard>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_map::default_config;
    use std::time::Duration;

    #[tokio::test]
    async fn test_sidecar_new() {
        let sidecar = Sidecar::options()
            .into_builder_from_args(vec!["kaze", "-n", "kaze_test_new"])
            .unwrap()
            .build()
            .unwrap();

        assert!(sidecar.thread_count().is_none());
    }

    #[tokio::test]
    async fn test_thread_count() {
        let ctx = Context::builder().build();
        let options = Options {
            threads: Some(4),
            ..default_config()
        };
        let sidecar = Sidecar::new(ctx, options, None, None);
        assert_eq!(sidecar.thread_count(), Some(4));
    }

    #[tokio::test]
    async fn test_run_with_shutdown() {
        let sidecar = Sidecar::options()
            .into_builder_from_args(vec!["kaze", "-n", "kaze_test_run"])
            .unwrap()
            .build()
            .unwrap();

        let notify = tokio::time::sleep(Duration::from_millis(100));
        let shutdown = Shutdown::new(notify);

        // Should complete without error
        let ctx = sidecar.ctx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            ctx.trigger_exiting();
        });
        let result = sidecar.run_with_shutdown(shutdown).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_shutdown_guard_error() {
        // Create a context that will reject the shutdown guard
        let ctx = Context::mock();
        // Already set a shutdown guard to force error
        let dummy_shutdown = Shutdown::default();
        ctx.set_shutdown_guard(dummy_shutdown.guard()).unwrap();

        let options = default_config();
        let sidecar = Sidecar::new(ctx, options, None, None);

        let shutdown2 = Shutdown::default();
        let result = sidecar.run_with_shutdown(shutdown2).await;
        assert!(result.is_err());
    }
}
