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
#[cfg(test)]
mod tests {
    use papaya::HashMap;

    use crate::local::Local;

    use super::*;

    #[tokio::test]
    async fn test_cached_resolver() {
        let mock = Local::from_map(HashMap::from([
            (1, "127.0.0.1:8080".parse().unwrap()),
            (2, "127.0.0.1:8081".parse().unwrap()),
            (3, "127.0.0.1:8082".parse().unwrap()),
        ]));

        let cached = Cached::new(mock, 100, Duration::from_secs(60));

        // Test add_node
        cached.add_node(4, "127.0.0.1:8083".parse().unwrap()).await;
        assert_eq!(
            cached.get_node(4).await,
            Some("127.0.0.1:8083".parse().unwrap())
        );

        // Test get_node
        assert_eq!(
            cached.get_node(1).await,
            Some("127.0.0.1:8080".parse().unwrap())
        );
        assert_eq!(cached.get_node(5).await, None);

        // Test visit_nodes
        let mut results = Vec::new();
        cached
            .visit_nodes([1, 2, 3].into_iter(), |id, addr| {
                results.push((id, addr));
            })
            .await;
        assert_eq!(results.len(), 3);

        // Test visit_masked_nodes and cache
        let mask = 0xFF;
        let mut mask_results1 = Vec::new();
        cached
            .visit_masked_nodes(1, mask, |id, addr| {
                mask_results1.push((id, addr));
            })
            .await;

        // Call again to verify it uses the cache
        let mut mask_results2 = Vec::new();
        cached
            .visit_masked_nodes(1, mask, |id, addr| {
                mask_results2.push((id, addr));
            })
            .await;

        assert_eq!(mask_results1, mask_results2);
    }
}
