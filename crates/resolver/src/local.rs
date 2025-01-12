use std::{collections::HashMap, net::SocketAddr};

use tokio::sync::Mutex;

use crate::Resolver;

/// resolve ident to node address
pub struct Local(Mutex<HashMap<u32, SocketAddr>>);

impl Resolver for Local {
    async fn add_node(&self, ident: u32, addr: SocketAddr) {
        self.0.lock().await.insert(ident, addr);
    }

    async fn get_node(&self, ident: u32) -> Option<SocketAddr> {
        self.0.lock().await.get(&ident).cloned()
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
        self.0
            .lock()
            .await
            .iter()
            .filter(|(&e, _)| e & mask == ident)
            .map(|(&k, &v)| (k, v))
            .for_each(|(k, v)| f(k, v));
    }
}

impl Local {
    /// create a new resolver
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}
