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

    /// create a new resolver with a given map
    pub fn from_map(map: HashMap<u32, SocketAddr>) -> Self {
        Self {
            node_map: Arc::new(map),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;

    #[tokio::test]
    async fn test_add_get_node() {
        let resolver = Local::new();
        let addr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        resolver.add_node(1, addr).await;
        let result = resolver.get_node(1).await;

        assert_eq!(result, Some(addr));
        assert_eq!(resolver.get_node(2).await, None);
    }

    #[tokio::test]
    async fn test_visit_nodes() {
        let resolver = Local::new();
        let addr1 =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let addr2 =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081);

        resolver.add_node(1, addr1).await;
        resolver.add_node(2, addr2).await;

        let mut result = vec![];
        resolver
            .visit_nodes([1, 2, 3].into_iter(), |id, addr| {
                result.push((id, addr));
            })
            .await;

        assert_eq!(result.len(), 2);
        assert!(result.contains(&(1, addr1)));
        assert!(result.contains(&(2, addr2)));
    }

    #[tokio::test]
    async fn test_visit_masked_nodes() {
        let resolver = Local::new();
        let addr1 =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let addr2 =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081);
        let addr3 =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8082);

        // Binary: 0b0001
        resolver.add_node(1, addr1).await;
        // Binary: 0b0011
        resolver.add_node(3, addr2).await;
        // Binary: 0b0101
        resolver.add_node(5, addr3).await;

        // Testing with mask 0b0011 and ident 0b0001 (should match 1)
        let mut result = vec![];
        resolver
            .visit_masked_nodes(1, 0b0011, |id, addr| {
                result.push((id, addr));
            })
            .await;

        assert_eq!(result.len(), 2);
        assert!(result.contains(&(1, addr1)));

        // Testing with mask 0b0001 and ident 0b0001 (should match 1, 3, 5)
        let mut result = vec![];
        resolver
            .visit_masked_nodes(1, 0b0001, |id, addr| {
                result.push((id, addr));
            })
            .await;

        assert_eq!(result.len(), 3);
        assert!(result.contains(&(1, addr1)));
        assert!(result.contains(&(3, addr2)));
        assert!(result.contains(&(5, addr3)));
    }

    #[tokio::test]
    async fn test_update_node() {
        let resolver = Local::new();
        let addr1 =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let addr2 =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9090);

        resolver.add_node(1, addr1).await;
        assert_eq!(resolver.get_node(1).await, Some(addr1));

        // Update the address for the same node
        resolver.add_node(1, addr2).await;
        assert_eq!(resolver.get_node(1).await, Some(addr2));
    }
}
