use kaze_plugin::service::AsyncService;
use rand::Rng;
use tracing::error;

use kaze_plugin::local_node;
use kaze_plugin::protocol::{
    message::{Destination, Message, Node},
    proto::hdr::{DstMask, DstMulticast, RouteType},
};

use crate::Resolver;

pub trait ResolverExt: Resolver {
    fn into_service(self) -> ResolverService<Self>
    where
        Self: Clone,
    {
        ResolverService::new(self)
    }
}

impl<R> ResolverExt for R where R: Resolver {}

#[derive(Debug, Clone, Copy)]
pub struct ResolverService<Resolver> {
    resolver: Resolver,
}

impl<T> ResolverService<T> {
    pub fn new(resolver: T) -> Self {
        Self { resolver }
    }

    pub fn resolver(&self) -> &T {
        &self.resolver
    }
}

impl<T> From<T> for ResolverService<T>
where
    T: Resolver,
{
    fn from(resolver: T) -> Self {
        Self { resolver }
    }
}

impl<T> AsyncService<Message> for ResolverService<T>
where
    T: Resolver + Clone,
{
    type Response = Option<Message>;
    type Error = anyhow::Error;

    async fn serve(
        &self,
        mut msg: Message,
    ) -> Result<Self::Response, Self::Error> {
        let route_type = msg.packet().hdr().route_type.clone();
        if let Some(dst) = dispatch(route_type, self.resolver.clone()).await {
            msg.set_destination(dst);
        } else {
            // can not find route
            error!(hdr = ?msg.packet().hdr(), "Can not find route");
            return Ok(None);
        }
        Ok(Some(msg))
    }
}

async fn dispatch(
    route_type: Option<RouteType>,
    resolver: impl Resolver,
) -> Option<Destination> {
    let Some(route_type) = route_type else {
        return None;
    };
    match route_type {
        RouteType::DstIdent(ident) if ident == local_node().ident => {
            Some(Destination::Host)
        }
        RouteType::DstIdent(ident) => dispatch_ident(&resolver, ident).await,
        RouteType::DstRandom(DstMask { ident, mask }) => {
            dispatch_random(&resolver, ident, mask).await
        }
        RouteType::DstBroadcast(DstMask { ident, mask }) => {
            dispatch_broadcast(&resolver, ident, mask).await
        }
        RouteType::DstMulticast(DstMulticast { dst_idents }) => {
            dispatch_multicast(&resolver, dst_idents.iter().cloned()).await
        }
    }
}

async fn dispatch_ident(
    resolver: &impl Resolver,
    ident: u32,
) -> Option<Destination> {
    resolver
        .get_node(ident)
        .await
        .map(|addr| Destination::Node(Node::new(ident, addr)))
}

async fn dispatch_random(
    resolver: &impl Resolver,
    ident: u32,
    mask: u32,
) -> Option<Destination> {
    let mut result = None;
    let mut cnt = 1;
    resolver
        .visit_masked_nodes(ident, mask, |ident, addr| {
            if rand::rng().random_ratio(1, cnt) {
                result = Some(Node::new(ident, addr));
            }
            cnt += 1;
        })
        .await;
    result.map(Destination::Node)
}

async fn dispatch_broadcast(
    resolver: &impl Resolver,
    ident: u32,
    mask: u32,
) -> Option<Destination> {
    let mut vec = Vec::new();
    resolver
        .visit_masked_nodes(ident, mask, |ident, addr| {
            vec.push(Node::new(ident, addr));
        })
        .await;
    Some(Destination::NodeList(vec))
}

async fn dispatch_multicast(
    resolver: &impl Resolver,
    dst_idents: impl Iterator<Item = u32> + Clone + Send,
) -> Option<Destination> {
    let mut vec = Vec::new();
    resolver
        .visit_nodes(dst_idents, |ident, addr| {
            vec.push(Node::new(ident, addr));
        })
        .await;
    Some(Destination::NodeList(vec))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kaze_plugin::{
        ClapDefault,
        protocol::{message::Source, packet::Packet, proto::Hdr},
    };

    use crate::LocalOptions;

    use super::*;

    #[tokio::test]
    async fn test_send() {
        let resolver: ResolverService<_> =
            Arc::new(LocalOptions::default().build().await).into();
        assert!(
            resolver
                .serve(Message::new(
                    Packet::from_hdr(Hdr::default()),
                    Source::Host
                ))
                .await
                .is_ok()
        );
    }
}
