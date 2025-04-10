use rand::Rng;
use tower::service_fn;
use tracing::error;

use kaze_plugin::local_node;
use kaze_plugin::protocol::{
    message::{Destination, Message, Node},
    proto::hdr::{DstMask, DstMulticast, RouteType},
    service::MessageService,
};

use crate::Resolver;

pub fn dispatch_service<R>(resolver: R) -> impl MessageService<Message>
where
    R: Resolver + Clone,
{
    service_fn(move |req: Message| {
        let route_type = req.packet().hdr().route_type.clone();
        let resolver = resolver.clone();
        let dispatch = dispatch(route_type, resolver);
        async move {
            let mut msg: Message = req.into();
            let Some(dst) = dispatch.await else {
                // can not find route
                error!(hdr = ?msg.packet().hdr(), "Can not find route");
                return Ok::<_, anyhow::Error>(msg);
            };
            msg.set_destination(dst);
            Ok(msg)
        }
    })
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

    use kaze_plugin::default_from_clap;
    use tower::ServiceExt;

    use crate::LocalOptions;

    use super::*;

    #[tokio::test]
    async fn test_send() {
        let resolver =
            Arc::new(default_from_clap::<LocalOptions>().build().await);
        dispatch_service(resolver).boxed();
    }
}
