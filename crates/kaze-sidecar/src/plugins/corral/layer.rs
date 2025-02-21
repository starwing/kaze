use std::{io::IoSlice, sync::Arc};

use anyhow::Context as _;
use futures::future::join_all;
use tokio::io::AsyncWriteExt;
use tower::{Layer, layer::layer_fn, service_fn};
use tracing::error;

use kaze_plugin::protocol::{
    message::{Destination, Message, Node, PacketWithAddr},
    service::MessageService,
};
use kaze_plugin::util::tower_ext::ServiceExt as _;

use super::Corral;

impl Corral {
    pub fn layer<S>(
        self: Arc<Corral>,
    ) -> impl Layer<S, Service: MessageService<()>>
    where
        S: MessageService<()>,
    {
        layer_fn(move |inner: S| {
            let corral = self.clone();
            service_fn(move |item: Message| {
                let corral = corral.clone();
                let inner = inner.clone();
                async move {
                    if !item.destination().is_remote() {
                        return inner
                            .clone()
                            .ready_call(item)
                            .await
                            .context("failed to forward packet");
                    }
                    let (item, dst) = item.split();
                    match dst {
                        Destination::Node(node) => {
                            corral_send(corral, item, node).await
                        }
                        Destination::NodeList(nodes) => {
                            corral_broadcast(corral, item, nodes).await
                        }
                        _ => unreachable!(),
                    }
                }
            })
        })
    }
}

async fn corral_send(
    corral: Arc<Corral>,
    item: PacketWithAddr,
    dst: Node,
) -> Result<(), anyhow::Error> {
    let (packet, _) = item;
    corral_send_raw(
        corral.clone(),
        &packet.as_iovec(corral.bytes_pool()).to_iovec(),
        dst,
    )
    .await
    .context("Failed to send message")
}

async fn corral_broadcast(
    corral: Arc<Corral>,
    item: PacketWithAddr,
    dst_list: Vec<Node>,
) -> Result<(), anyhow::Error> {
    let (packet, _) = item;
    let mut iovec = packet.as_iovec(corral.bytes_pool());
    let iovec = iovec.to_iovec();
    let task = join_all(
        dst_list
            .into_iter()
            .map(|node| corral_send_raw(corral.clone(), &iovec, node)),
    )
    .await;
    let mut errors = Vec::new();
    for res in task.into_iter() {
        if let Err(e) = res {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        return Ok(());
    }
    Err(BroadcastError::new(errors)).context("Failed to send message")
}

async fn corral_send_raw(
    corral: Arc<Corral>,
    iovec: &[IoSlice<'_>],
    dst: Node,
) -> Result<(), anyhow::Error> {
    match corral.find_or_connect(dst.addr).await? {
        Some(conn) => {
            conn.lock().await.write_vectored(iovec).await?;
        }
        _ => {
            error!(addr = ?dst, "Failed to find connection");
        }
    }
    Ok(())
}

pub struct BroadcastError {
    errors: Vec<anyhow::Error>,
}

impl BroadcastError {
    pub fn new(errors: Vec<anyhow::Error>) -> Self {
        Self { errors }
    }
}

impl std::fmt::Debug for BroadcastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BroadcastError")
            .field("errors", &self.errors)
            .finish()
    }
}

impl std::fmt::Display for BroadcastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "errors in broadcast: ")?;
        for (idx, err) in self.errors.iter().enumerate() {
            write!(f, "[{idx}: {err}]")?;
        }
        Ok(())
    }
}

impl std::error::Error for BroadcastError {}
