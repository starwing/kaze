#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use scopeguard::defer;
    use tower::{util::BoxCloneService, ServiceBuilder};

    use crate::{pipeline::corral_layer, plugins::ratelimit};
    use kaze_protocol::{
        message::PacketWithAddr,
        packet::{new_bytes_pool, Packet},
        proto::Hdr,
        service::{SinkMessage, ToMessageService},
    };
    use kaze_resolver::dispatch_service;
    use kaze_util::tower_ext::ServiceExt as _;
    use kaze_util::tower_ext::{ChainLayer, ServiceCell};

    #[tokio::test]
    async fn test_builder() {
        let edge = kaze_edge::Options {
            name: "sidecar_test".to_string(),
            ident: "127.0.0.1".parse().unwrap(),
            unlink: true,
            ..kaze_edge::Options::default()
        };
        let (prefix, ident) = (edge.name.clone(), edge.ident);
        defer! {
            kaze_edge::Edge::unlink(prefix, ident).unwrap();
        }
        let edge = dbg!(edge).build().unwrap();
        let (tx, _rx) = edge.split();

        let pool = new_bytes_pool();
        let resolver = Arc::new(kaze_resolver::local::Local::new());
        let ratelimit = ratelimit::Options::default().build();

        let sink_cell = ServiceCell::<
            BoxCloneService<PacketWithAddr, (), anyhow::Error>,
        >::new();
        let corral = Arc::new(
            kaze_corral::Options::default().build(pool.clone(), sink_cell),
        );

        let sink = ServiceBuilder::new()
            .layer(ChainLayer::new(ToMessageService::new()))
            .layer(ChainLayer::new(ratelimit.service()))
            .layer(ChainLayer::new(dispatch_service(resolver)))
            .layer(corral_layer(corral.clone()))
            .layer(tx.layer(pool))
            .service(SinkMessage::new());
        let mut sink: BoxCloneService<PacketWithAddr, (), anyhow::Error> =
            BoxCloneService::new(sink);
        sink.ready_call((Packet::from_hdr(Hdr::default()), None))
            .await
            .unwrap();

        corral.sink().set(sink);
    }
}
