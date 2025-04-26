#[cfg(test)]
mod tests {
    use kaze_plugin::{
        service::ServiceExt, tokio_graceful::Shutdown, ClapDefault as _,
        Context, PipelineService, PluginFactory as _,
    };
    use kaze_resolver::ResolverExt;
    use scopeguard::defer;
    use tower::{util::BoxCloneSyncService, ServiceBuilder};

    use crate::plugins::{corral, ratelimit, tracker::RpcTracker};
    use kaze_plugin::protocol::{
        packet::Packet,
        proto::Hdr,
        service::{SinkMessage, ToMessageService},
    };
    use kaze_plugin::util::tower_ext::ServiceExt as _;

    #[tokio::test]
    async fn test_builder() {
        let signal = tokio::time::sleep(std::time::Duration::from_millis(100));

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
        let edge = edge.build().unwrap();
        let (tx, rx) = edge.into_split();

        let resolver = kaze_resolver::local::Local::new();
        let ratelimit = ratelimit::Options::default().build().unwrap();
        let corral = corral::Options::default().build().unwrap();
        let tracker = RpcTracker::new(10);

        let sink = ServiceBuilder::new()
            .layer(ToMessageService.into_layer())
            .layer(ratelimit.clone().into_filter())
            .layer(resolver.clone().into_service().into_filter())
            .layer(tracker.clone().into_filter())
            .layer(corral.clone().into_filter())
            .layer(tx.clone().into_filter())
            .service(SinkMessage.map_response(|_| Some(())))
            .map_response(|_| ());
        let mut sink: PipelineService =
            BoxCloneSyncService::new(sink.into_tower());
        sink.ready_call((Packet::from_hdr(Hdr::default()), None))
            .await
            .unwrap();

        let shutdown = Shutdown::builder()
            .with_delay(std::time::Duration::from_millis(50))
            .with_signal(signal)
            .with_overwrite_fn(tokio::signal::ctrl_c)
            .build();

        let ctx = Context::builder()
            .register(corral.clone())
            .register(ratelimit)
            .register(resolver)
            .register(tracker.clone())
            .register(tx)
            .register(rx)
            .build(shutdown.guard());
        ctx.sink().set(sink);
    }
}
