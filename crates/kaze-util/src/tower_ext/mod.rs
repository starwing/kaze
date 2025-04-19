mod cell;
mod chain;
mod option;
mod sink;

pub mod case;
pub mod filter;

pub use case::Case;
pub use cell::CellService;
pub use chain::{Chain, ChainLayer};
pub use filter::{Filter, FilterChain};
pub use option::OptionService;
pub use sink::SinkService;

use futures::ready;
use pin_project::pin_project;
use tower::Service;

pub trait ServiceExt<T>: Service<T> {
    fn chain<S>(self, outer: S) -> Chain<Self, S>
    where
        Self: Sized,
    {
        Chain::new(self, outer)
    }

    fn filter<S>(self, outer: S) -> Filter<Self, S>
    where
        Self: Sized,
    {
        Filter::new(self, outer)
    }

    fn ready_call(&mut self, req: T) -> ReadyCall<Self, T>
    where
        Self: Sized,
    {
        ReadyCall::new(self, req)
    }

    fn to_filter(self) -> ToFilter<Self>
    where
        Self: Sized,
    {
        ToFilter::new(self)
    }
}

impl<T, R> ServiceExt<R> for T where T: Service<R> {}

#[pin_project]
pub struct ReadyCall<'a, S, T>
where
    S: Service<T>,
{
    service: &'a mut S,
    req: Option<T>,
    #[pin]
    fut: Option<S::Future>,
}

impl<'a, S, T> ReadyCall<'a, S, T>
where
    S: Service<T>,
{
    pub fn new(service: &'a mut S, req: T) -> Self {
        Self {
            service,
            req: Some(req),
            fut: None,
        }
    }
}

impl<'a, S, T> Future for ReadyCall<'a, S, T>
where
    S: Service<T>,
{
    type Output = Result<S::Response, S::Error>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        loop {
            let mut proj = self.as_mut().project();
            if let Some(fut) = proj.fut.as_mut().as_pin_mut() {
                return ready!(fut.poll(cx)).into();
            }
            if let Err(e) = ready!(proj.service.poll_ready(cx)) {
                return Err(e).into();
            }
            let req = proj.req.take().expect("req is none");
            let fut = proj.service.call(req);
            proj.fut.set(Some(fut));
        }
    }
}

#[derive(Clone, Copy)]
pub struct ToFilter<S> {
    svc: S,
}

impl<S> ToFilter<S> {
    pub fn new(svc: S) -> Self {
        Self { svc }
    }
}

impl<S, T> Service<T> for ToFilter<S>
where
    S: Service<T>,
{
    type Response = Option<S::Response>;
    type Error = S::Error;
    type Future = ToFilterFuture<S::Future>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.svc.poll_ready(cx)
    }

    fn call(&mut self, req: T) -> Self::Future {
        ToFilterFuture {
            fut: self.svc.call(req),
        }
    }
}

#[pin_project]
pub struct ToFilterFuture<F> {
    #[pin]
    fut: F,
}

impl<F, R, E> Future for ToFilterFuture<F>
where
    F: Future<Output = Result<R, E>>,
{
    type Output = Result<Option<R>, E>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        use std::task::Poll;
        match self.project().fut.poll(cx) {
            Poll::Ready(Ok(res)) => Poll::Ready(Ok(Some(res))),
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }
}
