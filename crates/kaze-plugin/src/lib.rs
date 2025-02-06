use tower::util::BoxCloneSyncService;

use kaze_protocol::message::PacketWithAddr;
use kaze_util::tower_ext::ServiceCell;

pub use clap;
pub use clap_merge;
pub use serde;

pub use kaze_protocol as protocol;
pub use kaze_util as util;

pub type PipelineService =
    BoxCloneSyncService<PacketWithAddr, (), anyhow::Error>;

pub type PipelineCell = ServiceCell<PipelineService>;
