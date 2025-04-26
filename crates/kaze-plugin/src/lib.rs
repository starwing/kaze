mod clap_default;
mod context;
mod local;
mod wrapper;

use std::{any::Any, pin::Pin, sync::Arc};

use tower::util::BoxCloneSyncService;

use kaze_protocol::message::PacketWithAddr;
use kaze_util::tower_ext::CellService;

pub use anyhow;
pub use clap;
pub use serde;
pub use tokio_graceful;

pub use kaze_protocol as protocol;
pub use kaze_service as service;
pub use kaze_util as util;

pub use clap_default::ClapDefault;
pub use context::*;
pub use local::*;
pub use wrapper::*;

pub type PipelineService =
    BoxCloneSyncService<PacketWithAddr, (), anyhow::Error>;

pub type PipelineCell = CellService<PipelineService>;

/// a trait that inits the plugin, and provides a context to the plugin.
pub trait Plugin: AnyClone + Send + Sync + 'static {
    fn init(&self, _ctx: Context) {}

    fn context(&self) -> &Context {
        unimplemented!("context() is not implemented for Plugin");
    }

    fn run(
        &self,
    ) -> Option<
        Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'static>>,
    > {
        None
    }
}

pub trait PluginFactory: Send + Sync + 'static {
    type Plugin: Plugin + Clone;

    fn build(self) -> anyhow::Result<Self::Plugin>;
}

pub trait ArcPlugin: Send + Sync + 'static {
    fn init(self: &Arc<Self>, context: Context);
    fn context(self: &Arc<Self>) -> &Context;
}

impl<T> Plugin for Arc<T>
where
    T: 'static + ArcPlugin,
{
    fn init(&self, context: Context) {
        self.init(context);
    }
    fn context(&self) -> &Context {
        self.context()
    }
}

pub trait AnyClone: Send + Sync + 'static {
    fn clone_box(&self) -> Box<dyn Plugin>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl<T> AnyClone for T
where
    T: 'static + Plugin + Clone,
{
    fn clone_box(&self) -> Box<dyn Plugin> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl Clone for Box<dyn Plugin> {
    fn clone(&self) -> Self {
        (**self).clone_box()
    }
}
