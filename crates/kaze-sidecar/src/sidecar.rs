use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::Context as _;
use kaze_plugin::clap::{
    crate_version, CommandFactory, FromArgMatches, Parser,
};
use tokio::join;
use tokio_util::task::TaskTracker;
use tower::util::BoxCloneSyncService;
use tower::ServiceBuilder;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

use kaze_plugin::protocol::service::{SinkMessage, ToMessageService};
use kaze_plugin::serde::{Deserialize, Serialize};
use kaze_plugin::service::ServiceExt;
use kaze_plugin::tokio_graceful::Shutdown;
use kaze_plugin::{Context, PipelineService, Plugin, PluginFactory};
use kaze_resolver::ResolverExt;

use crate::config::{ConfigBuilder, ConfigFileBuilder, ConfigMap};
use crate::plugins::{consul, corral, log, prometheus, ratelimit, tracker};

/// the kaze sidecar for host
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
    pub fn build() -> anyhow::Result<Sidecar> {
        let mut config =
            Self::new_config_map().context("failed to load config")?;

        let expander = |prefix: &str| -> String {
            let edge = config.get::<kaze_edge::Options>().unwrap();
            prefix
                .replace("{name}", edge.name.as_str())
                .replace("{ident}", &edge.ident.to_string())
                .replace("{version}", VERSION.as_str())
        };
        let log = config.get::<log::Options>();
        let (non_block, _guard) = log
            .map(|log| log::Options::build_writer(log, expander))
            .transpose()
            .context("failed to build log")?
            .unzip();

        // install tracing with configuration
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(non_block.map(|non_block| {
                fmt::layer().with_ansi(false).with_writer(non_block)
            }))
            .with(env_filter())
            .init();

        let edge = config
            .take::<kaze_edge::Options>()
            .unwrap()
            .build()
            .unwrap();
        let _unlink_guard = edge.unlink_guard();
        let (tx, rx) = edge.into_split();

        let resolver = futures::executor::block_on(async {
            config
                .take::<kaze_resolver::LocalOptions>()
                .unwrap()
                .build()
                .await
        });
        let ratelimit =
            config.take::<ratelimit::Options>().unwrap().build()?;
        let corral = config.take::<corral::Options>().unwrap().build()?;
        let tracker = tracker::RpcTracker::new(10);
        let logger = log::LogService;

        let sink = ServiceBuilder::new()
            .layer(ToMessageService.into_layer())
            .layer(logger.into_filter())
            .layer(ratelimit.clone().into_filter())
            .layer(resolver.clone().into_service().into_filter())
            .layer(tracker.clone().into_filter())
            .layer(corral.clone().into_filter())
            .layer(tx.clone().into_filter())
            .service(SinkMessage.map_response(|_| Some(())))
            .map_response(|_| ());
        let sink: PipelineService =
            BoxCloneSyncService::new(sink.into_tower());
        let shutdown = Shutdown::default();

        let ctx = Context::builder()
            .register(corral.clone())
            .register(ratelimit)
            .register(resolver)
            .register(tracker.clone())
            .register(tx)
            .register(rx.clone())
            .build(shutdown.guard());
        ctx.sink().set(sink);

        let options = config.take::<Options>().unwrap();
        Ok(Sidecar {
            ctx,
            options,
            _shutdown: shutdown,
            _tracker: TaskTracker::new(),
            _unlink_guard,
            _log_guard: _guard,
        })
    }

    fn new_config_map() -> anyhow::Result<ConfigMap> {
        let cmd = Options::command();
        let merger = Self::new_config_builder(cmd).get_matches();
        let options = Options::from_arg_matches(merger.arg_matches())
            .context("failed to parse options")?;

        let mut filefinder = ConfigFileBuilder::default();
        if let Some(path) = &options.config {
            filefinder = filefinder.add_file(path.clone());
        }

        let content = filefinder.build().context("build file finder error")?;
        let mut map = merger.build(content).context("merger build error")?;
        map.insert(options);
        Ok(map)
    }

    fn new_config_builder(cmd: clap::Command) -> ConfigBuilder {
        ConfigBuilder::new(cmd)
            .add::<log::Options>("log")
            .add::<kaze_edge::Options>("edge")
            .add::<corral::Options>("corral")
            .add::<ratelimit::Options>("rate_limit")
            .add::<kaze_resolver::LocalOptions>("local")
            .add::<consul::Options>("consul")
            .add::<prometheus::Options>("prometheus")
            .debug_assert()
    }
}

pub struct Sidecar {
    ctx: Context,
    options: Options,
    _shutdown: Shutdown,
    _tracker: TaskTracker,
    _unlink_guard: kaze_edge::UnlinkGuard,
    _log_guard: Option<WorkerGuard>,
}

impl Sidecar {
    pub fn new(
        ctx: Context,
        options: Options,
        _shutdown: Shutdown,
        _unlink_guard: kaze_edge::UnlinkGuard,
        _log_guard: Option<WorkerGuard>,
    ) -> Self {
        Self {
            ctx,
            options,
            _shutdown,
            _tracker: TaskTracker::new(),
            _unlink_guard,
            _log_guard,
        }
    }
    /// get the thread count
    pub fn thread_count(&self) -> Option<usize> {
        self.options.threads
    }

    /// run the sidecar
    pub async fn run(self) -> anyhow::Result<()> {
        let (r1, r2) = join!(
            self.ctx
                .get::<kaze_edge::Receiver>()
                .unwrap()
                .run()
                .unwrap(),
            self.ctx.get::<corral::Corral>().unwrap().run().unwrap(),
        );
        r1?;
        r2?;
        Ok(())
    }
}

fn env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .with_env_var("KAZE_LOG")
        .from_env_lossy()
}

pub(crate) static VERSION: LazyLock<String> = LazyLock::new(|| {
    let git_version = bugreport::git_version!(fallback = "");

    if git_version.is_empty() {
        crate_version!().to_string()
    } else {
        format!("{} ({})", crate_version!(), git_version)
    }
});
