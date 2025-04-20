use std::{net::SocketAddr, sync::Arc};

use kaze_plugin::Plugin;
use papaya::HashMap;

use crate::Resolver;

/// resolve ident to node address
#[derive(Clone)]
pub struct Local {
    node_map: Arc<HashMap<u32, SocketAddr>>,
}

impl Default for Local {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for Local {
    fn init(&self, _context: kaze_plugin::Context) {}
    fn context(&self) -> &kaze_plugin::Context {
        unreachable!("Local resolver does not have a context")
    }
}

impl Resolver for Local {
    async fn add_node(&self, ident: u32, addr: SocketAddr) {
        self.node_map.pin().insert(ident, addr);
    }

    async fn get_node(&self, ident: u32) -> Option<SocketAddr> {
        self.node_map.pin().get(&ident).cloned()
    }

    async fn visit_nodes(
        &self,
        idents: impl Iterator<Item = u32>,
        mut f: impl FnMut(u32, SocketAddr),
    ) -> () {
        let node_map = self.node_map.pin();
        for ident in idents {
            if let Some(addr) = node_map.get(&ident) {
                f(ident, *addr);
            }
        }
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
        self.node_map
            .pin()
            .iter()
            .filter(|&(&e, _)| e & mask == ident)
            .map(|(&k, &v)| (k, v))
            .for_each(|(k, v)| f(k, v));
    }
}

impl Local {
    /// create a new resolver
    pub fn new() -> Self {
        Self {
            node_map: Arc::new(HashMap::new()),
        }
    }
}
