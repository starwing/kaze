use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use moka::future::Cache;
use tokio::sync::Mutex;

use crate::Resolver;

/// resolve ident to node address
pub struct LocalResolver {
    node_map: Mutex<HashMap<u32, SocketAddr>>,
    mask_cache: Cache<(u32, u32), Arc<Vec<(u32, SocketAddr)>>>,
}

impl Resolver for LocalResolver {
    async fn add_node(&self, ident: u32, addr: SocketAddr) {
        self.node_map.lock().await.insert(ident, addr);
    }

    async fn get_node(&self, ident: u32) -> Option<SocketAddr> {
        self.node_map.lock().await.get(&ident).cloned()
    }

    /// Get node address list by ident and mask
    ///
    /// Returns all nodes that match `(ident & mask)`
    #[tracing::instrument(level = "trace", skip(self, f))]
    async fn visit_masked_nodes(
        &self,
        ident: u32,
        mask: u32,
        mut f: impl FnMut(u32, SocketAddr),
    ) {
        self.mask_cache
            .get_with((ident, mask), self.calc_mask_nodes(ident, mask))
            .await
            .iter()
            .for_each(|(ident, addr)| f(*ident, *addr));
    }
}

impl LocalResolver {
    /// create a new resolver
    pub fn new(cache_capactiy: usize, live_time: impl Into<Duration>) -> Self {
        Self {
            node_map: Mutex::new(HashMap::new()),
            mask_cache: Cache::builder()
                .name("resolver-cache")
                .max_capacity(cache_capactiy as u64)
                .weigher(|_, v: &Arc<Vec<(u32, SocketAddr)>>| v.len() as u32)
                .time_to_live(live_time.into())
                .build(),
        }
    }

    /// calculate all nodes that match `(ident & mask)`
    async fn calc_mask_nodes(
        &self,
        ident: u32,
        mask: u32,
    ) -> Arc<Vec<(u32, SocketAddr)>> {
        Arc::new(
            self.node_map
                .lock()
                .await
                .iter()
                .filter(|(&e, _)| e & mask == ident)
                .map(|(&k, &v)| (k, v))
                .collect(),
        )
    }
}
