mod codec;
mod config;
mod dispatcher;
mod register;
mod resolver;
mod kaze {
    include!("proto/kaze.rs");
}

use std::io::{ErrorKind, Result};
use std::sync::Arc;

use dispatcher::Dispatcher;
use kaze_core::KazeState;
use log::{error, info};
use metrics::counter;
use register::Register;
use resolver::Resolver;
use tokio::net::TcpListener;
use tokio::task::block_in_place;
use tokio::try_join;

#[tokio::main]
async fn main() -> Result<()> {
    let app = config::parse_args()?;

    let listener = TcpListener::bind(&app.listen).await?;
    info!("Listening on {}", app.listen);

    let (sq, cq) = new_kaze_pair(
        app.shmfile,
        app.ident.to_bits(),
        app.sq_bufsize,
        app.cq_bufsize,
    )?;
    info!(
        "create kaze shm ident={} (sq={}, cq={})",
        sq.ident(),
        app.sq_bufsize,
        app.cq_bufsize
    );
    let resolver = Resolver::new(app.resolver_cache, app.resolver_time);
    for node in app.nodes {
        resolver.add_node(node.ident.to_bits(), node.addr).await;
    }

    if app.host_cmd.len() > 0 {
        let mut cmd = std::process::Command::new(&app.host_cmd[0]);
        cmd.args(&app.host_cmd[1..]);
        cmd.spawn()?;
    }

    let reg = Arc::new(Register::new(sq));
    let dispatcher = Dispatcher::new();
    try_join!(
        handle_listener(listener, &reg, &resolver),
        handle_completion_queue(cq, &reg, &resolver, &dispatcher)
    )?;
    Ok(())
}

async fn handle_listener(
    listener: TcpListener,
    reg: &Arc<Register>,
    resolver: &Resolver,
) -> Result<()> {
    loop {
        let (socket, addr) = listener.accept().await?;
        info!("Accepted connection from {}", addr);
        counter!("kaze-connections").increment(1);
        reg.incomming(resolver, socket, addr).await?;
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
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                counter!("kaze-write-blocking").increment(1);
                block_in_place(|| cq.pop()).map_err(|e| {
                    counter!("kaze-read-blocking-errors").increment(1);
                    error!("Error reading from blocking kaze: {e}");
                    e
                })?
            }
            Err(e) => {
                counter!("kaze-read-errors").increment(1);
                error!("Error reading from kaze: {e}");
                return Err(e);
            }
        };

        let mut data = ctx.buffer();
        let hdr = codec::decode_packet(&mut data)?;
        counter!("kaze-packets").increment(1);
        if let Err(e) = dispatcher.dispatch(&reg, resolver, hdr, &data).await {
            counter!("kaze-dispatch-errors").increment(1);
            error!("Error dispatching packet: {e}");
            // continue running
        }
        ctx.commit()
    }
}

fn new_kaze_pair(
    shm_name: impl AsRef<std::path::Path>,
    ident: u32,
    sq_bufsize: usize,
    cq_bufsize: usize,
) -> Result<(KazeState, KazeState)> {
    let sq = KazeState::new(shm_name.as_ref(), ident, sq_bufsize)?;
    let cq = KazeState::new(shm_name.as_ref(), ident, cq_bufsize)?;
    Ok((sq, cq))
}
