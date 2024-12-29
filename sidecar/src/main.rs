mod codec;
mod config;
mod dispatcher;
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
use tokio::{net::TcpListener, task::block_in_place, try_join};
use tracing::{error, info, span, trace, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::config::LogFileInfo;
use crate::dispatcher::Dispatcher;
use crate::register::Register;
use crate::resolver::Resolver;

#[tokio::main]
async fn main() -> Result<()> {
    let app = config::parse_args().context("Failed to parse config")?;
    let (non_block, _guard) = LogFileInfo::build_writer(app.log.as_ref())?;

    // install tracing with configuration
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .with(non_block.map(|non_block| fmt::layer().with_writer(non_block)))
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
        app.shmfile,
        &app.ident,
        app.sq_bufsize,
        app.cq_bufsize,
        app.unlink,
    )
    .context("Failed to create kaze shm queue")?;
    info!("create kaze shm");

    let resolver = Resolver::new(app.resolver_cache, app.resolver_time);
    for node in app.nodes {
        resolver.add_node(node.ident.to_bits(), node.addr).await;
    }

    if app.host_cmd.len() > 0 {
        let mut cmd = std::process::Command::new(&app.host_cmd[0]);
        cmd.args(&app.host_cmd[1..]);
        cmd.spawn().context("Failed to start host command")?;
        info!("start host command");
    }

    let reg =
        Arc::new(Register::new(sq, app.pending_timeout, app.idle_timeout));
    let dispatcher = Dispatcher::new();

    // start listening at last
    let listener = TcpListener::bind(&app.listen)
        .await
        .context("Failed to bind local address")?;
    info!(addr = app.listen, "start listening");

    try_join!(
        handle_listener(listener, &reg, &resolver),
        handle_completion_queue(cq, &reg, &resolver, &dispatcher)
    )
    .context("Failed to in main loop")?;
    Ok(())
}

async fn handle_listener(
    listener: TcpListener,
    reg: &Arc<Register>,
    resolver: &Resolver,
) -> Result<()> {
    loop {
        let (socket, addr) = listener.accept().await?;
        info!(addr = %addr, "Accepted connection");
        reg.handle_incomming(resolver, socket, addr).await?;
    }
}

async fn handle_completion_queue(
    mut cq: KazeState,
    reg: &Arc<Register>,
    resolver: &Resolver,
    dispatcher: &Dispatcher,
) -> Result<()> {
    loop {
        let ctx = match cq.try_pop() {
            Ok(ctx) => ctx,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                counter!("kaze_pop_blocking_total").increment(1);
                block_in_place(|| cq.pop()).map_err(|e| {
                    counter!("kaze_pop_blocking_errors_total").increment(1);
                    error!(error = %e, "Error reading from blocking kaze");
                    e
                })?
            }
            Err(e) => {
                counter!("kaze_pop_errors_total").increment(1);
                error!(error = %e, "Error reading from kaze");
                return Err(e.into());
            }
        };

        let mut data = ctx.buffer();
        let hdr = codec::decode_packet(&mut data)?;
        if let Err(e) = dispatcher.dispatch(&reg, resolver, &hdr, &data).await
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
    let sq_name = format!("{}_sq_{}", prefix.as_ref(), ident);
    let cq_name = format!("{}_sq_{}", prefix.as_ref(), ident);
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
            KazeState::unlink(&sq_name)
                .context("Failed to unlink submission queue")?;
            KazeState::unlink(&cq_name)
                .context("Failed to unlink completion queue")?;
        }
    }

    let mut sq = KazeState::new(&sq_name, ident, sq_bufsize)
        .context("Failed to create submission queue")?;
    let mut cq = KazeState::new(&cq_name, ident, cq_bufsize)
        .context("Failed to create completion queue")?;
    sq.set_owner(Some(sq.pid()), None);
    cq.set_owner(None, Some(cq.pid()));
    Ok((sq, cq))
}
