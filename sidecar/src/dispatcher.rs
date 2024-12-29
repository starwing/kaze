use std::{io::IoSlice, sync::Arc};

use anyhow::{bail, Context, Result};
use metrics::counter;
use rand::seq::SliceRandom;
use tokio::{io::AsyncWriteExt, net::tcp::OwnedWriteHalf, sync::Mutex};
use tokio_stream::StreamExt;
use tracing::instrument;

use crate::{
    kaze::{
        self,
        hdr::{
            DstMask,
            RouteType::{DstBroadcast, DstIdent, DstMulticast, DstRandom},
        },
    },
    register::Register,
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
    #[instrument(level = "trace", skip(self, reg, resolver, hdr, data))]
    pub async fn dispatch(
        &self,
        reg: &Arc<Register>,
        resolver: &Resolver,
        hdr: &kaze::Hdr,
        data: &kaze_core::Bytes<'_>,
    ) -> Result<()> {
        if hdr.route_type.is_none() {
            counter!("kaze_dispatch_errors_total", "bodyType" => hdr.body_type.clone())
                .increment(1);
            bail!("no route type for packet seq={}", hdr.seq);
        }
        match hdr.route_type.as_ref().unwrap() {
            DstIdent(ident) => {
                self.dispatch_to(reg, resolver, *ident, data).await
            }
            DstRandom(mask) => {
                self.dispatch_to_random(reg, resolver, *mask, data).await
            }
            DstBroadcast(mask) => {
                self.dispatch_to_broadcast(reg, resolver, *mask, data).await
            }
            DstMulticast(multicast) => {
                let idents = multicast.dst_idents.iter().cloned();
                self.dispatch_to_multicast(reg, resolver, idents, &data)
                    .await
            }
        }
        .context("Failed to dispatch packet")
    }

    #[instrument(level = "trace", skip(self, reg, resolver, data))]
    async fn dispatch_to(
        &self,
        reg: &Arc<Register>,
        resolver: &resolver::Resolver,
        ident: u32,
        data: &kaze_core::Bytes<'_>,
    ) -> Result<()> {
        // 1. find a node
        let sock_write = reg
            .find_socket(resolver, ident)
            .await
            .context("Failed to find socket")?;

        // 2. transfer the packet
        Self::dispatch_to_socket(data, sock_write)
            .await
            .context("Failed to dispatch packet")
    }

    async fn dispatch_to_multicast(
        &self,
        reg: &Arc<Register>,
        resolver: &Resolver,
        idents: impl Iterator<Item = u32>,
        data: &kaze_core::Bytes<'_>,
    ) -> Result<()> {
        let mut stream = idents
            .map(|ident| self.dispatch_to(reg, resolver, ident, &data))
            .collect::<futures::stream::FuturesUnordered<_>>();
        while let Some(e) = stream.next().await {
            e.context("Failed to dispatch packet in multicast")?;
        }
        Ok(())
    }

    async fn dispatch_to_random(
        &self,
        reg: &Arc<Register>,
        resolver: &resolver::Resolver,
        mask: DstMask,
        data: &kaze_core::Bytes<'_>,
    ) -> Result<()> {
        let idents = resolver.get_mask_nodes(mask.ident, mask.mask).await;
        if let Some((ident, _)) = idents.choose(&mut rand::thread_rng()) {
            self.dispatch_to(reg, resolver, *ident, data)
                .await
                .context("Failed to dispatch packet in random")?
        }
        Ok(())
    }

    async fn dispatch_to_broadcast(
        &self,
        reg: &Arc<Register>,
        resolver: &resolver::Resolver,
        mask: DstMask,
        data: &kaze_core::Bytes<'_>,
    ) -> Result<()> {
        // 1. find a node
        let nodes = resolver.get_mask_nodes(mask.ident, mask.mask).await;
        let idents = nodes.iter().map(|(ident, _)| *ident);

        // 2. transfer the packet
        self.dispatch_to_multicast(reg, resolver, idents, data)
            .await
            .context("Failed to dispatch packet in multicast")
    }

    async fn dispatch_to_socket(
        data: &kaze_core::Bytes<'_>,
        socket: Arc<Mutex<OwnedWriteHalf>>,
    ) -> Result<()> {
        let size_buf = (data.len() as u32).to_le_bytes();
        let (s1, s2) = data.as_slice();
        let written = socket
            .lock()
            .await
            .write_vectored(&[
                IoSlice::new(&size_buf),
                IoSlice::new(s1),
                IoSlice::new(s2),
            ])
            .await
            .context("Failed to write to socket")?;
        counter!("kaze_write_packets_total").increment(written as u64);
        Ok(())
    }
}
