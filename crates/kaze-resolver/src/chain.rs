use std::net::SocketAddr;

use crate::Resolver;

/// chain of multiple resolvers
pub struct Chain<R1: Resolver + Send, R2: Resolver + Send> {
    r1: R1,
    r2: R2,
}

impl<R1: Resolver, R2: Resolver> Resolver for Chain<R1, R2> {
    async fn add_node(&self, ident: u32, addr: SocketAddr) {
        self.r1.add_node(ident, addr).await;
        self.r2.add_node(ident, addr).await;
    }

    async fn get_node(&self, ident: u32) -> Option<SocketAddr> {
        if let Some(addr) = self.r1.get_node(ident).await {
            return Some(addr);
        }

        self.r2.get_node(ident).await
    }

    async fn visit_nodes(
        &self,
        idents: impl Iterator<Item = u32> + Clone + Send,
        mut f: impl FnMut(u32, SocketAddr) + Send,
    ) {
        self.r1.visit_nodes(idents.clone(), &mut f).await;
        self.r2.visit_nodes(idents, &mut f).await;
    }

    async fn visit_masked_nodes(
        &self,
        ident: u32,
        mask: u32,
        mut f: impl FnMut(u32, SocketAddr) + Send,
    ) {
        self.r1.visit_masked_nodes(ident, mask, &mut f).await;
        self.r2.visit_masked_nodes(ident, mask, &mut f).await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::local::Local;

    use super::*;

    #[tokio::test]
    async fn test_chain_add_get_node() {
        let r1 = Local::new();
        let r2 = Local::new();
        let chain = Chain { r1, r2 };

        let addr1: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        chain.add_node(1, addr1).await;

        assert_eq!(chain.get_node(1).await, Some(addr1));
        assert_eq!(chain.get_node(2).await, None);
    }

    #[tokio::test]
    async fn test_chain_fallback() {
        let r1 = Local::new();
        let r2 = Local::new();
        let chain = Chain { r1, r2 };

        let addr1: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:9000".parse().unwrap();

        // Only add to second resolver
        chain.r2.add_node(1, addr1).await;

        // Should fallback to r2
        assert_eq!(chain.get_node(1).await, Some(addr1));

        // Add different address to first resolver
        chain.r1.add_node(1, addr2).await;

        // Should get from first resolver now
        assert_eq!(chain.get_node(1).await, Some(addr2));
    }

    #[tokio::test]
    async fn test_visit_nodes() {
        let r1 = Local::new();
        let r2 = Local::new();
        let chain = Chain { r1, r2 };

        let addr1: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8002".parse().unwrap();
        let addr3: SocketAddr = "127.0.0.1:8003".parse().unwrap();

        // Add nodes to different resolvers
        chain.r1.add_node(1, addr1).await;
        chain.r2.add_node(2, addr2).await;
        chain.r1.add_node(3, addr3).await;

        let mut visited = HashMap::new();
        chain
            .visit_nodes([1, 2, 3].iter().copied(), |id, addr| {
                visited.insert(id, addr);
            })
            .await;
        // Should visit all nodes
        assert_eq!(visited.len(), 3);
        assert_eq!(visited.get(&1), Some(&addr1));
        assert_eq!(visited.get(&2), Some(&addr2));
        assert_eq!(visited.get(&3), Some(&addr3));
    }

    #[tokio::test]
    async fn test_visit_masked_nodes() {
        let r1 = Local::new();
        let r2 = Local::new();
        let chain = Chain { r1, r2 };

        let addr1: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8002".parse().unwrap();
        let addr3: SocketAddr = "127.0.0.1:8003".parse().unwrap();
        let addr4: SocketAddr = "127.0.0.1:8004".parse().unwrap();

        // Add nodes to different resolvers with IDs that match mask pattern
        chain.r1.add_node(0b1010, addr1).await;
        chain.r2.add_node(0b1110, addr2).await;
        chain.r1.add_node(0b0010, addr3).await;
        chain.r2.add_node(0b0011, addr4).await;

        let mut visited = HashMap::new();
        chain
            .visit_masked_nodes(0b0010, 0b0111, |id, addr| {
                visited.insert(id, addr);
            })
            .await;

        // Should match IDs with pattern xx10
        assert_eq!(visited.len(), 2);
        assert_eq!(visited.get(&0b1010), Some(&addr1));
        assert_eq!(visited.get(&0b0010), Some(&addr3));
        assert!(!visited.contains_key(&0b0011));
    }
}
