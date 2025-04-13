use std::future::Future;
use std::task::Poll;
use std::{pin::Pin, sync::Arc};

use futures::{Sink, SinkExt};
use pin_project::pin_project;
use tower::Service;

/// A `Service` that sends messages to a `Sink`.
///
/// This is a wrapper around a `Sink` that implements the `Service` trait.
#[derive(Debug)]
pub struct SinkService<S, Item> {
    inner: Arc<std::sync::Mutex<S>>,
    _marker: std::marker::PhantomData<Item>,
}

impl<S, Item> Clone for SinkService<S, Item> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _marker: self._marker,
        }
    }
}

impl<S, Item> SinkService<S, Item> {
    /// Creates a new `SinkService` from a `Sink`.
    pub fn new(sink: S) -> Self {
        SinkService {
            inner: Arc::new(std::sync::Mutex::new(sink)),
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns the inner `Sink`.
    pub fn into_inner(self) -> Option<S> {
        Arc::try_unwrap(self.inner)
            .ok()
            .and_then(|inner| inner.into_inner().ok())
    }
}

impl<S, Item> Service<Item> for SinkService<S, Item>
where
    Item: Send,
    S: Sink<Item, Error: Into<anyhow::Error> + Send + Sync + 'static>
        + Sync
        + Unpin,
{
    type Response = ();
    type Error = anyhow::Error;
    type Future = SendFuture<S, Item>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock state: {e}"))?;
        match inner.poll_ready_unpin(cx) {
            Poll::Ready(res) => Poll::Ready(res.map_err(Into::into)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn call(&mut self, item: Item) -> Self::Future {
        SendFuture::new(self.inner.clone(), item)
    }
}

#[pin_project]
pub struct SendFuture<S: Sink<Item>, Item> {
    #[pin]
    inner: Arc<std::sync::Mutex<S>>,
    item: Option<Item>,
}

impl<S: Sink<Item>, Item> SendFuture<S, Item> {
    fn new(inner: Arc<std::sync::Mutex<S>>, item: Item) -> Self {
        Self {
            inner,
            item: Some(item),
        }
    }
}

impl<S, Item> Future for SendFuture<S, Item>
where
    Item: Send,
    S: Sink<Item, Error: Into<anyhow::Error> + Send + Sync + 'static>
        + Sync
        + Unpin,
{
    type Output = Result<(), anyhow::Error>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        let proj = self.as_mut().project();
        let mut guard = proj
            .inner
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock state: {e}"))?;
        if let Some(item) = proj.item.take() {
            if let Err(err) = guard.start_send_unpin(item) {
                return Poll::Ready(Err(err.into()));
            }
        }
        return guard.poll_flush_unpin(cx).map_err(Into::into);
    }
}

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use futures_test::{sink::SinkTestExt, task::noop_context};
    use kaze_protocol::{
        codec::NetPacketCodec,
        packet::{Packet, new_bytes_pool},
        proto::Hdr,
    };
    use tokio_util::codec::FramedWrite;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn test_sink_service() {
        let sink = vec![];
        let mut service = SinkService::new(sink);
        let res = service.ready().await.unwrap().call(1).await;
        assert!(res.is_ok());
        let res = service.ready().await.unwrap().call(2).await;
        assert!(res.is_ok());
        let res = service.ready().await.unwrap().call(3).await;
        assert!(res.is_ok());
        assert_eq!(service.into_inner().unwrap(), vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_sink_service_backpressure() {
        let sink = Vec::<u32>::new().interleave_pending_sink();
        let mut service = SinkService::new(sink);
        let mut cx = noop_context();
        assert!(service.poll_ready(&mut cx).is_pending());
        assert!(service.ready().await.unwrap().call(1).await.is_ok());
        assert!(service.poll_ready(&mut cx).is_pending());
        assert!(service.ready().await.unwrap().call(2).await.is_ok());
        assert!(service.poll_ready(&mut cx).is_pending());
        assert!(service.ready().await.unwrap().call(3).await.is_ok());
        assert!(service.poll_ready(&mut cx).is_pending());
        assert_eq!(service.into_inner().unwrap().into_inner(), vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_framed_backpressure() {
        let (tx, _rx) = tokio::io::duplex(1024);
        let mut sink = SinkService::new(FramedWrite::new(
            tx,
            NetPacketCodec::new(new_bytes_pool()),
        ));

        // fill the buffer to full
        let mut cx = noop_context();
        loop {
            let r = Service::poll_ready(&mut sink, &mut cx);
            // framed alway return Ready in poll_ready, unless the pending buffer is full
            assert!(r.is_ready());

            let packet = Packet::from_hdr(Hdr::default());
            match sink.call(packet).poll_unpin(&mut cx) {
                Poll::Ready(res) => {
                    assert!(res.is_ok());
                }
                Poll::Pending => {
                    // and when the deplex is full, call returns Pending because
                    // `poll_flush` can not complete
                    break;
                }
            }
        }

        // sink still return ready when the deplex is full
        assert!(sink.poll_ready(&mut cx).is_ready());
        // but flush call returns pending
        assert!(
            sink.into_inner()
                .unwrap()
                .poll_flush_unpin(&mut cx)
                .is_pending()
        );
    }
}
