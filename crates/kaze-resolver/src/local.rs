use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
};

use kaze_protocol::message::Node;
use papaya::HashMap;

use crate::Resolver;

static LOCAL_NODE: Mutex<Node> = Mutex::new(Node::new(
    0,
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
));

/// set local node ident and address
pub fn set_local_node(ident: u32, addr: SocketAddr) {
    let mut node = LOCAL_NODE.lock().unwrap();
    node.ident = ident;
    node.addr = addr;
}

/// get local node ident and address
pub fn local_node() -> Node {
    LOCAL_NODE.lock().unwrap().clone()
}

/// resolve ident to node address
pub struct Local {
    node_map: Arc<HashMap<u32, SocketAddr>>,
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
            .filter(|(&e, _)| e & mask == ident)
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
