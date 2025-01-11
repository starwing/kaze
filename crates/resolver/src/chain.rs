use std::net::SocketAddr;

use crate::Resolver;

/// chain of multiple resolvers
pub struct Chain<R1: Resolver, R2: Resolver> {
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

    async fn visit_masked_nodes(
        &self,
        ident: u32,
        mask: u32,
        mut f: impl FnMut(u32, SocketAddr),
    ) {
        self.r1.visit_masked_nodes(ident, mask, &mut f).await;
        self.r2.visit_masked_nodes(ident, mask, &mut f).await;
    }
}
