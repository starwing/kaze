mod config;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use config::parse_args;
use metrics::counter;
use tokio::{net::TcpListener, try_join};
use tracing::level_filters::LevelFilter;
use tracing::{info, span, Level};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

use kaze_corral::corral::Corral;
use kaze_edge::Edge;
use kaze_util::ratelimit::RateLimit;

use crate::config::LogFileOptions;

fn main() -> Result<()> {
    let app = parse_args().context("Failed to parse config")?;

    let (non_block, _guard) =
        LogFileOptions::build_writer(&app, app.log.as_ref())?;

    // install tracing with configuration
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(non_block.map(|non_block| {
            fmt::layer().with_ansi(false).with_writer(non_block)
        }))
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .with_env_var("KAZE_LOG")
                .from_env_lossy(),
        )
        .init();

    // our first log
    info!(config = ?app);

    // install prometheus metrics
    if let Some(conf) = &app.prometheus {
        let mut recorder =
            metrics_exporter_prometheus::PrometheusBuilder::new();
        if let Some(addr) = conf.listen {
            recorder = recorder.with_http_listener(addr);
        }
        if let Some(endpoint) = &conf.endpoint {
            let default_interval = Duration::from_secs(10);
            recorder = recorder.with_push_gateway(
                endpoint,
                conf.interval.unwrap_or(default_interval.into()).into(),
                conf.username.clone(),
                conf.password.clone(),
            )?;
        }
        recorder
            .install()
            .context("Failed to install prometheus recorder")?;
    }

    // go into main loop span
    let span = span!(Level::INFO, "main loop");
    let _enter = span.enter();

    // create edge
    let (name, ident) = (app.edge.name.clone(), app.edge.ident);

    // 创建运行时，并设置线程数量为 4
    let mut runtime = tokio::runtime::Builder::new_multi_thread();
    if let Some(threads) = app.threads {
        if threads > 0 {
            runtime.worker_threads(threads);
        }
    }

    runtime
        .enable_all() // 启用所有功能（I/O、计时器等）
        .build()?
        .block_on(async_main(app))?;

    Edge::unlink(&name, ident)?;
    info!("kaze shared memory files unlinked");
    Ok(())
}

async fn async_main(app: config::Config) -> Result<()> {
    if let Some(rate_limit) = &app.rate_limit {
        counter!("kaze_rate_limit_total")
            .increment(rate_limit.per_msg.len() as u64);
    }
    let rate_limit = app.rate_limit.as_ref().map(|info| RateLimit::new(info));

    let resolver = app.local.build().await;
    // if let Some(conf) = &app.consul {
    //     resolver
    //         .setup_consul(conf.addr.clone(), conf.consul_token.clone())
    //         .await
    //         .context("Failed to setup consul client")?;
    // }

    if app.host_cmd.len() > 0 {
        let mut cmd = std::process::Command::new(&app.host_cmd[0]);
        cmd.args(&app.host_cmd[1..]);
        cmd.spawn().context("Failed to start host command")?;
        info!("start host command");
    }

    let ident = app.edge.ident.to_bits();
    let edge = app
        .edge
        .build()
        .context("Failed to create kaze shm queue")?;
    info!("submission queue created name={}", edge.sq_name());
    info!("completion queue created name={}", edge.cq_name());
    let (receiver, sender) = edge.split();

    // let corral = Arc::new(
    //     kaze_corral::Builder::from_options(app.corral, resolver, sender)
    //         .with_rate_limit(rate_limit)
    //         .build(ident),
    // );

    // start listening at last
    let listener = TcpListener::bind(&app.listen)
        .await
        .context("Failed to bind local address")?;
    info!(addr = app.listen, "start listening");

    // {
    //     let cqlock = receiver.lock();
    //     let corral = corral.clone();
    //     tokio::spawn(async move {
    //         tokio::signal::ctrl_c()
    //             .await
    //             .expect("Failed to wait for ctrl-c");
    //         info!("ctrl-c received");
    //         corral.notify_exit();
    //         drop(cqlock);
    //     });
    // }

    // try_join!(
    //     corral.handle_listener(&listener),
    //     corral.handle_completion(receiver),
    //     corral.handle_rpc(),
    // )
    // .context("Failed to in main loop")?;

    // info!("corral graceful exiting start ...");
    // corral.graceful_exit().await?;
    // info!("corral graceful exited");

    Ok(())
}
