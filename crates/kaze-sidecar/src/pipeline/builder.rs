#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kaze_plugin::{
        service::ServiceExt, ClapDefault as _, PipelineService,
    };
    use kaze_resolver::ResolverExt;
    use scopeguard::defer;
    use tokio::sync::Notify;
    use tower::{util::BoxCloneSyncService, ServiceBuilder};

    use crate::plugins::{corral, ratelimit, tracker::RpcTracker};
    use kaze_plugin::protocol::{
        packet::{new_bytes_pool, Packet},
        proto::Hdr,
        service::{SinkMessage, ToMessageService},
    };
    use kaze_plugin::util::tower_ext::ServiceExt as _;
    use kaze_plugin::PipelineRequired;

    #[tokio::test]
    async fn test_builder() {
        let edge = kaze_edge::Options {
            name: "sidecar_test".to_string(),
            ident: "127.0.0.1".parse().unwrap(),
            unlink: true,
            ..kaze_edge::Options::new()
        };
        let (prefix, ident) = (edge.name.clone(), edge.ident);
        defer! {
            kaze_edge::Edge::unlink(prefix, ident).unwrap();
        }
        let edge = dbg!(edge).build().unwrap();
        let pool = new_bytes_pool();
        let (tx, _rx) = edge.into_split(&pool);

        let resolver = Arc::new(kaze_resolver::local::Local::new());
        let ratelimit = ratelimit::Options::default().build();
        let corral = corral::Options::default().build(pool.clone());
        let tracker = RpcTracker::new(10, Notify::new());

        let sink = ServiceBuilder::new()
            .layer(ToMessageService.into_layer())
            .layer(ratelimit.into_filter())
            .layer(resolver.clone().into_service().into_filter())
            .layer(tracker.clone().into_filter())
            .layer(corral.clone().into_filter())
            .layer(tx.clone().into_layer())
            .service(SinkMessage)
            .map_response(|_| ());
        let mut sink: PipelineService =
            BoxCloneSyncService::new(sink.into_tower());
        sink.ready_call((Packet::from_hdr(Hdr::default()), None))
            .await
            .unwrap();

        corral.sink().set(sink.clone());
        tracker.sink().set(sink.clone());
    }
}
