use std::future::{ready, Ready};

use kaze_util::tower_ext::Chain;
use tower::Service;
use tracing::info;

use crate::message::{Message, PacketWithAddr};

/// A service that use message as request
pub trait MessageService<R>:
    Service<Message, Response = R, Error = anyhow::Error, Future: Send>
    + Send
    + Sync
    + Clone
{
}

impl<T, R> MessageService<R> for T where
    T: Service<Message, Response = R, Error = anyhow::Error, Future: Send>
        + Send
        + Sync
        + Clone
{
}

/// A service that convert packet to message
#[derive(Debug, Clone, Copy)]
pub struct ToMessageService {}

impl ToMessageService {
    pub fn new() -> Self {
        Self {}
    }
}

impl tower::Service<PacketWithAddr> for ToMessageService {
    type Response = Message;
    type Error = anyhow::Error;
    type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, req: PacketWithAddr) -> Self::Future {
        std::future::ready(Ok(req.into()))
    }
}

impl<S> tower::Layer<S> for ToMessageService {
    type Service = Chain<ToMessageService, S>;
    fn layer(&self, inner: S) -> Chain<ToMessageService, S> {
        Chain::new(self.clone(), inner)
    }
}

/// A service that drops messages and log it.
#[derive(Debug, Clone, Copy)]
pub struct SinkMessage {}

impl SinkMessage {
    pub fn new() -> Self {
        Self {}
    }
}

impl Service<Message> for SinkMessage {
    type Response = ();
    type Error = anyhow::Error;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, req: Message) -> Self::Future {
        info!("message dropped: {:?}", req);
        ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn test_to_message_service() {
        let svc = ToMessageService::new();
        svc.boxed();
    }
}
