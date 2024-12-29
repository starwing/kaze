mod codec;
mod config;
mod dispatcher;
mod ratelimit;
mod register;
mod resolver;
mod kaze {
    include!("proto/kaze.rs");
}

use std::net::Ipv4Addr;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use kaze_core::KazeState;
use metrics::counter;
use ratelimit::RateLimit;
use tokio::select;
use tokio::{net::TcpListener, task::block_in_place, try_join};
use tracing::level_filters::LevelFilter;
use tracing::{error, info, instrument, span, trace, warn, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::config::LogFileInfo;
use crate::dispatcher::Dispatcher;
use crate::register::Register;
use crate::resolver::Resolver;

#[tokio::main]
async fn main() -> Result<()> {
    let app = config::parse_args().context("Failed to parse config")?;
    let (non_block, _guard) = LogFileInfo::build_writer(app.log.as_ref())?;

    if let Some(rate_limit) = &app.rate_limit {
        counter!("kaze_rate_limit_total")
            .increment(rate_limit.per_msg.len() as u64);
    }
    let rate_limit = app.rate_limit.as_ref().map(|info| RateLimit::new(info));

    // install tracing with configuration
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(non_block.map(|non_block| fmt::layer().with_writer(non_block)))
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
    if app.prometheus.is_some() || app.prometheus_push.is_some() {
        let mut recorder =
            metrics_exporter_prometheus::PrometheusBuilder::new();
        if let Some(addr) = app.prometheus {
            recorder = recorder.with_http_listener(addr);
        }
        if let Some(push) = app.prometheus_push {
            recorder = recorder.with_push_gateway(
                push.endpoint,
                push.interval,
                push.username,
                push.password,
            )?;
        }
        recorder
            .install()
            .context("Failed to install prometheus recorder")?;
    }

    // go into main loop span
    let span = span!(Level::INFO, "main loop");
    let _enter = span.enter();

    // create shm
    let (sq, cq) = new_kaze_pair(
        &app.shmfile,
        &app.ident,
        app.sq_bufsize,
        app.cq_bufsize,
        app.unlink,
    )
    .context("Failed to create kaze shm queue")?;
    info!("submission queue created name={}", sq.name());
    info!("completion queue created name={}", sq.name());

    let resolver = Resolver::new(app.resolver_cache, app.resolver_livetime);
    for node in app.nodes {
        resolver.add_node(node.ident.to_bits(), node.addr).await;
    }

    if app.host_cmd.len() > 0 {
        let mut cmd = std::process::Command::new(&app.host_cmd[0]);
        cmd.args(&app.host_cmd[1..]);
        cmd.spawn().context("Failed to start host command")?;
        info!("start host command");
    }

    let sq_shutdown = sq.shutdown_guard();
    let reg = Arc::new(Register::new(
        sq,
        rate_limit,
        app.pending_timeout,
        app.idle_timeout,
    ));
    let dispatcher = Dispatcher::new();

    // start listening at last
    let listener = TcpListener::bind(&app.listen)
        .await
        .context("Failed to bind local address")?;
    info!(addr = app.listen, "start listening");

    let exit_notify = Arc::new(tokio::sync::Notify::new());
    {
        let cq_shutdown = cq.shutdown_guard();
        let exit_notify = exit_notify.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to wait for ctrl-c");
            info!("ctrl-c received");
            exit_notify.notify_waiters();
            drop(cq_shutdown);
        });
    }

    try_join!(
        handle_listener(&listener, &exit_notify, &reg, &resolver),
        handle_completion_queue(cq, &dispatcher, &reg, &resolver)
    )
    .context("Failed to in main loop")?;

    info!("begin graceful exiting ...");
    reg.graceful_exit().await?;
    info!("graceful exited");

    // notify host process to exit.
    // must be called before exit, because `sq` has moved into `reg`, and it
    // drops **before** sq_shutdown, makes a crash.
    drop(sq_shutdown);
    info!("submission queue closed");
    unlink_kaze_pair(&app.shmfile, &app.ident)?;
    info!("kaze shared memory files unlinked");
    Ok(())
}

#[instrument(level = "trace", skip(listener, exit_notify, reg, resolver))]
async fn handle_listener(
    listener: &TcpListener,
    exit_notify: &Arc<tokio::sync::Notify>,
    reg: &Arc<Register>,
    resolver: &Resolver,
) -> Result<()> {
    loop {
        let (socket, addr) = select! {
            r = listener.accept() => r?,
            _ = exit_notify.notified() => {
                info!("stop listening");
                return Ok(());
            }
        };
        info!(addr = %addr, "Accepted connection");
        reg.handle_incomming(socket, addr, resolver).await?;
    }
}

#[instrument(level = "trace", skip(cq, dispatcher, reg, resolver))]
async fn handle_completion_queue(
    mut cq: KazeState,
    dispatcher: &Dispatcher,
    reg: &Arc<Register>,
    resolver: &Resolver,
) -> Result<()> {
    loop {
        let ctx = match cq.try_pop() {
            Ok(ctx) => ctx,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                counter!("kaze_pop_blocking_total").increment(1);
                match block_in_place(|| cq.pop()) {
                    Ok(ctx) => ctx,
                    Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                        info!("completion queue closed");
                        return Ok(());
                    }
                    Err(e) => {
                        counter!("kaze_pop_blocking_errors_total")
                            .increment(1);
                        error!(error = %e, "Error reading from blocking kaze");
                        return Err(e.into());
                    }
                }
            }
            Err(e) => {
                counter!("kaze_pop_errors_total").increment(1);
                error!(error = %e, "Error reading from kaze");
                return Err(e.into());
            }
        };

        let mut data = ctx.buffer();
        let hdr = codec::decode_packet(&mut data)?;
        if let Err(e) = dispatcher.dispatch(&hdr, &data, &reg, resolver).await
        {
            counter!("kaze_dispatch_errors_total").increment(1);
            error!("Error dispatching packet: {e}");
            // continue running
        }
        trace!(hdr = ?hdr, len = data.len(), "dispatch packet");
        ctx.commit();
        counter!("kaze_completion_packets_total").increment(1);
    }
}

fn new_kaze_pair(
    prefix: impl AsRef<str>,
    ident: &Ipv4Addr,
    sq_bufsize: usize,
    cq_bufsize: usize,
    unlink: bool,
) -> Result<(KazeState, KazeState)> {
    let (sq_name, cq_name) = get_kaze_pair_names(prefix, ident);
    let ident = ident.to_bits();

    if KazeState::exists(&sq_name).context("Failed to check shm queue")? {
        if !unlink {
            let sq = KazeState::open(&sq_name)
                .context("Failed to open submission queue")?;
            let (sender, receiver) = sq.owner();
            bail!(
                "shm queue {} already exists, previous kaze sender={} receiver={}",
                sq_name,
                sender,
                receiver
            );
        } else {
            if let Err(e) = KazeState::unlink(&sq_name) {
                warn!(error = %e, "Failed to unlink submission queue");
            }
            if let Err(e) = KazeState::unlink(&cq_name) {
                warn!(error = %e, "Failed to unlink completion queue");
            }
        }
    }

    let page_size = page_size::get();
    let sq_bufsize = KazeState::aligned_bufsize(sq_bufsize, page_size);
    let cq_bufsize = KazeState::aligned_bufsize(cq_bufsize, page_size);
    let mut sq = KazeState::new(&sq_name, ident, sq_bufsize)
        .context("Failed to create submission queue")?;
    let mut cq = KazeState::new(&cq_name, ident, cq_bufsize)
        .context("Failed to create completion queue")?;
    sq.set_owner(Some(sq.pid()), None);
    cq.set_owner(None, Some(cq.pid()));
    Ok((sq, cq))
}

fn unlink_kaze_pair(prefix: impl AsRef<str>, ident: &Ipv4Addr) -> Result<()> {
    let (sq_name, cq_name) = get_kaze_pair_names(prefix, ident);
    KazeState::unlink(&sq_name)
        .context("Failed to unlink submission queue")?;
    KazeState::unlink(&cq_name)
        .context("Failed to unlink completion queue")?;
    Ok(())
}

fn get_kaze_pair_names(
    prefix: impl AsRef<str>,
    ident: &Ipv4Addr,
) -> (String, String) {
    let sq_name = format!("{}_sq_{}", prefix.as_ref(), ident);
    let cq_name = format!("{}_cq_{}", prefix.as_ref(), ident);
    (sq_name, cq_name)
}
