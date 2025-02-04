use std::{future::Future, task::ready};

use tower::Service;

pin_project_lite::pin_project! {
    pub struct ReadyCall<'a, S, T>
    where
        S: Service<T>,
    {
        service: &'a mut S,
        req: Option<T>,
        #[pin]
        fut: Option<S::Future>,
    }
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
