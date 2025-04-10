#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kaze_plugin::{default_from_clap, PipelineService};
    use scopeguard::defer;
    use tokio::sync::Notify;
    use tower::{util::BoxCloneSyncService, ServiceBuilder};

    use crate::plugins::{corral, ratelimit, tracker::RpcTracker};
    use kaze_plugin::protocol::{
        packet::{new_bytes_pool, Packet},
        proto::Hdr,
        service::{SinkMessage, ToMessageService},
    };
    use kaze_plugin::util::tower_ext::ChainLayer;
    use kaze_plugin::util::tower_ext::ServiceExt as _;
    use kaze_plugin::PipelineRequired;
    use kaze_resolver::dispatch_service;

    #[tokio::test]
    async fn test_builder() {
        let edge = kaze_edge::Options {
            name: "sidecar_test".to_string(),
            ident: "127.0.0.1".parse().unwrap(),
            unlink: true,
            ..default_from_clap()
        };
        let (prefix, ident) = (edge.name.clone(), edge.ident);
        defer! {
            kaze_edge::Edge::unlink(prefix, ident).unwrap();
        }
        let edge = dbg!(edge).build().unwrap();
        let (tx, _rx) = edge.into_split();

        let pool = new_bytes_pool();
        let resolver = Arc::new(kaze_resolver::local::Local::new());
        let ratelimit = default_from_clap::<ratelimit::Options>().build();
        let corral =
            default_from_clap::<corral::Options>().build(pool.clone());
        let tracker = RpcTracker::new(10, Notify::new());

        let sink = ServiceBuilder::new()
            .layer(ToMessageService::new())
            .layer(ChainLayer::new(ratelimit.service()))
            .layer(ChainLayer::new(dispatch_service(resolver)))
            .layer(ChainLayer::new(tracker.clone().service()))
            .layer(corral.clone().layer())
            .layer(tx.clone().layer(pool))
            .service(SinkMessage::new());
        let mut sink: PipelineService = BoxCloneSyncService::new(sink);
        sink.ready_call((Packet::from_hdr(Hdr::default()), None))
            .await
            .unwrap();

        corral.sink().set(sink.clone());
        tracker.sink().set(sink.clone());
    }
}
