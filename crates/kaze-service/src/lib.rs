mod adaptor;
mod combiner;
mod reusable_box;
mod service_fn;
mod util;

use std::sync::Arc;

pub use adaptor::*;
pub use combiner::*;
pub use service_fn::*;
pub use util::*;

pub trait AsyncService<Request> {
    type Response;
    type Error;

    fn serve(
        &self,
        req: Request,
    ) -> impl Future<Output = Result<Self::Response, Self::Error>> + Send + '_;
}

pub trait OwnedAsyncService<Request> {
    type Response;
    type Error;

    fn serve(
        self: Arc<Self>,
        req: Request,
    ) -> impl Future<Output = Result<Self::Response, Self::Error>> + Send + 'static;
}

impl<S, Request> AsyncService<Request> for std::sync::Arc<S>
where
    S: OwnedAsyncService<Request>,
{
    type Response = S::Response;
    type Error = S::Error;

    #[inline]
    fn serve(
        &self,
        req: Request,
    ) -> impl Future<Output = Result<Self::Response, Self::Error>> + Send + '_
    {
        self.clone().serve(req)
    }
}

impl<S, Request> AsyncService<Request> for &'static S
where
    S: AsyncService<Request>,
{
    type Response = S::Response;
    type Error = S::Error;

    #[inline]
    fn serve(
        &self,
        req: Request,
    ) -> impl Future<Output = Result<Self::Response, Self::Error>> + Send + '_
    {
        (**self).serve(req)
    }
}

impl<S, Request> AsyncService<Request> for Box<S>
where
    S: AsyncService<Request>,
{
    type Response = S::Response;
    type Error = S::Error;

    #[inline]
    fn serve(
        &self,
        req: Request,
    ) -> impl Future<Output = Result<Self::Response, Self::Error>> + Send + '_
    {
        self.as_ref().serve(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fn() {
        let svc =
            |req: String| async move { Ok::<_, anyhow::Error>(req.len()) };
        let result = svc.into_service().serve("test".to_string()).await;
        assert_eq!(result.unwrap(), 4);
        let result = async_service_fn(svc).serve("test".to_string()).await;
        assert_eq!(result.unwrap(), 4);
    }
}
