// mod args;
mod codec;
mod config;
mod connection;
mod corral;
mod dispatcher;
mod edge;
mod ratelimit;
mod resolver;
mod kaze {
    include!("proto/kaze.rs");
}

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use config::parse_args;
use edge::{Edge, Receiver};
use metrics::counter;
use ratelimit::RateLimit;
use tokio::select;
use tokio::{net::TcpListener, try_join};
use tracing::level_filters::LevelFilter;
use tracing::{error, info, span, trace, Level};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

use crate::config::LogFileConfig;
use crate::corral::Corral;
use crate::dispatcher::Dispatcher;
use crate::resolver::Resolver;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[tokio::main]
async fn main() -> Result<()> {
    let app = parse_args().context("Failed to parse config")?;

    let (non_block, _guard) =
        LogFileConfig::build_writer(&app, app.log.as_ref())?;

    if let Some(rate_limit) = &app.rate_limit {
        counter!("kaze_rate_limit_total")
            .increment(rate_limit.per_msg.len() as u64);
    }
    let rate_limit = app.rate_limit.as_ref().map(|info| RateLimit::new(info));

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
    let edge = edge::Options::new()
        .with_sq_bufsize(app.kaze.sq_bufsize)
        .with_cq_bufsize(app.kaze.cq_bufsize)
        .with_unlink(app.unlink)
        .build(&app.kaze.name, app.kaze.ident)
        .context("Failed to create kaze shm queue")?;
    info!("submission queue created name={}", edge.sq_name());
    info!("completion queue created name={}", edge.cq_name());

    let mut resolver =
        Resolver::new(app.resolver.cache_size, app.resolver.live_time);
    resolver.setup_local(app.nodes.iter()).await;
    if let Some(conf) = &app.consul {
        resolver
            .setup_consul(conf.addr.clone(), conf.consul_token.clone())
            .await
            .context("Failed to setup consul client")?;
    }

    if app.host_cmd.len() > 0 {
        let mut cmd = std::process::Command::new(&app.host_cmd[0]);
        cmd.args(&app.host_cmd[1..]);
        cmd.spawn().context("Failed to start host command")?;
        info!("start host command");
    }

    let (receiver, sender) = edge.split();

    let corral = Arc::new(Corral::new(
        rate_limit,
        app.register.pending_timeout,
        app.register.idle_timeout,
    ));
    let sqlock = sender.lock().await;
    let dispatcher = Dispatcher::new();

    // start listening at last
    let listener = TcpListener::bind(&app.listen)
        .await
        .context("Failed to bind local address")?;
    info!(addr = app.listen, "start listening");

    let exit_notify = Arc::new(tokio::sync::Notify::new());
    {
        let cqlock = receiver.lock();
        let exit_notify = exit_notify.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to wait for ctrl-c");
            info!("ctrl-c received");
            exit_notify.notify_waiters();
            drop(cqlock);
        });
    }

    try_join!(
        handle_listener(&listener, &sender, &exit_notify, &corral, &resolver),
        handle_completion_queue(
            receiver,
            &sender,
            &dispatcher,
            &corral,
            &resolver,
        )
    )
    .context("Failed to in main loop")?;

    // shutdown submission queue, do not allow new requests
    drop(sqlock);
    info!("submission queue closed");

    info!("graceful exiting start ...");
    corral.graceful_exit().await?;
    info!("register graceful exited");

    Edge::unlink(&app.kaze.name, app.kaze.ident)?;
    info!("graceful exiting completed");
    Ok(())
}

#[tracing::instrument(
    level = "trace",
    skip(listener, sender, exit_notify, corral, resolver)
)]
async fn handle_listener(
    listener: &TcpListener,
    sender: &edge::Sender,
    exit_notify: &Arc<tokio::sync::Notify>,
    corral: &Arc<Corral>,
    resolver: &Resolver,
) -> Result<()> {
    loop {
        let (socket, addr) = select! {
            _ = exit_notify.notified() => {
                info!("stop listening");
                return Ok(());
            },
            r = listener.accept() => r?,
        };
        info!(addr = %addr, "Accepted connection");
        corral
            .handle_incomming(socket, addr, resolver, sender)
            .await?;
    }
}

#[tracing::instrument(
    level = "trace",
    skip(receiver, sender, dispatcher, reg, resolver)
)]
async fn handle_completion_queue(
    mut receiver: Receiver,
    sender: &edge::Sender,
    dispatcher: &Dispatcher,
    reg: &Arc<Corral>,
    resolver: &Resolver,
) -> Result<()> {
    while let Some(ctx) = receiver.recv().await? {
        let mut data = ctx.buffer();
        let hdr = codec::decode_packet(&mut data)?;
        if let Err(e) = dispatcher
            .dispatch(&hdr, &data, &reg, resolver, sender)
            .await
        {
            counter!("kaze_dispatch_errors_total").increment(1);
            error!("Error dispatching packet: {e}");
            // continue running
        }
        trace!(hdr = ?hdr, len = data.len(), "dispatch packet");
        ctx.commit();
        counter!("kaze_completion_packets_total").increment(1);
    }
    Ok(())
}
