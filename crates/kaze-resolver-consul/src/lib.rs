mod options;

use std::{
    net::Ipv4Addr,
    sync::{Arc, OnceLock},
};

use anyhow::Context as _;
use options::Options;
use rs_consul::types::*;

use kaze_plugin::{Context, Plugin, local_node};
use kaze_resolver::Resolver;
use tokio::select;
use tracing::info;

/// Kaze Resolver implemented with Consul
#[derive(Clone)]
pub struct ConsulResolver {
    inner: Arc<Inner>,
}

impl Plugin for ConsulResolver {
    fn context_storage(&self) -> Option<&OnceLock<Context>> {
        Some(&self.inner.ctx)
    }

    fn run(&self) -> Option<kaze_plugin::PluginRunFuture> {
        Some(Box::pin(self.clone().keep_alive()))
    }
}

struct Inner {
    consul: rs_consul::Consul,
    ctx: OnceLock<Context>,
}

impl ConsulResolver {
    pub(crate) fn new(client: rs_consul::Consul) -> Self {
        ConsulResolver {
            inner: Arc::new(Inner {
                consul: client,
                ctx: OnceLock::new(),
            }),
        }
    }

    async fn keep_alive(self) -> anyhow::Result<()> {
        let ctx = self.context().clone();
        let payload = Self::make_payload(&ctx)
            .context("Failed to make register payload")?;

        loop {
            self.register(&payload)
                .await
                .context("Failed to register current service")?;

            select! {
                _ = ctx.exiting() => break,
                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {},
            };
        }
        info!("consul resolver exited");
        Ok(())
    }

    async fn register(
        &self,
        payload: &RegisterEntityPayload,
    ) -> anyhow::Result<()> {
        self.inner
            .consul
            .register_entity(payload)
            .await
            .context("Failed to register entity")?;
        Ok(())
    }

    fn make_payload(ctx: &Context) -> anyhow::Result<RegisterEntityPayload> {
        let node_id = gethostname::gethostname().to_string_lossy().to_string();
        let options = ctx.config_map().get::<Options>().unwrap();
        let service_name = options.service_name.clone();
        let service_address = options.register_addr.clone();

        let service_ip = if !service_address.ip().is_unspecified() {
            service_address.ip()
        } else {
            local_ip_address::local_ip()
                .context("Failed to get local IP address")?
        };

        let local_ident = Ipv4Addr::from(local_node().ident);

        Ok(RegisterEntityPayload {
            ID: None,
            Node: node_id,
            Address: service_ip.to_string(),
            Datacenter: None,
            TaggedAddresses: Default::default(),
            NodeMeta: Default::default(),
            Service: Some(RegisterEntityService {
                ID: None,
                Service: service_name.to_string(),
                Tags: vec![],
                TaggedAddresses: Default::default(),
                Meta: (&[("ident".to_string(), local_ident.to_string())])
                    .iter()
                    .cloned()
                    .collect(),
                Port: Some(service_address.port()),
                Namespace: None,
            }),
            Checks: vec![],
            SkipNodeUpdate: None,
        })
    }
}

impl Resolver for ConsulResolver {
    async fn add_node(&self, _ident: u32, _addr: std::net::SocketAddr) {}

    async fn get_node(&self, _ident: u32) -> Option<std::net::SocketAddr> {
        todo!()
    }

    async fn visit_nodes(
        &self,
        _ident: impl Iterator<Item = u32> + Clone + Send,
        _f: impl FnMut(u32, std::net::SocketAddr) + Send,
    ) {
        todo!()
    }

    async fn visit_masked_nodes(
        &self,
        _ident: u32,
        _mask: u32,
        _f: impl FnMut(u32, std::net::SocketAddr) + Send,
    ) {
        todo!()
    }
}
