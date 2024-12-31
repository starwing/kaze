use std::{io::IoSlice, sync::Arc};

use anyhow::{bail, Context, Result};
use metrics::counter;
use rand::seq::SliceRandom;
use tokio::{io::AsyncWriteExt, net::tcp::OwnedWriteHalf, sync::Mutex};
use tokio_stream::StreamExt;

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
    #[tracing::instrument(
        level = "trace",
        skip(self, hdr, data, reg, resolver)
    )]
    pub async fn dispatch(
        &self,
        hdr: &kaze::Hdr,
        data: &kaze_core::Bytes<'_>,
        reg: &Arc<Register>,
        resolver: &Resolver,
    ) -> Result<()> {
        if hdr.route_type.is_none() {
            counter!("kaze_dispatch_errors_total", "bodyType" => hdr.body_type.clone())
                .increment(1);
            bail!("no route type for packet seq={}", hdr.seq);
        }
        match hdr.route_type.as_ref().unwrap() {
            DstIdent(ident) => {
                self.dispatch_to(*ident, data, reg, resolver).await
            }
            DstRandom(mask) => {
                self.dispatch_to_random(*mask, data, reg, resolver).await
            }
            DstBroadcast(mask) => {
                self.dispatch_to_broadcast(*mask, data, reg, resolver).await
            }
            DstMulticast(multicast) => {
                let idents = multicast.dst_idents.iter().cloned();
                self.dispatch_to_multicast(idents, &data, reg, resolver)
                    .await
            }
        }
        .context("Failed to dispatch packet")
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, ident, data, reg, resolver)
    )]
    async fn dispatch_to(
        &self,
        ident: u32,
        data: &kaze_core::Bytes<'_>,
        reg: &Arc<Register>,
        resolver: &resolver::Resolver,
    ) -> Result<()> {
        // 1. find a node
        let sock_write = reg
            .find_socket(ident, resolver)
            .await
            .context("Failed to find socket")?;

        // 2. transfer the packet
        Self::dispatch_to_socket(data, sock_write)
            .await
            .context("Failed to dispatch packet")
    }

    async fn dispatch_to_multicast(
        &self,
        idents: impl Iterator<Item = u32>,
        data: &kaze_core::Bytes<'_>,
        reg: &Arc<Register>,
        resolver: &resolver::Resolver,
    ) -> Result<()> {
        let mut stream = idents
            .map(|ident| self.dispatch_to(ident, &data, reg, resolver))
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
        reg: &Arc<Register>,
        resolver: &resolver::Resolver,
    ) -> Result<()> {
        let idents = resolver.get_mask_nodes(mask.ident, mask.mask).await;
        if let Some((ident, _)) = idents.choose(&mut rand::thread_rng()) {
            self.dispatch_to(*ident, data, reg, resolver)
                .await
                .context("Failed to dispatch packet in random")?
        }
        Ok(())
    }

    async fn dispatch_to_broadcast(
        &self,
        mask: DstMask,
        data: &kaze_core::Bytes<'_>,
        reg: &Arc<Register>,
        resolver: &resolver::Resolver,
    ) -> Result<()> {
        // 1. find a node
        let nodes = resolver.get_mask_nodes(mask.ident, mask.mask).await;
        let idents = nodes.iter().map(|(ident, _)| *ident);

        // 2. transfer the packet
        self.dispatch_to_multicast(idents, data, reg, resolver)
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
