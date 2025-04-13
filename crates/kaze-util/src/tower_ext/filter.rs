use std::convert::Infallible;

use futures::ready;
use pin_project::pin_project;
use tower::{Layer, Service};

#[derive(Clone, Copy)]
pub struct FilterChain<F> {
    filter: F,
}

impl FilterChain<Identity> {
    pub fn new() -> Self {
        Self { filter: Identity }
    }
}

impl<F> FilterChain<F> {
    pub fn filter<T>(self, filter: T) -> FilterChain<Stack<F, T>> {
        FilterChain {
            filter: Stack::new(self.filter, filter),
        }
    }

    pub fn service(self) -> F {
        self.filter
    }
}

impl<F: Clone, S> Layer<S> for FilterChain<F> {
    type Service = Filter<F, S>;

    fn layer(&self, service: S) -> Self::Service {
        Filter {
            filter: self.filter.clone(),
            service,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Filter<F, S> {
    filter: F,
    service: S,
}

impl<F, S> Filter<F, S> {
    pub fn new(filter: F, service: S) -> Self {
        Self { filter, service }
    }
}

impl<T, M, F, S> Service<T> for Filter<F, S>
where
    F: Service<T, Response = Option<M>>,
    S: Service<M> + Clone,
    S::Error: From<F::Error>,
{
    type Response = Option<S::Response>;
    type Error = S::Error;
    type Future = FilterServiceFuture<F::Future, S, S::Future>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        ready!(self.filter.poll_ready(cx)).map_err(Into::into)?;
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: T) -> Self::Future {
        let filter_fut = self.filter.call(req);
        FilterServiceFuture {
            filter_fut,
            service: self.service.clone(),
            service_fut: None,
        }
    }
}

#[pin_project]
pub struct FilterServiceFuture<F, S, SF> {
    #[pin]
    filter_fut: F,
    service: S,
    #[pin]
    service_fut: Option<SF>,
}

impl<F, S, T, R, FE> Future for FilterServiceFuture<F, S, S::Future>
where
    F: Future<Output = Result<Option<T>, FE>>,
    S: Service<T, Response = R> + Clone,
    S::Error: From<FE>,
{
    type Output = Result<Option<R>, S::Error>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        use std::task::Poll;

        let proj = self.as_mut().project();
        if let Some(sf) = proj.service_fut.as_pin_mut() {
            return Poll::Ready(ready!(sf.poll(cx)).map(Some));
        }
        match ready!(proj.filter_fut.poll(cx)) {
            Ok(Some(req)) => {
                let mut proj = self.as_mut().project();
                let fut = proj.service.call(req);
                proj.service_fut.set(Some(fut));
                Poll::Ready(
                    ready!(proj.service_fut.as_pin_mut().unwrap().poll(cx))
                        .map(Some),
                )
            }
            Ok(None) => Poll::Ready(Ok(None)),
            Err(err) => Poll::Ready(Err(err.into())),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Stack<F1, F2> {
    filter1: F1,
    filter2: F2,
}

impl<F1, F2> Stack<F1, F2> {
    pub fn new(filter1: F1, filter2: F2) -> Self {
        Self { filter1, filter2 }
    }
}

impl<T, M, R, F1, F2> Service<T> for Stack<F1, F2>
where
    F1: Service<T, Response = Option<M>>,
    F2: Service<M, Response = Option<R>> + Clone,
    F2::Error: From<F1::Error>,
{
    type Response = Option<R>;
    type Error = F2::Error;
    type Future = StackFuture<F1::Future, F2, F2::Future>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        ready!(self.filter1.poll_ready(cx)).map_err(Into::into)?;
        self.filter2.poll_ready(cx)
    }

    fn call(&mut self, req: T) -> Self::Future {
        let filter1_fut = self.filter1.call(req);
        StackFuture {
            filter1_fut,
            filter2_fut: None,
            filter2: self.filter2.clone(),
        }
    }
}

#[pin_project]
pub struct StackFuture<F1, F2, F2F> {
    #[pin]
    filter1_fut: F1,
    filter2: F2,
    #[pin]
    filter2_fut: Option<F2F>,
}

impl<T, R, F1, F2, F1E> Future for StackFuture<F1, F2, F2::Future>
where
    F1: Future<Output = Result<Option<T>, F1E>>,
    F2: Service<T, Response = Option<R>>,
    F2::Error: From<F1E>,
{
    type Output = Result<F2::Response, F2::Error>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        use std::task::Poll;

        let proj = self.as_mut().project();
        if let Some(sf) = proj.filter2_fut.as_pin_mut() {
            return Poll::Ready(ready!(sf.poll(cx)));
        }
        match ready!(proj.filter1_fut.poll(cx)) {
            Ok(Some(req)) => {
                let mut proj = self.as_mut().project();
                let fut = proj.filter2.call(req);
                proj.filter2_fut.set(Some(fut));
                Poll::Ready(ready!(
                    proj.filter2_fut.as_pin_mut().unwrap().poll(cx)
                ))
            }
            Ok(None) => Poll::Ready(Ok(None)),
            Err(err) => Poll::Ready(Err(err.into())),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Identity;

impl<T> Service<T> for Identity {
    type Response = Option<T>;
    type Error = Infallible;
    type Future = futures::future::Ready<Result<Option<T>, Infallible>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: T) -> Self::Future {
        futures::future::ready(Ok(Some(req)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceBuilder;
    use tower::ServiceExt as _;
    use tower::service_fn;
    use tower::util::ServiceFn;

    fn ok<T>(t: T) -> Result<T, anyhow::Error> {
        Ok(t)
    }

    #[tokio::test]
    async fn test_filter_chain() {
        let u32_u64_add_1 =
            service_fn(async move |a: u32| ok(Some(a as u64 + 1)));
        let u64_i64_filter_odd = service_fn(async move |a: u64| {
            ok(if a % 2 == 1 { Some(a as i64) } else { None })
        });
        let i64_add_2 = service_fn(async move |a: i64| ok(a as u8 + 2));

        assert_eq!(Identity.oneshot(1).await.unwrap(), Some(1));
        assert_eq!(
            FilterChain::new()
                .filter(u32_u64_add_1)
                .service()
                .oneshot(1)
                .await
                .unwrap(),
            Some(2)
        );

        let chain = FilterChain::new()
            .filter(u32_u64_add_1)
            .filter(u64_i64_filter_odd);
        assert_eq!(chain.layer(i64_add_2).oneshot(1).await.unwrap(), None);
        assert_eq!(chain.layer(i64_add_2).oneshot(2).await.unwrap(), Some(5));
    }

    #[tokio::test]
    async fn test_filter_layer() {
        let u32_u64_add_1 =
            service_fn(async move |a: u32| ok(Some(a as u64 + 1)));
        let u64_i64_filter_odd = service_fn(async move |a: u64| {
            ok(if a % 2 == 1 { Some(a as i64) } else { None })
        });
        let i64_add_2 = service_fn(async move |a: i64| ok(a as u8 + 2));

        let svc = ServiceBuilder::new()
            .layer(
                FilterChain::new()
                    .filter(u32_u64_add_1)
                    .filter(u64_i64_filter_odd),
            )
            .service(i64_add_2);
        assert_eq!(svc.oneshot(1).await.unwrap(), None);
        assert_eq!(svc.oneshot(2).await.unwrap(), Some(5));
    }

    fn noncopy_service_fn<F>(f: F) -> NonCopyServiceFn<F> {
        NonCopyServiceFn::new(f)
    }
    #[derive(Clone)]
    struct NonCopyServiceFn<F> {
        svc: ServiceFn<F>,
    }
    impl<F> NonCopyServiceFn<F> {
        fn new(f: F) -> Self {
            Self { svc: service_fn(f) }
        }
    }
    impl<T, F, Request, R, E> Service<Request> for NonCopyServiceFn<T>
    where
        T: FnMut(Request) -> F,
        F: Future<Output = Result<R, E>>,
    {
        type Response = R;
        type Error = E;
        type Future = F;

        fn poll_ready(
            &mut self,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            self.svc.poll_ready(cx)
        }

        fn call(&mut self, req: Request) -> Self::Future {
            self.svc.call(req)
        }
    }

    #[tokio::test]
    async fn test_noncopy() {
        let u32_u64_add_1 =
            noncopy_service_fn(async move |a: u32| ok(Some(a as u64 + 1)));
        let u64_i64_filter_odd = noncopy_service_fn(async move |a: u64| {
            ok(if a % 2 == 1 { Some(a as i64) } else { None })
        });
        let i64_add_2 =
            noncopy_service_fn(async move |a: i64| ok(a as u8 + 2));

        let svc = ServiceBuilder::new()
            .layer(
                FilterChain::new()
                    .filter(u32_u64_add_1)
                    .filter(u64_i64_filter_odd),
            )
            .service(i64_add_2);
        assert_eq!(svc.clone().oneshot(1).await.unwrap(), None);
        assert_eq!(svc.oneshot(2).await.unwrap(), Some(5));
    }
}
