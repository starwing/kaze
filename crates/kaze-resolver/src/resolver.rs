use std::net::SocketAddr;

/// Trait that defines the core low-level poll-based methods
pub trait Resolver: Send + Sync + 'static {
    /// Add a ident->node mapping
    fn add_node(
        &self,
        ident: u32,
        addr: SocketAddr,
    ) -> impl std::future::Future<Output = ()>;

    /// Get node address by ident
    fn get_node(
        &self,
        ident: u32,
    ) -> impl std::future::Future<Output = Option<SocketAddr>>;

    /// Get node address list by ident and mask
    ///
    fn visit_nodes(
        &self,
        ident: impl Iterator<Item = u32> + Clone,
        f: impl FnMut(u32, SocketAddr),
    ) -> impl std::future::Future<Output = ()>;

    /// Visit all nodes that match `(ident & mask)`
    fn visit_masked_nodes(
        &self,
        ident: u32,
        mask: u32,
        f: impl FnMut(u32, SocketAddr),
    ) -> impl std::future::Future<Output = ()>;
}
