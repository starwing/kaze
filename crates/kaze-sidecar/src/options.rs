use std::{ffi::OsString, path::PathBuf, sync::LazyLock};

use anyhow::Context as _;
use clap::{crate_version, Command, Parser};
use clap::{CommandFactory as _, FromArgMatches as _};
use documented_toml::DocumentedToml;
use kaze_plugin::service::{AsyncService, FilterChain};
use kaze_plugin::{Context, ContextBuilder};
use tower::layer::util::Stack;

use kaze_plugin::{
    serde::{Deserialize, Serialize},
    PluginFactory,
};
use tower::Layer;

use crate::builder::SidecarBuilder;
use crate::plugins::{corral, tracker};
use kaze_plugin::config_map::{ConfigBuilder, ConfigFileBuilder, ConfigMap};

/// The kaze sidecar for host
#[derive(Parser, Serialize, Deserialize, DocumentedToml, Clone, Debug)]
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

impl Options {
    pub fn builder() -> OptionsBuilder<FilterEnd> {
        OptionsBuilder::new(Self::new_config_builder(Self::command()))
    }

    fn new_config_builder(cmd: clap::Command) -> ConfigBuilder {
        // required by all sidecars.
        ConfigBuilder::new(cmd)
            .add::<kaze_resolver::LocalOptions>("local")
            .add::<tracker::Options>("tracker")
            .add::<corral::Options>("corral")
            .add::<kaze_edge::Options>("edge")
    }
}

pub struct OptionsBuilder<L> {
    config_builder: ConfigBuilder,
    layer: L,
}

impl OptionsBuilder<FilterEnd> {
    fn new(cfg: ConfigBuilder) -> Self {
        Self {
            config_builder: cfg,
            layer: FilterEnd,
        }
    }
}

impl<L> OptionsBuilder<L> {
    pub fn add<T>(
        self,
        name: impl ToString,
    ) -> OptionsBuilder<Stack<PluginCreator<T>, L>>
    where
        T: PluginFactory
            + for<'a> Deserialize<'a>
            + Serialize
            + DocumentedToml
            + clap::Args
            + 'static,
    {
        let cfg = self.config_builder.add::<T>(name);
        let stack = Stack::new(PluginCreator::<T>::new(), self.layer);
        OptionsBuilder {
            config_builder: cfg,
            layer: stack,
        }
    }

    pub fn command(&self) -> &Command {
        self.config_builder.command()
    }

    pub fn debug_assert(self) -> Self {
        self.command().clone().debug_assert();
        self
    }

    pub fn into_builder_from_args<I, T, F>(
        self,
        itr: I,
    ) -> anyhow::Result<SidecarBuilder<F>>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
        L: Layer<
            anyhow::Result<(FilterStart, ConfigMap, ContextBuilder)>,
            Service = anyhow::Result<(F, ConfigMap, ContextBuilder)>,
        >,
    {
        let cmd = self.config_builder.command();
        let mut matches = cmd.clone().get_matches_from(itr);
        self.into_builder(&mut matches)
    }

    pub fn into_builder<F>(
        self,
        matches: &mut clap::ArgMatches,
    ) -> anyhow::Result<SidecarBuilder<F>>
    where
        L: Layer<
            anyhow::Result<(FilterStart, ConfigMap, ContextBuilder)>,
            Service = anyhow::Result<(F, ConfigMap, ContextBuilder)>,
        >,
    {
        let options = Options::from_arg_matches(matches)
            .context("failed to parse options")?;

        let mut filefinder = ConfigFileBuilder::default();
        if let Some(path) = &options.config {
            filefinder = filefinder.add_file(path.clone());
        }

        let content = filefinder.build().context("build file finder error")?;
        let mut config = self
            .config_builder
            .build(matches, content)
            .context("merger build error")?;
        config.insert(options);

        let (filter, config, cb) =
            self.layer
                .layer(Ok((FilterStart, config, Context::builder())))?;
        Ok(SidecarBuilder::new(filter, config, cb))
    }
}

pub struct PluginCreator<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T, L> Layer<anyhow::Result<(L, ConfigMap, ContextBuilder)>>
    for PluginCreator<T>
where
    T: PluginFactory,
    L: Layer<T::Plugin>,
{
    type Service = anyhow::Result<(L::Service, ConfigMap, ContextBuilder)>;

    fn layer(
        &self,
        next: anyhow::Result<(L, ConfigMap, ContextBuilder)>,
    ) -> Self::Service {
        if let Err(err) = next {
            return Err(err);
        }
        let (layer, mut config, cb) = next.unwrap();
        let (plugin, cb) = self.create_plugin(&mut config, cb)?;
        let filter = layer.layer(plugin.clone());
        Ok((filter, config, cb.register(plugin)))
    }
}

pub struct FilterStart;

impl<S> Layer<S> for FilterStart {
    type Service = FilterLayer<S>;

    fn layer(&self, next: S) -> Self::Service {
        FilterLayer::new(next)
    }
}

type ChainContext<T> = anyhow::Result<(T, ConfigMap, ContextBuilder)>;

#[derive(Clone, Copy)]
pub struct FilterEnd;

impl Layer<ChainContext<FilterStart>> for FilterEnd {
    type Service = ChainContext<FilterEnd>;

    fn layer(&self, next: ChainContext<FilterStart>) -> Self::Service {
        let (_, config, cb) = next?;
        Ok((FilterEnd, config, cb))
    }
}

impl<F> Layer<ChainContext<FilterLayer<F>>> for FilterEnd {
    type Service = ChainContext<F>;

    fn layer(&self, next: ChainContext<FilterLayer<F>>) -> Self::Service {
        let (filter_layer, config, cb) = next?;
        Ok((filter_layer.plugin, config, cb))
    }
}

impl<Request: Send + 'static> AsyncService<Request> for FilterEnd {
    type Response = Option<Request>;
    type Error = anyhow::Error;

    async fn serve(&self, req: Request) -> anyhow::Result<Self::Response> {
        Ok(Some(req))
    }
}

#[derive(Clone, Copy)]
pub struct FilterLayer<T> {
    plugin: T,
}

impl<T> FilterLayer<T> {
    pub fn new(plugin: T) -> Self {
        Self { plugin }
    }
}

impl<T: Clone, S> Layer<S> for FilterLayer<T> {
    type Service = FilterLayer<FilterChain<S, T>>;

    fn layer(&self, prev: S) -> Self::Service {
        FilterLayer::new(FilterChain::new(prev, self.plugin.clone()))
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
        if let Some(opt) = config.get::<T>() {
            let plugin = opt.build().context("failed to build plugin")?;
            let cb = cb.register(plugin.clone());
            return Ok((plugin, cb));
        }
        panic!("Plugin {} not found", std::any::type_name::<T>());
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

#[cfg(test)]
mod tests {
    use super::*;
    use kaze_plugin::Plugin;

    #[test]
    fn test_options_builder_creation() {
        let builder = Options::builder();
        assert!(builder.config_builder.command().get_name() == "kaze-sidecar");
    }

    #[test]
    fn test_options_deserialization() {
        let toml = r#"threads=4"#;
        let options: Options = toml::from_str(toml).unwrap();
        assert_eq!(options.threads, Some(4));
        assert!(options.host_cmd.is_empty());
    }

    #[test]
    fn test_version_string() {
        let version = VERSION.to_string();
        assert!(!version.is_empty());
    }

    #[derive(Clone)]
    struct MockPlugin;

    impl Plugin for MockPlugin {}

    struct MockFactory;

    impl PluginFactory for MockFactory {
        type Plugin = MockPlugin;

        fn build(&self) -> anyhow::Result<Self::Plugin> {
            Ok(MockPlugin)
        }
    }

    #[test]
    fn test_plugin_creator() {
        let creator = PluginCreator::<MockFactory>::new();
        let mut config = ConfigMap::mock();
        config.insert(MockFactory);
        let cb = Context::builder();

        let (plugin, cb) = creator.create_plugin(&mut config, cb).unwrap();
        let config = cb.build(ConfigMap::mock());
        assert!(plugin.name().ends_with("::MockPlugin"));
        assert!(config.get::<MockPlugin>().is_some());
    }

    #[test]
    fn test_filter_layer() {
        let filter = FilterStart.layer(42);
        assert_eq!(
            std::mem::size_of_val(&filter.plugin),
            std::mem::size_of_val(&42)
        );
    }
}
