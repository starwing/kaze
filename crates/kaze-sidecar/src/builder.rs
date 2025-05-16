use anyhow::Context as _;
use tower::{util::BoxCloneSyncService, ServiceBuilder};
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    layer::SubscriberExt as _, util::SubscriberInitExt as _, EnvFilter,
};

use kaze_plugin::{
    config_map::ConfigMap,
    protocol::{
        message::Message,
        service::{SinkMessage, ToMessageService},
    },
    service::{AsyncService, ServiceExt as _},
    ContextBuilder, PluginFactory,
};
use kaze_resolver::ResolverExt as _;

use crate::{
    host::Host,
    options::{Options, VERSION},
    plugins::{corral, log, tracker},
    sidecar::Sidecar,
};

pub struct SidecarBuilder<F> {
    filter: F,
    config: ConfigMap,
    ctx_builder: ContextBuilder,
}

impl<F> SidecarBuilder<F> {
    pub(crate) fn new(
        filter: F,
        config: ConfigMap,
        ctx_builder: ContextBuilder,
    ) -> Self {
        Self {
            filter,
            config,
            ctx_builder,
        }
    }

    pub fn config(self) -> ConfigMap {
        self.config
    }

    pub fn build(self) -> anyhow::Result<Sidecar>
    where
        F: AsyncService<
                Message,
                Response = Option<Message>,
                Error = anyhow::Error,
            > + Clone
            + Send
            + Sync
            + 'static,
    {
        // make up the log guard
        let _log_guard = self
            .config
            .get::<log::Options>()
            .map(|log| {
                Self::init_log(
                    self.config.get::<kaze_edge::Options>().unwrap(),
                    log,
                )
                .context("failed to init log")
            })
            .transpose()?;

        // create edge intstance
        let edge = self
            .config
            .get::<kaze_edge::Options>()
            .unwrap()
            .build()
            .context("Failed to create edge")?;
        let _unlink_guard = edge.unlink_guard();
        let (tx, rx) = edge.into_split();

        // create corral instance
        let corral = self.config.get::<corral::Options>().unwrap().build()?;

        // create the base resolver (local) instance
        let resolver = futures::executor::block_on(async {
            self.config
                .get::<kaze_resolver::LocalOptions>()
                .unwrap()
                .build()
                .await
        });

        // create the tracker instance
        let tracker = self
            .config
            .get::<tracker::Options>()
            .unwrap()
            .build()
            .context("failed to build tracker")?;

        // construct the service stack
        let cb = self
            .ctx_builder
            .register(tracker.clone())
            .register(resolver.clone())
            .register(corral.clone())
            .register(tx.clone())
            .register(rx);

        let pipeline = ServiceBuilder::new()
            .layer(ToMessageService.into_layer())
            .layer(resolver.into_service().into_filter())
            .layer(tracker.into_filter())
            .layer(self.filter.into_filter())
            .layer(corral.into_filter())
            .layer(tx.into_filter())
            .service(SinkMessage.map_response(|_| Some(())))
            .map_response(|_| ());

        // construct the context
        let config = self.config;
        let options = config.get::<Options>().unwrap();
        let ctx = cb
            .register(Host::new(options.host_cmd.clone()))
            .build(ConfigMap::mock());
        ctx.sink()
            .set(BoxCloneSyncService::new(pipeline.into_tower()));

        Ok(Sidecar::new(ctx, config, Some(_unlink_guard), _log_guard))
    }

    fn init_log(
        edge: &kaze_edge::Options,
        log: &log::Options,
    ) -> anyhow::Result<WorkerGuard> {
        let expander = |prefix: &str| -> String {
            prefix
                .replace("{name}", edge.name.as_str())
                .replace("{ident}", &edge.ident.to_string())
                .replace("{version}", VERSION.as_str())
        };
        let (non_block, guard) = log::Options::build_writer(log, expander)
            .context("failed to build log")?;

        // install tracing with configuration
        let result = tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(
                tracing_subscriber::fmt::layer()
                    .pretty()
                    .with_ansi(false)
                    .with_writer(non_block),
            )
            .with(env_filter())
            .try_init();

        if let Err(err) = result {
            // FIXME: work around for tracing-log double setting.
            if err.to_string().contains("SetLoggerError") {}
        } else {
            result?;
        }

        Ok(guard)
    }
}

fn env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .with_env_var("KAZE_LOG")
        .from_env_lossy()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaze_plugin::config_map::default_config;
    use kaze_plugin::Context;
    use std::{env::temp_dir, net::Ipv4Addr};

    #[derive(Clone, Copy)]
    struct TestFilter;

    impl AsyncService<Message> for TestFilter {
        type Response = Option<Message>;
        type Error = anyhow::Error;

        async fn serve(
            &self,
            req: Message,
        ) -> anyhow::Result<Option<Message>> {
            Ok(Some(req))
        }
    }

    fn create_test_config() -> ConfigMap {
        let mut config = ConfigMap::mock();
        let temp_dir = temp_dir();

        // Edge options
        let edge_opts = kaze_edge::Options {
            name: "test-sidecar".to_string(),
            ident: Ipv4Addr::new(0, 0, 0, 1),
            bufsize: 1024,
            unlink: true,
        };
        config.insert(edge_opts);

        // Local resolver options
        config.insert(default_config::<kaze_resolver::LocalOptions>());

        // Log options
        let log_opts = log::Options {
            directory: temp_dir.clone(),
            prefix: "test.log".to_string(),
            ..default_config()
        };
        config.insert(log_opts);

        // Corral options
        config.insert(default_config::<corral::Options>());

        // Tracker options
        config.insert(default_config::<tracker::Options>());

        // Sidecar options
        let sidecar_opts = Options {
            host_cmd: vec!["echo".to_string()],
            ..default_config()
        };
        config.insert(sidecar_opts);

        config
    }

    #[test]
    fn test_sidecar_builder() {
        let config = create_test_config();
        let ctx_builder = Context::builder();

        let builder = SidecarBuilder::new(TestFilter, config, ctx_builder);
        let sidecar = builder.build();

        assert!(sidecar.is_ok());
    }

    #[test]
    fn test_env_filter() {
        let filter = env_filter();
        assert!(filter.to_string().contains("trace"));
    }

    #[test]
    fn test_init_log() {
        let temp_dir = temp_dir();

        let edge_opts = kaze_edge::Options {
            name: "test-log".to_string(),
            ident: Ipv4Addr::new(0, 0, 0, 1),
            ..default_config()
        };

        let log_opts = log::Options {
            directory: temp_dir.clone(),
            prefix: "test.log".to_string(),
            ..default_config()
        };

        let guard =
            SidecarBuilder::<TestFilter>::init_log(&edge_opts, &log_opts);
        assert!(guard.is_ok());
    }
}
