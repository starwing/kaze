mod chain_layer;
mod chain_service;
mod ready_call;
mod service_cell;
mod sink_service;

pub use chain_layer::ChainLayer;
pub use chain_service::Chain;
pub use ready_call::ReadyCall;
pub use sink_service::SinkService;
pub use service_cell::ServiceCell;

use tower::Service;

pub trait ServiceExt<T>: Service<T> {
    fn chain<S>(self, outer: S) -> Chain<Self, S>
    where
        Self: Sized,
    {
        Chain::new(self, outer)
    }

    fn ready_call(&mut self, req: T) -> ReadyCall<Self, T>
    where
        Self: Sized,
    {
        ReadyCall::new(self, req)
    }
}

impl<T, R> ServiceExt<R> for T where T: Service<R> {}
