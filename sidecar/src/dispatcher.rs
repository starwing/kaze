use std::sync::Arc;

use anyhow::{bail, Context, Result};
use metrics::counter;
use rand::seq::SliceRandom;
use tokio_stream::StreamExt;

use crate::{
    corral::Corral,
    edge,
    kaze::{
        self,
        hdr::{
            DstMask,
            RouteType::{DstBroadcast, DstIdent, DstMulticast, DstRandom},
        },
    },
    resolver::{self, Resolver},
};

/// packet dispatcher
///
/// finds node that matches the route type, and transfer the packet to that node
/// with socket connections.
pub struct Dispatcher {}

impl Dispatcher {
    /// create a new dispatcher
    pub fn new() -> Dispatcher {
        Dispatcher {}
    }

    /// dispatch a packet
    #[tracing::instrument(
        level = "trace",
        skip(self, hdr, data, corral, resolver, sender)
    )]
    pub async fn dispatch(
        &self,
        hdr: &kaze::Hdr,
        data: &kaze_core::Bytes<'_>,
        corral: &Arc<Corral>,
        resolver: &Resolver,
        sender: &edge::Sender,
    ) -> Result<()> {
        if hdr.route_type.is_none() {
            counter!("kaze_dispatch_errors_total", "bodyType" => hdr.body_type.clone())
                .increment(1);
            bail!("no route type for packet seq={}", hdr.seq);
        }
        match hdr.route_type.as_ref().unwrap() {
            DstIdent(ident) => {
                self.dispatch_to(*ident, data, corral, resolver, sender)
                    .await
            }
            DstRandom(mask) => {
                self.dispatch_to_random(*mask, data, corral, resolver, sender)
                    .await
            }
            DstBroadcast(mask) => {
                self.dispatch_to_broadcast(
                    *mask, data, corral, resolver, sender,
                )
                .await
            }
            DstMulticast(multicast) => {
                let idents = multicast.dst_idents.iter().cloned();
                self.dispatch_to_multicast(idents, &data, corral, resolver, sender)
                    .await
            }
        }
        .context("Failed to dispatch packet")
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, ident, data, reg, resolver, sender)
    )]
    async fn dispatch_to(
        &self,
        ident: u32,
        data: &kaze_core::Bytes<'_>,
        reg: &Arc<Corral>,
        resolver: &resolver::Resolver,
        sender: &edge::Sender,
    ) -> Result<()> {
        // 1. find a node
        let conn = reg
            .find_or_create(ident, resolver, sender)
            .await
            .context("Failed to find socket")?;

        // 2. transfer the packet
        conn.dispatch(data)
            .await
            .context("Failed to dispatch packet")
    }

    async fn dispatch_to_multicast(
        &self,
        idents: impl Iterator<Item = u32>,
        data: &kaze_core::Bytes<'_>,
        reg: &Arc<Corral>,
        resolver: &resolver::Resolver,
        sender: &edge::Sender,
    ) -> Result<()> {
        let mut stream = idents
            .map(|ident| self.dispatch_to(ident, &data, reg, resolver, sender))
            .collect::<futures::stream::FuturesUnordered<_>>();
        while let Some(e) = stream.next().await {
            e.context("Failed to dispatch packet in multicast")?;
        }
        Ok(())
    }

    async fn dispatch_to_random(
        &self,
        mask: DstMask,
        data: &kaze_core::Bytes<'_>,
        reg: &Arc<Corral>,
        resolver: &resolver::Resolver,
        sender: &edge::Sender,
    ) -> Result<()> {
        let idents = resolver.get_mask_nodes(mask.ident, mask.mask).await;
        if let Some((ident, _)) = idents.choose(&mut rand::thread_rng()) {
            self.dispatch_to(*ident, data, reg, resolver, sender)
                .await
                .context("Failed to dispatch packet in random")?
        }
        Ok(())
    }

    async fn dispatch_to_broadcast(
        &self,
        mask: DstMask,
        data: &kaze_core::Bytes<'_>,
        reg: &Arc<Corral>,
        resolver: &resolver::Resolver,
        sender: &edge::Sender,
    ) -> Result<()> {
        // 1. find a node
        let nodes = resolver.get_mask_nodes(mask.ident, mask.mask).await;
        let idents = nodes.iter().map(|(ident, _)| *ident);

        // 2. transfer the packet
        self.dispatch_to_multicast(idents, data, reg, resolver, sender)
            .await
            .context("Failed to dispatch packet in multicast")
    }
}
