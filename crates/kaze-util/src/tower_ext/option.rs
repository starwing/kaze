use std::pin::Pin;

use pin_project::pin_project;
use tower::Service;

#[derive(Clone)]
pub struct OptionService<S> {
    service: Option<S>,
}

impl<S> OptionService<S> {
    pub fn new(service: Option<S>) -> Self {
        Self { service }
    }
}

impl<S, R> Service<R> for OptionService<S>
where
    S: Service<R, Response = R>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = OptionFuture<R, S::Future>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        if let Some(service) = self.service.as_mut() {
            return service.poll_ready(cx);
        }
        Ok(()).into()
    }

    fn call(&mut self, req: R) -> Self::Future {
        if let Some(service) = self.service.as_mut() {
            return OptionFuture::new(service.call(req));
        }
        OptionFuture::none(req)
    }
}

#[pin_project(project = OptionFutureProj)]
pub enum OptionFuture<R, Future> {
    Future(#[pin] Future),
    None(Option<R>),
}

impl<R, F> OptionFuture<R, F> {
    pub fn new(future: F) -> Self {
        Self::Future(future)
    }

    pub fn none(req: R) -> Self {
        Self::None(Some(req))
    }
}

impl<R, F, E> Future for OptionFuture<R, F>
where
    F: Future<Output = Result<R, E>>,
{
    type Output = F::Output;
    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.project() {
            OptionFutureProj::Future(future) => future.poll(cx),
            OptionFutureProj::None(req) => {
                std::task::Poll::Ready(Ok(req.take().unwrap()))
            }
        }
    }
}
