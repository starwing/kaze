use anyhow::Context as _;
use tower::{util::BoxCloneSyncService, ServiceBuilder};
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    layer::SubscriberExt as _, util::SubscriberInitExt as _, EnvFilter,
};

use kaze_plugin::{
    protocol::{
        message::Message,
        service::{SinkMessage, ToMessageService},
    },
    service::{AsyncService, ServiceExt as _},
    ContextBuilder, PluginFactory,
};
use kaze_resolver::ResolverExt as _;

use crate::{
    config::ConfigMap,
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

    pub fn build(mut self) -> anyhow::Result<Sidecar>
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
            .take::<kaze_edge::Options>()
            .unwrap()
            .build()
            .unwrap();
        let _unlink_guard = edge.unlink_guard();
        let (tx, rx) = edge.into_split();

        // create corral instance
        let corral = self.config.take::<corral::Options>().unwrap().build()?;

        // create the base resolver (local) instance
        let resolver = futures::executor::block_on(async {
            self.config
                .take::<kaze_resolver::LocalOptions>()
                .unwrap()
                .build()
                .await
        });

        // create the tracker instance
        let tracker = self
            .config
            .take::<tracker::Options>()
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
        let mut config = self.config;
        let options = config.take::<Options>().unwrap();
        let ctx = cb.register(Host::new(options.host_cmd.clone())).build();
        ctx.sink()
            .set(BoxCloneSyncService::new(pipeline.into_tower()));

        Ok(Sidecar::new(ctx, options, _unlink_guard, _log_guard))
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
