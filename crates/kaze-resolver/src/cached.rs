use std::{net::SocketAddr, sync::Arc, time::Duration};

use moka::future::Cache;

use crate::Resolver;

/// resolve ident to node address
pub struct Cached<R: Resolver> {
    resolver: R,
    mask_cache: Cache<(u32, u32), Arc<Vec<(u32, SocketAddr)>>>,
}

impl<R: Resolver> Cached<R> {
    pub fn new(
        resolver: R,
        cache_size: usize,
        live_time: impl Into<Duration>,
    ) -> Self {
        Self {
            resolver,
            mask_cache: Cache::builder()
                .name("resolver-cache")
                .max_capacity(cache_size as u64)
                .weigher(|_, v: &Arc<Vec<(u32, SocketAddr)>>| v.len() as u32)
                .time_to_live(live_time.into())
                .build(),
        }
    }

    pub async fn calc_mask_nodes(
        &self,
        ident: u32,
        mask: u32,
    ) -> Arc<Vec<(u32, SocketAddr)>> {
        let mut r = Vec::new();
        self.resolver
            .visit_masked_nodes(ident, mask, |ident, addr| {
                r.push((ident, addr));
            })
            .await;
        Arc::new(r)
    }
}

impl<R: Resolver> Resolver for Cached<R> {
    async fn add_node(&self, ident: u32, addr: SocketAddr) -> () {
        self.resolver.add_node(ident, addr).await
    }

    async fn get_node(&self, ident: u32) -> Option<SocketAddr> {
        self.resolver.get_node(ident).await
    }

    async fn visit_nodes(
        &self,
        idents: impl Iterator<Item = u32> + Clone + Send,
        mut f: impl FnMut(u32, SocketAddr) + Send,
    ) {
        self.resolver.visit_nodes(idents, &mut f).await
    }

    async fn visit_masked_nodes(
        &self,
        ident: u32,
        mask: u32,
        mut f: impl FnMut(u32, SocketAddr) + Send,
    ) {
        self.mask_cache
            .get_with((ident, mask), self.calc_mask_nodes(ident, mask))
            .await
            .iter()
            .for_each(|(ident, addr)| f(*ident, *addr));
    }
}
