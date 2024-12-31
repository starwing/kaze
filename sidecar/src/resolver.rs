use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use moka::future::Cache;
use tokio::sync::Mutex;

use crate::config::NodeConfig;

/// resolve ident to node address
pub struct Resolver {
    node_map: Mutex<HashMap<u32, SocketAddr>>,
    mask_cache: Cache<(u32, u32), Arc<Vec<(u32, SocketAddr)>>>,
    consul: Option<consul::Client>,
}

impl Resolver {
    /// create a new resolver
    pub fn new(cache_capactiy: usize, live_time: impl Into<Duration>) -> Self {
        Self {
            node_map: Mutex::new(HashMap::new()),
            mask_cache: Cache::builder()
                .name("resolver-cache")
                .max_capacity(cache_capactiy as u64)
                .weigher(|_, v: &Arc<Vec<(u32, SocketAddr)>>| v.len() as u32)
                .time_to_live(live_time.into())
                .build(),
            consul: None,
        }
    }

    /// setup consul client
    pub async fn setup_consul(
        &mut self,
        consul: String,
        token: Option<String>,
    ) -> Result<()> {
        let mut config = consul::Config::new()
            .map_err(|e| anyhow!("Error from consul: {}", e))
            .context("Failed to create consul config")?;
        config.address = consul;
        config.token = token;
        self.consul = Some(consul::Client::new(config));
        Ok(())
    }

    pub async fn setup_local(
        &mut self,
        conf: impl Iterator<Item = &NodeConfig>,
    ) {
        for node in conf {
            self.add_node(node.ident.to_bits(), node.addr).await;
        }
    }

    /// add a ident->node mapping
    pub async fn add_node(&self, ident: u32, addr: SocketAddr) {
        self.node_map.lock().await.insert(ident, addr);
    }

    /// get node address by ident
    pub async fn get_node(&self, ident: u32) -> Option<SocketAddr> {
        self.node_map.lock().await.get(&ident).cloned()
    }

    /// get node address list by ident and mask
    ///
    /// Returns all nodes that match `(ident & mask)`
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn get_mask_nodes(
        &self,
        ident: u32,
        mask: u32,
    ) -> Arc<Vec<(u32, SocketAddr)>> {
        self.mask_cache
            .get_with((ident, mask), self.calc_mask_nodes(ident, mask))
            .await
    }

    /// calculate all nodes that match `(ident & mask)`
    async fn calc_mask_nodes(
        &self,
        ident: u32,
        mask: u32,
    ) -> Arc<Vec<(u32, SocketAddr)>> {
        Arc::new(
            self.node_map
                .lock()
                .await
                .iter()
                .filter(|(&e, _)| e & mask == ident)
                .map(|(&k, &v)| (k, v))
                .collect(),
        )
    }
}
