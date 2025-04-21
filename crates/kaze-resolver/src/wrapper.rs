#[macro_export]
macro_rules! resolver_wrapper {
    ($name:ident) => {
        resolver_wrapper!($name, resolver);
    };
    ($name:ident, $field:ident) => {
        make_wrapper!($name, resolver);

        impl<R: Resolver> Resolver for $name<R> {
            fn add_node(
                &self,
                ident: u32,
                addr: std::net::SocketAddr,
            ) -> impl std::future::Future<Output = ()> + Send {
                self.$field.add_node(ident, addr)
            }

            fn get_node(
                &self,
                ident: u32,
            ) -> impl std::future::Future<Output = Option<std::net::SocketAddr>> + Send
            {
                self.$field.get_node(ident)
            }

            fn visit_nodes(
                &self,
                ident: impl Iterator<Item = u32> + Clone + Send,
                f: impl FnMut(u32, std::net::SocketAddr) + Send,
            ) -> impl std::future::Future<Output = ()> + Send {
                self.$field.visit_nodes(ident, f)
            }

            fn visit_masked_nodes(
                &self,
                ident: u32,
                mask: u32,
                f: impl FnMut(u32, std::net::SocketAddr) + Send,
            ) -> impl std::future::Future<Output = ()> + Send {
                self.$field.visit_masked_nodes(ident, mask, f)
            }
        }
    };
}
