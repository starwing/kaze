use std::{ffi::OsString, path::PathBuf, sync::LazyLock};

use anyhow::Context as _;
use clap::{crate_version, Parser};
use clap::{CommandFactory as _, FromArgMatches as _};
use kaze_plugin::service::FilterChain;
use kaze_plugin::{Context, ContextBuilder};
use tower::layer::util::{Identity, Stack};

use kaze_plugin::{
    serde::{Deserialize, Serialize},
    PluginFactory,
};
use tower::Layer;

use crate::builder::SidecarBuilder;
use crate::config::ConfigFileBuilder;
use crate::{
    config::{ConfigBuilder, ConfigMap},
    plugins::{corral, tracker},
};

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

impl Options {
    pub fn builder() -> OptionsBuilder<Identity> {
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

impl OptionsBuilder<Identity> {
    fn new(cfg: ConfigBuilder) -> Self {
        let layer = Identity::new();
        Self {
            config_builder: cfg,
            layer,
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

    pub fn debug_assert(self) -> Self {
        self.config_builder.command().clone().debug_assert();
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
            Service = anyhow::Result<(
                FilterLayer<F>,
                ConfigMap,
                ContextBuilder,
            )>,
        >,
    {
        let cmd = self.config_builder.command();
        let mut matches = cmd.clone().get_matches_from(itr);
        let options = Options::from_arg_matches(&matches)
            .context("failed to parse options")?;

        let mut filefinder = ConfigFileBuilder::default();
        if let Some(path) = &options.config {
            filefinder = filefinder.add_file(path.clone());
        }

        let content = filefinder.build().context("build file finder error")?;
        let mut config = self
            .config_builder
            .build(&mut matches, content)
            .context("merger build error")?;
        config.insert(options);

        let (layer, config, cb) =
            self.layer
                .layer(Ok((FilterStart, config, Context::builder())))?;
        Ok(SidecarBuilder::new(layer.plugin, config, cb))
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
