use std::sync::Arc;

use anyhow::{bail, Context, Result};
use futures::stream::FuturesUnordered;
use kaze_resolver::Resolver;
use metrics::counter;
use rand::Rng;
use tokio_stream::StreamExt;

use kaze_protocol::{
    hdr::{
        DstMask,
        RouteType::{DstBroadcast, DstIdent, DstMulticast, DstRandom},
    },
    Hdr,
};

use crate::corral::Corral;

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
    #[tracing::instrument(level = "trace", skip(self, hdr, data, corral))]
    pub async fn dispatch<R: Resolver>(
        &self,
        hdr: &Hdr,
        data: &kaze_core::Bytes<'_>,
        corral: &Arc<Corral<R>>,
    ) -> Result<()> {
        if hdr.route_type.is_none() {
            counter!("kaze_dispatch_errors_total", "bodyType" => hdr.body_type.clone())
                .increment(1);
            bail!("no route type for packet seq={}", hdr.seq);
        }
        match hdr.route_type.as_ref().unwrap() {
            DstIdent(ident) => self.dispatch_to(*ident, data, corral).await,
            DstRandom(mask) => {
                self.dispatch_to_random(*mask, data, corral).await
            }
            DstBroadcast(mask) => {
                self.dispatch_to_broadcast(*mask, data, corral).await
            }
            DstMulticast(multicast) => {
                let idents = multicast.dst_idents.iter().cloned();
                self.dispatch_to_multicast(idents, &data, corral).await
            }
        }
        .context("Failed to dispatch packet")
    }

    #[tracing::instrument(level = "trace", skip(self, ident, data, corral))]
    async fn dispatch_to<R: Resolver>(
        &self,
        ident: u32,
        data: &kaze_core::Bytes<'_>,
        corral: &Arc<Corral<R>>,
    ) -> Result<()> {
        // 1. find a node
        let conn = corral
            .find_or_create(ident)
            .await
            .context("Failed to find socket")?;

        // 2. transfer the packet
        conn.dispatch(data)
            .await
            .context("Failed to dispatch packet")
    }

    async fn dispatch_to_multicast<R: Resolver>(
        &self,
        idents: impl Iterator<Item = u32>,
        data: &kaze_core::Bytes<'_>,
        corral: &Arc<Corral<R>>,
    ) -> Result<()> {
        let mut stream = idents
            .map(|ident| self.dispatch_to(ident, &data, corral))
            .collect::<FuturesUnordered<_>>();
        while let Some(e) = stream.next().await {
            e.context("Failed to dispatch packet in multicast")?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self, data, corral))]
    async fn dispatch_to_random<R: Resolver>(
        &self,
        mask: DstMask,
        data: &kaze_core::Bytes<'_>,
        corral: &Arc<Corral<R>>,
    ) -> Result<()> {
        let mut selected_result = None;
        let mut count = 1;
        corral
            .resolver()
            .visit_masked_nodes(mask.ident, mask.mask, |ident, _| {
                let mut rng = rand::thread_rng();
                if rng.gen_ratio(1, count) {
                    selected_result = Some(ident);
                }
                count += 1;
            })
            .await;
        if let Some(ident) = selected_result {
            self.dispatch_to(ident, data, corral).await
        } else {
            Ok(())
        }
    }

    async fn dispatch_to_broadcast<R: Resolver>(
        &self,
        mask: DstMask,
        data: &kaze_core::Bytes<'_>,
        reg: &Arc<Corral<R>>,
    ) -> Result<()> {
        let mut stream = FuturesUnordered::new();
        reg.resolver()
            .visit_masked_nodes(mask.ident, mask.mask, |ident, _| {
                stream.push(self.dispatch_to(ident, data, reg));
            })
            .await;
        while let Some(e) = stream.next().await {
            e.context("Failed to dispatch packet in multicast")?;
        }
        Ok(())
    }
}
