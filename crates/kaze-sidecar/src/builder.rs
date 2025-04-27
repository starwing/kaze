use anyhow::Context as _;
use clap::{CommandFactory as _, FromArgMatches as _};
use tower::{
    layer::util::{Identity, Stack},
    util::BoxCloneSyncService,
    Layer, ServiceBuilder,
};
use tracing::{level_filters::LevelFilter, subscriber::DefaultGuard};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    layer::SubscriberExt as _, util::SubscriberInitExt as _, EnvFilter,
};

use kaze_plugin::{
    protocol::{
        message::Message,
        service::{SinkMessage, ToMessageService},
    },
    serde::{Deserialize, Serialize},
    service::{AsyncService, FilterChain, ServiceExt as _},
    tokio_graceful::Shutdown,
    Context, ContextBuilder, PipelineService, PluginFactory,
};
use kaze_resolver::{Resolver, ResolverExt as _};

use crate::{
    config::{ConfigBuilder, ConfigFileBuilder, ConfigMap},
    plugins::{corral, log},
    sidecar::{Sidecar, VERSION},
    Options,
};

pub struct SidecarBuilder<State> {
    state: State,
    shutdown: Shutdown,
    temp_log: DefaultGuard,
}

impl SidecarBuilder<StateFilter<Identity>> {
    pub fn new(shutdown: Shutdown) -> Self {
        // init intial log
        let temp_log = tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(env_filter())
            .set_default();

        let cfg = Self::new_config_builder(Options::command());
        Self {
            state: StateFilter::new(cfg),
            shutdown,
            temp_log,
        }
    }

    fn new_config_builder(cmd: clap::Command) -> ConfigBuilder {
        ConfigBuilder::new(cmd)
            .add::<kaze_edge::Options>("edge")
            .add::<corral::Options>("corral")
            .add::<kaze_resolver::LocalOptions>("local")
    }
}

impl<L> SidecarBuilder<StateFilter<L>> {
    pub fn add<T>(
        self,
        name: impl ToString,
    ) -> SidecarBuilder<StateFilter<Stack<PluginCreator<T>, L>>>
    where
        T: PluginFactory
            + for<'a> Deserialize<'a>
            + Serialize
            + clap::Args
            + 'static,
    {
        SidecarBuilder {
            state: self.state.add(name),
            temp_log: self.temp_log,
            shutdown: self.shutdown,
        }
    }

    pub fn debug_assert(mut self) -> Self {
        self.state = self.state.debug_assert();
        self
    }

    pub fn build_filter<F>(
        self,
    ) -> anyhow::Result<
        SidecarBuilder<
            StatePipeline<
                F,
                impl AsyncService<
                        Message,
                        Response = Option<()>,
                        Error = anyhow::Error,
                    > + Clone,
                impl Resolver + Clone,
            >,
        >,
    >
    where
        L: Layer<
            anyhow::Result<(FilterIdentity, ConfigMap, ContextBuilder)>,
            Service = anyhow::Result<(F, ConfigMap, ContextBuilder)>,
        >,
    {
        let (mut config, filter_builder) = self.state.build_config()?;

        // make up the log guard
        let _log_guard = config
            .get::<log::Options>()
            .map(|log| {
                Self::init_log(
                    config.get::<kaze_edge::Options>().unwrap(),
                    log,
                )
                .context("failed to init log")
            })
            .transpose()?;

        // create edge intstance
        let edge = config
            .take::<kaze_edge::Options>()
            .unwrap()
            .build()
            .unwrap();
        let _unlink_guard = edge.unlink_guard();
        let (tx, rx) = edge.into_split();

        // create corral instance
        let corral = config.take::<corral::Options>().unwrap().build()?;

        // create the base resolver (local) instance
        let resolver = futures::executor::block_on(async {
            config
                .take::<kaze_resolver::LocalOptions>()
                .unwrap()
                .build()
                .await
        });

        // construct the service stack
        let cb = Context::builder()
            .register(resolver.clone())
            .register(corral.clone())
            .register(tx.clone())
            .register(rx);

        let (filter, config, cb) = filter_builder.build(config, cb)?;

        let raw_sink = ServiceBuilder::new()
            .layer(corral.into_filter())
            .layer(tx.into_filter())
            .service(SinkMessage.map_response(|()| Some(())));

        Ok(SidecarBuilder {
            state: StatePipeline {
                filter,
                raw_sink,
                config,
                cb,
                resolver,
                _unlink_guard,
                _log_guard,
            },
            shutdown: self.shutdown,
            temp_log: self.temp_log,
        })
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

impl<F, RS, R> SidecarBuilder<StatePipeline<F, RS, R>> {
    pub fn build_sidecar<FS>(self) -> anyhow::Result<Sidecar>
    where
        F: Layer<RS, Service = FS>,
        FS: AsyncService<Message, Response = Option<()>, Error = anyhow::Error>
            + Sync
            + Send
            + Clone
            + 'static,
        RS: AsyncService<Message>,
        R: Resolver + Clone,
    {
        let sink = construct_service(
            self.state.filter,
            self.state.resolver,
            self.state.raw_sink,
        );

        fn construct_service<F, FS, RS>(
            filter: F,
            resolver: impl Resolver + Clone,
            raw_sink: RS,
        ) -> PipelineService
        where
            F: Layer<RS, Service = FS>,
            RS: AsyncService<Message>,
            FS: AsyncService<
                    Message,
                    Response = Option<()>,
                    Error = anyhow::Error,
                > + Sync
                + Send
                + Clone
                + 'static,
        {
            let sink = ServiceBuilder::new()
                .layer(ToMessageService.into_layer())
                .layer(resolver.into_service().into_filter())
                .layer(filter)
                .service(raw_sink)
                .map_response(|_| ());

            // construct the context
            BoxCloneSyncService::new(sink.into_tower())
        }

        let ctx = self.state.cb.build(self.shutdown.guard());
        ctx.sink().set(sink);

        let mut config = self.state.config;
        let options = config.take::<Options>().unwrap();
        Ok(Sidecar::new(
            ctx,
            options,
            self.shutdown,
            self.state._unlink_guard,
            self.state._log_guard,
        ))
    }
}

fn env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .with_env_var("KAZE_LOG")
        .from_env_lossy()
}

pub struct StateFilter<L> {
    config_builder: ConfigBuilder,
    layer: L,
}

impl StateFilter<Identity> {
    fn new(cfg: ConfigBuilder) -> Self {
        let layer = Identity::new();
        Self {
            config_builder: cfg,
            layer,
        }
    }
}

impl<L> StateFilter<L> {
    fn add<T>(
        self,
        name: impl ToString,
    ) -> StateFilter<Stack<PluginCreator<T>, L>>
    where
        T: PluginFactory
            + for<'a> Deserialize<'a>
            + Serialize
            + clap::Args
            + 'static,
    {
        let cfg = self.config_builder.add::<T>(name);
        let stack = Stack::new(PluginCreator::<T>::new(), self.layer);
        StateFilter {
            config_builder: cfg,
            layer: stack,
        }
    }

    fn debug_assert(mut self) -> Self {
        self.config_builder = self.config_builder.debug_assert();
        self
    }

    fn build_config(self) -> anyhow::Result<(ConfigMap, FilterBuilder<L>)> {
        let merger = self.config_builder.get_matches();
        let options = Options::from_arg_matches(merger.arg_matches())
            .context("failed to parse options")?;

        let mut filefinder = ConfigFileBuilder::default();
        if let Some(path) = &options.config {
            filefinder = filefinder.add_file(path.clone());
        }

        let content = filefinder.build().context("build file finder error")?;
        let mut config =
            merger.build(content).context("merger build error")?;
        config.insert(options);
        Ok((config, FilterBuilder::new(self.layer)))
    }
}

pub struct FilterBuilder<L> {
    layer: L,
}

impl<L> FilterBuilder<L> {
    fn new(layer: L) -> Self {
        Self { layer }
    }

    fn build<F>(
        self,
        config: ConfigMap,
        cb: ContextBuilder,
    ) -> anyhow::Result<(F, ConfigMap, ContextBuilder)>
    where
        L: Layer<
            anyhow::Result<(FilterIdentity, ConfigMap, ContextBuilder)>,
            Service = anyhow::Result<(F, ConfigMap, ContextBuilder)>,
        >,
    {
        self.layer.layer(Ok((FilterIdentity, config, cb)))
    }
}

pub struct PluginCreator<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T, Filter> Layer<anyhow::Result<(Filter, ConfigMap, ContextBuilder)>>
    for PluginCreator<T>
where
    T: PluginFactory,
{
    type Service = anyhow::Result<(
        FilterChain<T::Plugin, Filter>,
        ConfigMap,
        ContextBuilder,
    )>;

    fn layer(
        &self,
        next: anyhow::Result<(Filter, ConfigMap, ContextBuilder)>,
    ) -> Self::Service {
        if let Err(err) = next {
            return Err(err);
        }
        let (filter, mut config, cb) = next.unwrap();
        let (plugin, cb) = self.create_plugin(&mut config, cb)?;
        let filter = FilterChain::new(plugin.clone(), filter);
        Ok((filter, config, cb.register(plugin)))
    }
}

impl<T> PluginCreator<T> {
    pub fn new() -> Self {
        PluginCreator {
            _marker: std::marker::PhantomData,
        }
    }

    pub fn create_plugin(
        &self,
        config: &mut ConfigMap,
        cb: kaze_plugin::ContextBuilder,
    ) -> anyhow::Result<(T::Plugin, ContextBuilder)>
    where
        T: PluginFactory,
    {
        if let Some(opt) = config.take::<T>() {
            let plugin = opt.build().context("failed to build plugin")?;
            let cb = cb.register(plugin.clone());
            return Ok((plugin, cb));
        }
        panic!("Plugin {} not found", std::any::type_name::<T>());
    }
}

#[derive(Clone, Copy)]
pub struct FilterIdentity;

impl<Request: Send + 'static> AsyncService<Request> for FilterIdentity {
    type Response = Option<Request>;
    type Error = anyhow::Error;

    async fn serve(&self, req: Request) -> anyhow::Result<Self::Response> {
        Ok(Some(req))
    }
}

pub struct StatePipeline<F, RS, R> {
    filter: F,
    raw_sink: RS,
    config: ConfigMap,
    cb: ContextBuilder,
    resolver: R,
    _unlink_guard: kaze_edge::UnlinkGuard,
    _log_guard: Option<WorkerGuard>,
}

#[cfg(test)]
mod tests {
    use kaze_plugin::Context;

    use crate::plugins::{log, prometheus, ratelimit};

    use super::*;

    fn new_pipeline_builder() -> StateFilter<Identity> {
        let cfg = ConfigBuilder::new(Options::command());
        StateFilter::new(cfg)
    }

    #[test]
    fn test_pipeline_builder_new() {
        let builder = new_pipeline_builder();
        builder.config_builder.debug_assert();
    }

    #[test]
    fn test_pipeline_builder_add() {
        let builder = new_pipeline_builder()
            .add::<log::Options>("log")
            .add::<ratelimit::Options>("rate_limit")
            .add::<prometheus::Options>("prometheus");
        builder.config_builder.debug_assert();
    }

    #[test]
    fn test_pipeline_builder_build() {
        let builder = new_pipeline_builder()
            .add::<log::Options>("log")
            .add::<ratelimit::Options>("rate_limit")
            .add::<prometheus::Options>("prometheus");

        let (config, filter_builder) = builder.build_config().unwrap();
        assert!(config.get::<log::Options>().is_some());
        assert!(config.get::<ratelimit::Options>().is_some());
        assert!(config.get::<prometheus::Options>().is_some());
        let filter = filter_builder.build(config, Context::builder());
        assert!(filter.is_ok());
    }

    #[test]
    fn test_plugin_creator_new() {
        let creator = PluginCreator::<prometheus::Options>::new();
        assert_eq!(std::mem::size_of_val(&creator._marker), 0);
    }

    #[test]
    fn test_build_sidecar() {
        // Create a shutdown handle requires tokio runtime
        let shutdown = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(async { Shutdown::default() });

        let sidecar = SidecarBuilder::new(shutdown)
            .add::<log::Options>("log")
            .add::<ratelimit::Options>("rate_limit")
            .add::<prometheus::Options>("prometheus")
            .debug_assert()
            .build_filter()
            .unwrap()
            .build_sidecar();
        assert!(sidecar.is_ok());
    }
}
