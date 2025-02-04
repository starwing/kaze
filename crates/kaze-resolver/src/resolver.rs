use std::{net::SocketAddr, sync::Arc};

/// Trait that defines the core low-level poll-based methods
pub trait Resolver: Send + Sync + 'static {
    /// Add a ident->node mapping
    fn add_node(
        &self,
        ident: u32,
        addr: SocketAddr,
    ) -> impl std::future::Future<Output = ()> + Send;

    /// Get node address by ident
    fn get_node(
        &self,
        ident: u32,
    ) -> impl std::future::Future<Output = Option<SocketAddr>> + Send;

    /// Get node address list by ident and mask
    ///
    fn visit_nodes(
        &self,
        ident: impl Iterator<Item = u32> + Clone + Send,
        f: impl FnMut(u32, SocketAddr) + Send,
    ) -> impl std::future::Future<Output = ()> + Send;

    /// Visit all nodes that match `(ident & mask)`
    fn visit_masked_nodes(
        &self,
        ident: u32,
        mask: u32,
        f: impl FnMut(u32, SocketAddr) + Send,
    ) -> impl std::future::Future<Output = ()> + Send;
}

impl<T> Resolver for Arc<T>
where
    T: Resolver + Send + Sync + 'static,
{
    fn add_node(
        &self,
        ident: u32,
        addr: SocketAddr,
    ) -> impl std::future::Future<Output = ()> + Send {
        self.as_ref().add_node(ident, addr)
    }

    fn get_node(
        &self,
        ident: u32,
    ) -> impl std::future::Future<Output = Option<SocketAddr>> + Send {
        self.as_ref().get_node(ident)
    }

    fn visit_nodes(
        &self,
        ident: impl Iterator<Item = u32> + Clone + Send,
        f: impl FnMut(u32, SocketAddr) + Send,
    ) -> impl std::future::Future<Output = ()> + Send {
        self.as_ref().visit_nodes(ident, f)
    }

    fn visit_masked_nodes(
        &self,
        ident: u32,
        mask: u32,
        f: impl FnMut(u32, SocketAddr) + Send,
    ) -> impl std::future::Future<Output = ()> + Send {
        self.as_ref().visit_masked_nodes(ident, mask, f)
    }
}
