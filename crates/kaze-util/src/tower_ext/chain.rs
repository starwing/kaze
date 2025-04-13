use futures::ready;
use pin_project::pin_project;
use tower::{Layer, Service};

use super::OptionService;

#[derive(Clone, Copy)]
pub struct ChainLayer<S> {
    service: S,
}

impl<S> ChainLayer<S> {
    pub fn new(service: S) -> Self {
        Self { service }
    }
}

impl<S> ChainLayer<OptionService<S>> {
    pub fn optional(service: Option<S>) -> Self {
        Self {
            service: OptionService::new(service),
        }
    }
}

impl<Inner: Clone, Outer> Layer<Outer> for ChainLayer<Inner> {
    type Service = Chain<Inner, Outer>;

    fn layer(&self, outer: Outer) -> Self::Service {
        Chain::new(self.service.clone(), outer)
    }
}

#[derive(Clone, Copy)]
pub struct Chain<First, Second> {
    first: First,
    second: Second,
}

impl<First, Second> Chain<First, Second> {
    pub fn new(first: First, second: Second) -> Self {
        Self { first, second }
    }
}

impl<T, First, Second> Service<T> for Chain<First, Second>
where
    First: Service<T>,
    Second: Service<First::Response> + Clone,
    Second::Error: From<First::Error> + std::fmt::Debug,
{
    type Response = Second::Response;
    type Error = Second::Error;
    type Future =
        ChainFuture<First::Future, Second, First::Response, First::Error>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        if let Err(err) = ready!(self.first.poll_ready(cx)) {
            return Err(err.into()).into();
        }
        if let Err(err) = ready!(self.second.poll_ready(cx)) {
            return Err(err.into()).into();
        }
        Ok(()).into()
    }

    fn call(&mut self, req: T) -> Self::Future {
        let fut = self.first.call(req);
        let clone = self.second.clone();
        let second = std::mem::replace(&mut self.second, clone);
        ChainFuture::new(fut, second)
    }
}

#[pin_project(project = ChainFutureProj)]
pub enum ChainFuture<Fut, Second, T, E>
where
    Fut: Future<Output = Result<T, E>>,
    Second: Service<T>,
{
    WaitingInner {
        #[pin]
        fut: Fut,
        second: Second,
    },
    WaitingOuter {
        #[pin]
        fut: Second::Future,
    },
}

impl<Fut, Second, T, E> ChainFuture<Fut, Second, T, E>
where
    Fut: Future<Output = Result<T, E>>,
    Second: Service<T>,
    Second::Error: From<E> + std::fmt::Debug,
{
    pub fn new(fut: Fut, outer: Second) -> Self {
        Self::WaitingInner { fut, second: outer }
    }
}

impl<Fut, Second, T, E> Future for ChainFuture<Fut, Second, T, E>
where
    Fut: Future<Output = Result<T, E>>,
    Second: Service<T>,
    Second::Error: From<E> + std::fmt::Debug,
{
    type Output = Result<Second::Response, Second::Error>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        use std::task::Poll;
        loop {
            match self.as_mut().project() {
                ChainFutureProj::WaitingInner { fut, second: outer } => {
                    let res: Result<_, Second::Error> =
                        ready!(fut.poll(cx)).map_err(Into::into);
                    if let Err(err) = res {
                        return Poll::Ready(Err(err.into()));
                    }
                    let fut = outer.call(res.unwrap());
                    self.set(ChainFuture::WaitingOuter { fut });
                }
                ChainFutureProj::WaitingOuter { fut } => {
                    return fut.poll(cx).map_err(Into::into);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ServiceExt;
    use super::*;
    use tower::{ServiceExt as _, service_fn};

    #[tokio::test]
    async fn test_chain_layer() {
        let svc1 = service_fn(async move |a: u32| ok(a as u64 + 1));
        let svc2 = service_fn(async move |a: u64| ok(a as u8 + 2));
        let layer = ChainLayer::new(svc1);
        let svc = layer.layer(svc2);
        let r: u8 = svc.oneshot(1).await.unwrap();
        assert_eq!(r, 4);
    }

    #[tokio::test]
    async fn test_chain_service() {
        let svc1 = service_fn(|a: u32| async move { ok(a as u64 + 1) });
        let svc2 = service_fn(|a: u64| async move { ok(a as u32 + 2) });
        let fut = svc1.chain(svc2).oneshot(1);
        assert_eq!(fut.await.unwrap(), 4);
    }

    fn ok<T>(t: T) -> Result<T, anyhow::Error> {
        Ok(t)
    }
}
