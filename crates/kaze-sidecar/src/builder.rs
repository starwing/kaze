use std::sync::Arc;

use kaze_plugin::{
    PipelineService,
    protocol::{
        packet::new_bytes_pool,
        service::{SinkMessage, ToMessageService},
    },
    util::tower_ext::ChainLayer,
};
use kaze_resolver::dispatch_service;
use scopeguard::defer;
use tokio::sync::Notify;
use tower::{ServiceBuilder, util::BoxCloneSyncService};

use crate::{
    options::Options, plugins::tracker::RpcTracker, sidecar::Sidecar,
};

impl Options {
    pub async fn build(self) -> Sidecar {
        let pool = new_bytes_pool();

        let edge = self.edge;
        let (prefix, ident) = (edge.name.clone(), edge.ident);
        defer! {
            kaze_edge::Edge::unlink(prefix, ident).unwrap();
        }
        let edge = edge.build().unwrap();
        let (tx, _rx) = edge.split();

        let resolver = Arc::new(self.local.build().await);
        // TODO: handle Option<RateLimit>
        let ratelimit = self.rate_limit.map(|o| o.build()).unwrap();
        let corral = self.corral.build(pool.clone());
        let tracker = RpcTracker::new(10, Notify::new());

        let sink = ServiceBuilder::new()
            .layer(ToMessageService::new())
            .layer(ChainLayer::new(ratelimit.service()))
            .layer(ChainLayer::new(dispatch_service(resolver)))
            .layer(ChainLayer::new(tracker.clone().service()))
            .layer(corral.clone().layer())
            .layer(tx.clone().layer(pool))
            .service(SinkMessage::new());
        let sink: PipelineService = BoxCloneSyncService::new(sink);
        Sidecar::new(sink)
    }
}
