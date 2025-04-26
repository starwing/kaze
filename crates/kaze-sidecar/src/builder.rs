use anyhow::Context as _;
use clap::{CommandFactory as _, FromArgMatches as _};
use tower::{layer::util::Stack, Layer};

use kaze_plugin::{
    serde::{Deserialize, Serialize},
    service::{FilterChain, FilterIdentity},
    ContextBuilder, PluginFactory,
};

use crate::{
    config::{ConfigBuilder, ConfigFileBuilder, ConfigMap},
    Options,
};

/*
pub struct SidecarBuilder<L> {
    cfg: ConfigBuilder,
    layer: L,
    temp_log: DefaultGuard,
}

impl SidecarBuilder<tower::layer::util::Identity> {
    pub fn new() -> Self {
        // init intial log
        let temp_log = tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(env_filter())
            .set_default();

        let cfg = Self::new_config_builder(Options::command());
        let layer = tower::layer::util::Identity::new();

        Self {
            cfg,
            layer,
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

impl<L> SidecarBuilder<L> {
    pub fn add<T>(
        self,
        name: impl ToString,
    ) -> SidecarBuilder<Stack<L, BuilderLayer<T>>>
    where
        T: PluginFactory
            + for<'a> Deserialize<'a>
            + Serialize
            + clap::Args
            + 'static,
    {
        let cfg = self.cfg.add::<T>(name);
        // assume called by:
        // SidecarBuilder::().add::<A>().add::<B>().add::<C>()
        // so the stack is:
        // Stack<Stack<Stack<Identity, A>, B>, C>
        // after call layer(S) on this type gets:
        // PluginLayer<C, PluginLayer<B, PluginLayer<A, S>>>
        // it's PluginLayer<Current, Prev>
        let stack = Stack::new(self.layer, BuilderLayer::<T>::new());
        SidecarBuilder {
            cfg,
            layer: stack,
            temp_log: self.temp_log,
        }
    }
}

impl<L> SidecarBuilder<L> {
    pub fn build(self) -> anyhow::Result<Sidecar>
    where
        L: Layer<PluginIdentity, Service: BuilderPluginMaker>,
    {
        let mut config = Self::new_config_map(self.cfg.debug_assert())?;

        // make up the log guard
        let _log_guard = config
            .take::<log::Options>()
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

        let plugin_maker = self.layer.layer(PluginIdentity);
        let (filter, cb) = plugin_maker.make_plugin(&mut config, cb)?;

        let raw_sink = ServiceBuilder::new()
            .layer(corral.into_filter())
            .layer(tx.into_filter())
            .service(SinkMessage.map_response(|()| Some(())));

        let sink = construct_service(filter, resolver, raw_sink);

        fn construct_service<Request, Response, F, FS, RS>(
            filter: F,
            resolver: impl Resolver + Clone,
            raw_sink: RS,
        ) -> PipelineService
        where
            Request: Send + 'static,
            Response: Send + 'static,
            F: Layer<RS, Service = FS>,
            RS: AsyncService<Request>,
            FS: AsyncService<
                    Message,
                    Response = Option<Response>,
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

        let shutdown = Shutdown::default();
        let ctx = cb.build(shutdown.guard());
        ctx.sink().set(sink);

        let options = config.take::<Options>().unwrap();
        Ok(Sidecar::new(
            ctx,
            options,
            shutdown,
            _unlink_guard,
            _log_guard,
        ))
    }

    fn new_config_map(
        config_builder: ConfigBuilder,
    ) -> anyhow::Result<ConfigMap> {
        let merger = config_builder.get_matches();
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
        Ok(config)
    }

    fn init_log(
        edge: &kaze_edge::Options,
        log: log::Options,
    ) -> anyhow::Result<WorkerGuard> {
        let expander = |prefix: &str| -> String {
            prefix
                .replace("{name}", edge.name.as_str())
                .replace("{ident}", &edge.ident.to_string())
                .replace("{version}", VERSION.as_str())
        };
        let (non_block, guard) = log::Options::build_writer(&log, expander)
            .context("failed to build log")?;

        // install tracing with configuration
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(fmt::layer().with_ansi(false).with_writer(non_block))
            .with(env_filter())
            .init();

        Ok(guard)
    }
}

pub trait BuilderPluginMaker {
    type Filter<Request, Response, S>: Send
        + Sync
        + 'static
        + Layer<
            S,
            Service: Send
                         + Sized
                         + Sync
                         + Clone
                         + AsyncService<
                Request,
                Response = Option<Response>,
                Error = anyhow::Error,
            >,
        >
    where
        S: Clone
            + Sized
            + Sync
            + Send
            + 'static
            + AsyncService<
                Request,
                Response = Option<Response>,
                Error = anyhow::Error,
            >;

    fn make_plugin<Request, Response, S>(
        self,
        cfg: &mut ConfigMap,
        cb: kaze_plugin::ContextBuilder,
    ) -> anyhow::Result<(Self::Filter<Request, Response, S>, ContextBuilder)>
    where
        S: Sync
            + Clone
            + Send
            + 'static
            + AsyncService<
                Request,
                Response = Option<Response>,
                Error = anyhow::Error,
            >;
}

pub struct BuilderLayer<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> BuilderLayer<T> {
    fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, S> tower::layer::Layer<S> for BuilderLayer<T>
where
    T: PluginFactory,
{
    type Service = PluginLayer<T, S>;

    fn layer(&self, inner: S) -> Self::Service {
        PluginLayer::new(inner)
    }
}

pub struct PluginLayer<T, Prev> {
    prev: Prev,
    _marker: std::marker::PhantomData<T>,
}

impl<T, Prev> PluginLayer<T, Prev> {
    fn new(prev: Prev) -> Self {
        Self {
            prev,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Curr, Prev> BuilderPluginMaker for PluginLayer<Curr, Prev>
where
    Curr: PluginFactory,
    Curr::Plugin: Clone
        + AsyncService<
            Message,
            Response = Option<Message>,
            Error = anyhow::Error,
        > + Send
        + Sync
        + 'static,
    Prev: BuilderPluginMaker,
    Prev::Filter<Message, (), Curr::Plugin>: AsyncService<Message, Response = Option<()>, Error = anyhow::Error>
        + Send
        + Sync
        + Clone
        + 'static,
    Prev::Filter<Message, Message, Curr::Plugin>: AsyncService<
            Message,
            Response = Option<Message>,
            Error = anyhow::Error,
        > + Send
        + Sync
        + Clone
        + 'static,
{
    type Filter<
        S: Clone
            + Sync
            + Send
            + 'static
            + AsyncService<Message, Response = Option<()>>,
    > = Either<Prev::Filter<S>, FilterLayer<Prev::Filter<Curr::Plugin>>>;

    fn make_plugin<S>(
        self,
        cfg: &mut ConfigMap,
        cb: kaze_plugin::ContextBuilder,
    ) -> anyhow::Result<(Self::Filter<S>, ContextBuilder)> {
        let (prev_filter, cb) = self.prev.make_plugin(cfg, cb)?;

        if let Some(opt) = cfg.take::<Curr>() {
            let plugin = Curr::build(opt)?;
            let cb = cb.register(plugin.clone());
            return Ok((
                Either::Right(FilterLayer::new(prev_filter.layer(plugin))),
                cb,
            ));
        }

        Ok((Either::Left(prev_filter), cb))
    }
}

#[derive(Clone, Copy)]
pub struct PluginIdentity;

impl BuilderPluginMaker for PluginIdentity {
    type Filter<S> = PluginIdentity;

    fn make_plugin<S>(
        self,
        _cfg: &mut ConfigMap,
        cb: kaze_plugin::ContextBuilder,
    ) -> anyhow::Result<(Self::Filter<S>, ContextBuilder)> {
        Ok((PluginIdentity, cb))
    }
}

impl<S> Layer<S> for PluginIdentity {
    type Service = S;

    fn layer(&self, inner: S) -> Self::Service {
        inner
    }
}

fn env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .with_env_var("KAZE_LOG")
        .from_env_lossy()
}
        */

// ***************************************************************************

pub struct PipelineBuilder<L> {
    cfg: ConfigBuilder,
    layer: L,
}

impl PipelineBuilder<tower::layer::util::Identity> {
    pub fn new() -> Self {
        let cfg = ConfigBuilder::new(Options::command());
        let layer = tower::layer::util::Identity::new();
        Self { cfg, layer }
    }
}

impl<L> PipelineBuilder<L> {
    pub fn add<T>(
        self,
        name: impl ToString,
    ) -> PipelineBuilder<Stack<PluginCreator<T>, L>>
    where
        T: PluginFactory
            + for<'a> Deserialize<'a>
            + Serialize
            + clap::Args
            + 'static,
    {
        let cfg = self.cfg.add::<T>(name);
        let stack = Stack::new(PluginCreator::<T>::new(), self.layer);
        PipelineBuilder { cfg, layer: stack }
    }

    pub fn build<F>(
        self,
        cb: ContextBuilder,
    ) -> anyhow::Result<(F, ConfigMap, ContextBuilder)>
    where
        L: Layer<
            anyhow::Result<(FilterIdentity, ConfigMap, ContextBuilder)>,
            Service = anyhow::Result<(F, ConfigMap, ContextBuilder)>,
        >,
    {
        let config = Self::new_config_map(self.cfg.debug_assert())?;
        self.layer.layer(Ok((FilterIdentity, config, cb)))
    }

    fn new_config_map(
        config_builder: ConfigBuilder,
    ) -> anyhow::Result<ConfigMap> {
        let merger = config_builder.get_matches();
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
        Ok(config)
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
        panic!("Plugin not found");
    }
}
#[cfg(test)]
mod tests {
    use kaze_plugin::Context;

    use crate::plugins::{log, prometheus, ratelimit};

    use super::*;

    #[test]
    fn test_pipeline_builder_new() {
        let builder = PipelineBuilder::new();
        builder.cfg.debug_assert();
    }

    #[test]
    fn test_pipeline_builder_add() {
        let builder = PipelineBuilder::new()
            .add::<log::Options>("log")
            .add::<ratelimit::Options>("rate_limit")
            .add::<prometheus::Options>("prometheus");
        builder.cfg.debug_assert();
    }

    #[test]
    fn test_pipeline_builder_build() {
        let cb = Context::builder();

        let builder = PipelineBuilder::new()
            .add::<log::Options>("log")
            .add::<ratelimit::Options>("rate_limit")
            .add::<prometheus::Options>("prometheus");

        let result = builder.build(cb);
        assert!(result.is_ok());
    }

    #[test]
    fn test_plugin_creator_new() {
        let creator = PluginCreator::<prometheus::Options>::new();
        assert_eq!(std::mem::size_of_val(&creator._marker), 0);
    }
}
