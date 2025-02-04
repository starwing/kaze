use std::{
    future::Future,
    task::{ready, Poll},
};

use tower::Service;

pub struct Chain<First, Second> {
    first: First,
    second: Second,
}

impl<First, Second> Clone for Chain<First, Second>
where
    First: Clone,
    Second: Clone,
{
    fn clone(&self) -> Self {
        Self {
            first: self.first.clone(),
            second: self.second.clone(),
        }
    }
}

impl<First, Second> Copy for Chain<First, Second>
where
    First: Copy,
    Second: Copy,
{
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
    <First as Service<T>>::Error: Into<anyhow::Error>,
    <Second as Service<First::Response>>::Error: Into<anyhow::Error>,
{
    type Response = Second::Response;
    type Error = anyhow::Error;
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

pin_project_lite::pin_project! {
    #[project = ChainFutureProj]
    pub enum ChainFuture<Fut, Second, T, E> where
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
}

impl<Fut, Second, T, E> ChainFuture<Fut, Second, T, E>
where
    Fut: Future<Output = Result<T, E>>,
    Second: Service<T>,
{
    pub fn new(fut: Fut, outer: Second) -> Self {
        Self::WaitingInner { fut, second: outer }
    }
}

impl<Fut, Second, T, E> Future for ChainFuture<Fut, Second, T, E>
where
    Fut: Future<Output = Result<T, E>>,
    Second: Service<T>,
    <Second as Service<T>>::Error: Into<anyhow::Error>,
    E: Into<anyhow::Error>,
{
    type Output = anyhow::Result<Second::Response>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        loop {
            match self.as_mut().project() {
                ChainFutureProj::WaitingInner { fut, second: outer } => {
                    let res = ready!(fut.poll(cx)).map_err(Into::into);
                    if let Err(err) = res {
                        return Poll::Ready(Err(err.into()));
                    }
                    let fut = outer.call(res.unwrap());
                    self.set(ChainFuture::WaitingOuter { fut });
                }
                ChainFutureProj::WaitingOuter { fut } => {
                    return fut.poll(cx).map_err(Into::into)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ServiceExt as _;
    use tower::{service_fn, ServiceExt};

    #[tokio::test]
    async fn test_chain_service() {
        let svr1 = service_fn(|a: u32| async move {
            Ok::<_, anyhow::Error>(a as u64 + 1)
        });
        let svr2 = service_fn(|a: u64| async move {
            Ok::<_, anyhow::Error>(a as u32 + 2)
        });
        let fut = svr1.chain(svr2).oneshot(1);
        assert_eq!(fut.await.unwrap(), 4);
    }
}
