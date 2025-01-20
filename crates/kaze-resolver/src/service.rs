use std::net::SocketAddr;

use rand::Rng;
use tower::{
    layer::{layer_fn, util::Stack},
    service_fn, Layer, Service,
};
use tracing::error;

use kaze_protocol::{
    message::{Destination, Message, Node},
    packet::Packet,
    proto::hdr::{DstMask, DstMulticast, RouteType},
};

use crate::{local::local_node, Resolver};

pub fn dispatch_service<R>(
    resolver: &R,
) -> impl Service<(Packet, Option<SocketAddr>), Response = Message> + use<'_, R>
where
    R: Resolver,
{
    service_fn(move |req: (Packet, Option<SocketAddr>)| {
        let route_type = req.0.hdr().route_type.clone();
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

pub fn dispatch_layer<R, T>(resolver: &R) -> impl Layer<T> + use<'_, R, T>
where
    R: Resolver,
    T: Service<Message>,
{
    layer_fn(move |inner: T| {
        let svc = dispatch_service(resolver);
        Stack::new(svc, inner)
    })
}

async fn dispatch(
    route_type: Option<RouteType>,
    resolver: &impl Resolver,
) -> Option<Destination> {
    let Some(route_type) = route_type else {
        return None;
    };
    match route_type {
        RouteType::DstIdent(ident) if ident == local_node().ident => {
            Some(Destination::Host)
        }
        RouteType::DstIdent(ident) => dispatch_ident(resolver, ident).await,
        RouteType::DstRandom(DstMask { ident, mask }) => {
            dispatch_random(resolver, ident, mask).await
        }
        RouteType::DstBroadcast(DstMask { ident, mask }) => {
            dispatch_broadcast(resolver, ident, mask).await
        }
        RouteType::DstMulticast(DstMulticast { dst_idents }) => {
            dispatch_multicast(resolver, dst_idents.iter().cloned()).await
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
    let mut rng = rand::thread_rng();
    let mut result = None;
    let mut cnt = 1;
    resolver
        .visit_masked_nodes(ident, mask, |ident, addr| {
            if rng.gen_ratio(1, cnt) {
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
    dst_idents: impl Iterator<Item = u32> + Clone,
) -> Option<Destination> {
    let mut vec = Vec::new();
    resolver
        .visit_nodes(dst_idents, |ident, addr| {
            vec.push(Node::new(ident, addr));
        })
        .await;
    Some(Destination::NodeList(vec))
}
