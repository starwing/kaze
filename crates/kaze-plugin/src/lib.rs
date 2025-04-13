mod clap_default;
mod local;

use std::sync::Arc;

use tower::util::BoxCloneSyncService;

use kaze_protocol::message::PacketWithAddr;
use kaze_util::tower_ext::CellService;

pub use anyhow;
pub use clap;
pub use serde;

pub use kaze_protocol as protocol;
pub use kaze_util as util;

pub use clap_default::ClapDefault;
pub use local::*;

pub type PipelineService =
    BoxCloneSyncService<PacketWithAddr, (), anyhow::Error>;

pub type PipelineCell = CellService<PipelineService>;

/// a trait that require a pipeline service, implemented by all plugins that
/// need a pipeline service. These plugins can contain a PipelineCell itself,
/// and implement this trait. the real pipeline service will be filled in before
/// sidecar is started.
pub trait PipelineRequired {
    fn sink(&self) -> &PipelineCell;
}

impl<T: PipelineRequired> PipelineRequired for Arc<T> {
    fn sink(&self) -> &PipelineCell {
        self.as_ref().sink()
    }
}
