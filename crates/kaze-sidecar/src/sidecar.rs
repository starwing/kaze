use anyhow::Context as _;
use kaze_plugin::clap::{
    crate_version, CommandFactory, FromArgMatches, Parser,
};
use kaze_plugin::protocol::packet::{new_bytes_pool, BytesPool};
use kaze_plugin::protocol::service::{SinkMessage, ToMessageService};
use kaze_plugin::serde::{Deserialize, Serialize};
use kaze_plugin::util::tower_ext::{ChainLayer, ServiceExt};
use kaze_plugin::PipelineService;
use kaze_resolver::dispatch_service;
use scopeguard::defer;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use tokio::join;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tower::util::BoxCloneSyncService;
use tower::ServiceBuilder;

use crate::config::{ConfigBuilder, ConfigFileBuilder};
use crate::plugins::corral::Corral;
use crate::plugins::tracker::RpcTracker;
use crate::plugins::{consul, corral, log, prometheus, ratelimit};

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

    /// listen address for the mesh endpoint
    #[serde(default = "default_listen")]
    #[arg(short, long, default_value_t = default_listen())]
    #[arg(value_name = "ADDR")]
    pub listen: String,

    /// Count of worker threads (0 means autodetect)
    #[arg(short = 'j', long)]
    #[arg(value_name = "N")]
    pub threads: Option<usize>,
}

impl Options {
    pub fn build() -> anyhow::Result<Sidecar> {
        let cmd = Options::command();
        let builder = ConfigBuilder::new(cmd)
            .add::<log::Options>("log")
            .add::<kaze_edge::Options>("edge")
            .add::<corral::Options>("corral")
            .add::<ratelimit::Options>("rate_limit")
            .add::<kaze_resolver::LocalOptions>("local")
            .add::<consul::Options>("consul")
            .add::<prometheus::Options>("prometheus")
            .debug_assert();

        let merger = builder.get_matches();
        let options = Options::from_arg_matches(merger.arg_matches())?;
        let mut filefinder = ConfigFileBuilder::default();
        if let Some(path) = &options.config {
            filefinder = filefinder.add_file(path.clone());
        }

        let content = filefinder.build()?;
        let mut config = merger.build(content)?;

        let pool = new_bytes_pool();

        let edge: Box<kaze_edge::Options> = config.take().unwrap();
        let (prefix, ident) = (edge.name.clone(), edge.ident);
        defer! {
            kaze_edge::Edge::unlink(prefix, ident).unwrap();
        }
        let edge = edge.build().unwrap();
        let (tx, rx) = edge.into_split();

        let resolver = Arc::new(futures::executor::block_on(async {
            config
                .take::<kaze_resolver::LocalOptions>()
                .unwrap()
                .build()
                .await
        }));
        let ratelimit = config.take::<ratelimit::Options>().unwrap().build();
        let corral = config
            .take::<corral::Options>()
            .unwrap()
            .build(pool.clone());
        let tracker = RpcTracker::new(10, Notify::new());

        let sink = ServiceBuilder::new()
            .layer(ToMessageService::new())
            .layer(ChainLayer::new(ratelimit.service()))
            .layer(ChainLayer::new(dispatch_service(resolver)))
            .layer(ChainLayer::new(tracker.clone().service()))
            .layer(corral.clone().layer())
            .layer(tx.clone().layer(pool.clone()))
            .service(SinkMessage::new());
        let sink: PipelineService = BoxCloneSyncService::new(sink);
        Ok(Sidecar::new(pool, rx, corral, sink, options))
    }
}

pub struct Sidecar {
    pool: BytesPool,
    rx: Option<kaze_edge::Receiver>,
    corral: Arc<Corral>,
    sink: PipelineService,
    options: Options,
}

impl Sidecar {
    /// create a new sidecar
    pub fn new(
        pool: BytesPool,
        rx: kaze_edge::Receiver,
        corral: Arc<Corral>,
        sink: PipelineService,
        options: Options,
    ) -> Self {
        Self {
            pool,
            rx: Some(rx),
            corral,
            sink,
            options,
        }
    }

    /// get the thread count
    pub fn thread_count(&self) -> Option<usize> {
        self.options.threads
    }

    /// run the sidecar
    pub async fn run(mut self) -> anyhow::Result<()> {
        let rx = self.rx.take().unwrap();
        let (r1, r2) = join!(self.handle_receiver(rx), self.handle_listener());
        r1?;
        r2?;
        Ok(())
    }

    async fn handle_receiver(
        &self,
        mut rx: kaze_edge::Receiver,
    ) -> anyhow::Result<()> {
        let mut sink = self.sink.clone();
        loop {
            let packet = rx
                .read_packet(&self.pool)
                .await
                .context("failed to read packet")?;
            sink.ready_call((packet, None)).await?;
        }
    }

    async fn handle_listener(&self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.options.listen).await?;
        while let Ok((conn, addr)) = listener.accept().await {
            self.corral.add_connection(conn, addr).await?;
        }
        Ok(())
    }
}

fn default_listen() -> String {
    "0.0.0.0:6081".to_string()
}

static VERSION: LazyLock<String> = LazyLock::new(|| {
    let git_version = bugreport::git_version!(fallback = "");

    if git_version.is_empty() {
        crate_version!().to_string()
    } else {
        format!("{} ({})", crate_version!(), git_version)
    }
});
