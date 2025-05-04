mod clap_default;
mod context;
mod local;
mod wrapper;

use std::{
    any::Any,
    sync::{Arc, OnceLock},
};

use kaze_protocol::message::PacketWithAddr;
use kaze_util::tower_ext::CellService;
use tower::util::BoxCloneSyncService;

pub use anyhow;
pub use clap;
pub use serde;
pub use tokio_graceful;

pub use clap_default::ClapDefault;
pub use context::*;
pub use local::*;
pub use wrapper::*;

pub use kaze_protocol as protocol;
pub use kaze_service as service;
pub use kaze_util as util;

pub type PipelineService =
    BoxCloneSyncService<PacketWithAddr, (), anyhow::Error>;

pub type PipelineCell = CellService<PipelineService>;

pub type PluginRunFuture =
    futures::future::BoxFuture<'static, anyhow::Result<()>>;

/// a trait that inits the plugin, and provides a context to the plugin.
pub trait Plugin: AnyClone + Send + Sync + 'static {
    /// Get the name of the plugin
    #[inline]
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Initialize the plugin with the context.
    #[inline]
    fn init(&self, ctx: Context) {
        if let Some(storage) = self.context_storage() {
            storage.set(ctx).expect("Context already initialized");
        }
    }

    /// Get the storage for the context in the plugin.
    #[inline]
    fn context_storage(&self) -> Option<&OnceLock<Context>> {
        None
    }

    /// Convenience method to get the context from the storage.
    #[inline]
    fn context(&self) -> &Context {
        self.context_storage()
            .and_then(|s| s.get())
            .expect("Context not initialized for Plugin")
    }

    /// Get the main logic of the plugin, if exists.
    #[inline]
    fn run(&self) -> Option<PluginRunFuture> {
        None
    }
}

pub trait PluginFactory: Send + Sync + 'static {
    type Plugin: Plugin + Clone;

    fn build(self) -> anyhow::Result<Self::Plugin>;
}

pub trait ArcPlugin: Send + Sync + 'static {
    /// Get the storage for the context in the plugin.
    #[inline]
    fn context_storage(self: &Arc<Self>) -> Option<&OnceLock<Context>> {
        None
    }
}

impl<T> Plugin for Arc<T>
where
    T: 'static + ArcPlugin,
{
    #[inline]
    fn context_storage(&self) -> Option<&OnceLock<Context>> {
        self.context_storage()
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
    #[inline]
    fn clone_box(&self) -> Box<dyn Plugin> {
        Box::new(self.clone())
    }

    #[inline]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    #[inline]
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl Clone for Box<dyn Plugin> {
    #[inline]
    fn clone(&self) -> Self {
        (**self).clone_box()
    }
}
