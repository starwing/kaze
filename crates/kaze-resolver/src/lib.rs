pub mod cached;
pub mod chain;
pub mod local;
pub mod resolver;

pub use options::Options as LocalOptions;
pub use resolver::Resolver;
pub use service::*;

mod options;
mod service;
