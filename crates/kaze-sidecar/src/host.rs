use std::sync::{Arc, OnceLock};

use anyhow::Context as _;
use kaze_plugin::Plugin;
use tokio::select;
use tracing::info;

#[derive(Clone)]
pub struct Host {
    inner: Arc<Inner>,
}

struct Inner {
    ctx: OnceLock<kaze_plugin::Context>,
    host_cmd: Vec<String>,
}

impl Host {
    pub(crate) fn new(host_cmd: Vec<String>) -> Self {
        Self {
            inner: Arc::new(Inner {
                ctx: OnceLock::new(),
                host_cmd,
            }),
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let host_cmd = &self.inner.host_cmd;
        let mut cmd = tokio::process::Command::new(&host_cmd[0]);
        cmd.args(&host_cmd[1..]);
        let mut host = cmd.spawn().context("failed to spawn host command")?;
        info!(cmd = ?host_cmd, "host command started");

        // wait for host command to finish
        info!("wait for host exiting");
        let is_shutdown = select! {
            status = host.wait() => {
                info!(status = status?.code().unwrap_or(-1),
                    "host command exited");
                self.context().trigger_exiting();
                false
            }
            _ = self.context().shutdwon_triggered() => {
                true
            }
        };

        if is_shutdown {
            select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    info!("host command exit timeout");
                    host.kill().await?;
                }
                status = host.wait() => {
                    info!(exit_code = status?.code().unwrap_or(-1),
                        "host command exited");
                }
            }
        }
        Ok(())
    }
}

impl Plugin for Host {
    fn context_storage(&self) -> Option<&OnceLock<kaze_plugin::Context>> {
        Some(&self.inner.ctx)
    }
    fn run(&self) -> Option<kaze_plugin::PluginRunFuture> {
        let host = self.clone();
        if self.inner.host_cmd.is_empty() {
            info!("No host command provided, skipping host running");
            // send exit packet even if no host command is provided
            return None;
        }
        Some(Box::pin(async move { host.start().await }))
    }
}

#[cfg(test)]
mod tests {
    use kaze_plugin::{config_map::ConfigMap, Context};

    use super::*;

    #[tokio::test]
    async fn test_new_host() {
        let cmd = vec!["echo".to_string(), "hello".to_string()];
        let host = Host::new(cmd.clone());
        assert_eq!(host.inner.host_cmd, cmd);
    }

    #[tokio::test]
    async fn test_empty_host_cmd() {
        let host = Host::new(vec![]);
        assert!(host.run().is_none());
    }

    #[tokio::test]
    async fn test_host_start_success() {
        let cmd = vec!["echo".to_string(), "hello".to_string()];
        let host = Host::new(cmd);

        // Create a mock Context
        let ctx = Context::builder().register(host).build(ConfigMap::mock());

        let host = ctx.get::<Host>().unwrap();
        let result = host.start().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_host_invalid_command() {
        let cmd = vec!["non_existent_command".to_string()];
        let host = Host::new(cmd);

        // Create a mock Context
        let ctx = Context::builder().register(host).build(ConfigMap::mock());

        let host = ctx.get::<Host>().unwrap();
        let result = host.start().await;
        assert!(result.is_err());
    }
}
