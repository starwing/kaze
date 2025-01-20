use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use anyhow::Result;
use tokio_util::sync::ReusableBoxFuture;

/// the sink trait for kaze edge and corral. using custom trait but not
/// `futures::Sink` because we need to `poll_ready` with Message length, that's
/// not supported by `futures::Sink`.
pub trait Sink<Item> {
    type Error;
    type Future: Future<Output = Result<(), Self::Error>>;

    fn send(&mut self, item: Item) -> Self::Future;
}

pub fn sink_fn<T, F, Item, E>(f: T) -> SinkFn<T>
where
    T: FnMut(Item) -> F,
    F: Future<Output = Result<(), E>>,
{
    SinkFn::new(f)
}

pub struct SinkFn<T> {
    f: T,
}

impl<T> SinkFn<T> {
    pub fn new(f: T) -> Self {
        SinkFn { f }
    }
}

impl<T, F, Item, E> Sink<Item> for SinkFn<T>
where
    T: FnMut(Item) -> F,
    F: Future<Output = Result<(), E>>,
{
    type Error = E;
    type Future = F;

    fn send(&mut self, item: Item) -> Self::Future {
        (self.f)(item)
    }
}

pub struct SinkWrapper<'a, Item, S: Sink<Item>> {
    sink: S,
    state: State,
    fut: ReusableBoxFuture<'a, Result<(), S::Error>>,
}

enum State {
    /// idle state, ready to accept new items
    Idle,
    /// sending state, holding a future
    Sending,
    /// closed state
    Closed,
}

impl<'a, Item, S: Sink<Item>> SinkWrapper<'a, Item, S> {
    /// create a new SinkWrapper
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            state: State::Idle,
            fut: ReusableBoxFuture::new(async { unreachable!() }),
        }
    }
}

impl<'a, Item, S> futures::Sink<Item> for SinkWrapper<'a, Item, S>
where
    S: Sink<Item> + Unpin,
    S::Future: Send + 'a,
    S::Error: Into<anyhow::Error>,
{
    type Error = anyhow::Error;

    fn poll_ready(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        loop {
            match &mut self.state {
                // return ready if in idle state
                State::Idle => return Poll::Ready(Ok(())),

                // poll the future if in sending state
                State::Sending => {
                    match self.fut.poll(cx) {
                        Poll::Ready(res) => {
                            // return to idle state when sending is finished
                            res.map_err(Into::into)?;
                            self.state = State::Idle;
                            return Poll::Ready(Ok(()));
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                }

                // return error if in closed state
                State::Closed => {
                    return Poll::Ready(Err(
                        anyhow::anyhow!("Sink is closed").into()
                    ))
                }
            }
        }
    }

    fn start_send(
        mut self: Pin<&mut Self>,
        item: Item,
    ) -> Result<(), Self::Error> {
        match self.state {
            State::Idle => {
                // create future and enter sending state
                let fut = self.sink.send(item);
                self.state = State::Sending;
                self.fut.set(fut);
                Ok(())
            }
            _ => Err(anyhow::anyhow!("Sink not ready").into()),
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        match &mut self.state {
            State::Sending => {
                // poll the future if in sending state
                match self.fut.poll(cx) {
                    Poll::Ready(res) => {
                        res.map_err(Into::into)?;
                        self.state = State::Idle;
                        Poll::Ready(Ok(()))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
            _ => Poll::Ready(Ok(())),
        }
    }

    fn poll_close(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        loop {
            match &mut self.state {
                // to close state when idle
                State::Idle => {
                    self.state = State::Closed;
                    return Poll::Ready(Ok(()));
                }

                // wait for the sending future to finish
                State::Sending => match self.fut.poll(cx) {
                    Poll::Ready(res) => {
                        self.state = State::Idle;
                        res.map_err(Into::into)?;
                    }
                    Poll::Pending => return Poll::Pending,
                },

                // return ready if in closed state
                State::Closed => return Poll::Ready(Ok(())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pin_feature() {
        use futures::SinkExt as _;
        use std::time::Duration;

        let wrapper = SinkWrapper::new(sink_fn(|_| async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok::<_, anyhow::Error>(())
        }));

        let mut pinned = Box::pin(wrapper);
        pinned.as_mut().send(42).await.unwrap();
    }

    #[tokio::test]
    async fn test_unpin_feature() {
        use futures::SinkExt as _;
        use std::time::Duration;

        let mut wrapper = SinkWrapper::new(sink_fn(|_| async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok::<_, anyhow::Error>(())
        }));

        wrapper.send(42).await.unwrap();
    }
}
