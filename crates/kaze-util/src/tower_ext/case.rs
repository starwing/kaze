use std::convert::Infallible;

use futures::ready;
use pin_project::pin_project;
use tower::Service;

use super::filter::{FilterError, FuncFilter};

#[derive(Clone, Copy)]
pub struct Case<S, D> {
    service: Option<S>,
    default: D,
}

impl Case<EmptyCase, Identity> {
    pub fn new() -> Self {
        Self {
            service: None,
            default: Identity,
        }
    }
}

impl<S, D> Case<S, D> {
    pub fn add<F, CS>(
        self,
        cond: F,
        service: CS,
    ) -> Case<Case<S, FuncFilter<F, CS>>, D> {
        Case {
            service: Some(Case {
                service: self.service,
                default: FuncFilter::new(cond, service),
            }),
            default: self.default,
        }
    }

    pub fn default<ND>(self, default: ND) -> Case<S, ND> {
        Case {
            service: self.service,
            default,
        }
    }
}

impl<T, R, S, D, E> Service<T> for Case<S, D>
where
    S: Service<T, Response = R, Error = FilterError<T, E>>,
    D: Service<T, Response = R> + Clone,
    D::Error: From<E>,
{
    type Response = R;
    type Error = D::Error;
    type Future = CaseFuture<T, S::Future, D>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        if let Some(service) = self.service.as_mut() {
            ready!(service.poll_ready(cx)).map_err(|e| match e {
                FilterError::Filtered(_) => {
                    unreachable!(
                        "Filtered error should not returned in poll_ready"
                    )
                }
                FilterError::Errored(err) => err,
            })?;
        }
        self.default.poll_ready(cx)
    }

    fn call(&mut self, req: T) -> Self::Future {
        let def = self.default.clone();
        let mut def = std::mem::replace(&mut self.default, def);
        if let Some(service) = self.service.as_mut() {
            CaseFuture::ServiceCall {
                future: service.call(req),
                default: Some(def),
            }
        } else {
            CaseFuture::DefaultCall {
                future: def.call(req),
            }
        }
    }
}

#[pin_project(project = CaseFutureProj)]
pub enum CaseFuture<T, SF, D>
where
    D: Service<T>,
{
    ServiceCall {
        #[pin]
        future: SF,
        default: Option<D>,
    },
    DefaultCall {
        #[pin]
        future: D::Future,
    },
    Done,
}

impl<T, SF, D, SE> Future for CaseFuture<T, SF, D>
where
    D: Service<T>,
    D::Error: From<SE>,
    SF: Future<Output = Result<D::Response, FilterError<T, SE>>>,
{
    type Output = Result<D::Response, D::Error>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        use std::task::Poll;
        match self.as_mut().project() {
            CaseFutureProj::ServiceCall { future, default } => {
                match ready!(future.poll(cx)) {
                    Ok(response) => Poll::Ready(Ok(response)),
                    Err(FilterError::Filtered(req)) => {
                        let mut service = default
                            .take()
                            .expect("default service should be available");
                        self.set(CaseFuture::DefaultCall {
                            future: service.call(req),
                        });
                        self.poll(cx) // recurse call to update self state
                    }
                    Err(FilterError::Errored(err)) => {
                        Poll::Ready(Err(err.into()))
                    }
                }
            }
            CaseFutureProj::DefaultCall { future } => future.poll(cx),
            CaseFutureProj::Done => panic!("Future polled after completion"),
        }
    }
}

#[derive(Clone, Copy)]
pub struct EmptyCase;

impl<T> Service<T> for EmptyCase {
    type Response = T;
    type Error = FilterError<T, Infallible>;
    type Future = std::future::Ready<Result<T, Self::Error>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: T) -> Self::Future {
        std::future::ready(Err(FilterError::Filtered(req)))
    }
}

#[derive(Clone, Copy)]
pub struct Identity;

impl<T> Service<T> for Identity {
    type Response = T;
    type Error = anyhow::Error;
    type Future = std::future::Ready<Result<T, Self::Error>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, req: T) -> Self::Future {
        std::future::ready(Ok(req))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_case() {
        let svc_odd = tower::service_fn(|x: i32| async move { ok(3 * x + 1) });
        let svc_even = tower::service_fn(|x: i32| async move { ok(x / 2) });

        assert!(matches!(
            EmptyCase.oneshot(1).await,
            Err(FilterError::Filtered(1))
        ));
        assert_eq!(Identity.oneshot(1).await.unwrap(), 1);

        let funcfilter = FuncFilter::new(|x: &i32| x % 2 == 0, svc_even);
        assert_eq!(funcfilter.clone().oneshot(2).await.unwrap(), 1);

        assert_eq!(Case::new().oneshot(2).await.unwrap(), 2);

        let case = Case::new().default(funcfilter);
        assert_eq!(case.oneshot(2).await.unwrap(), 1);

        let case = Case::new()
            .add(|x: &i32| x % 2 == 0, svc_even)
            .add(|x: &i32| x % 2 != 0, svc_odd);
        assert_eq!(case.oneshot(2).await.unwrap(), 1);
        assert_eq!(case.oneshot(1).await.unwrap(), 4);
    }

    fn ok<T>(t: T) -> Result<T, anyhow::Error> {
        Ok(t)
    }
}
