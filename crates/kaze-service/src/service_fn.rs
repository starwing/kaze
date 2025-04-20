use super::AsyncService;

pub fn async_service_fn<F, Fut, Request, Response, Error>(
    f: F,
) -> AsyncServiceFn<F>
where
    F: Fn(Request) -> Fut,
    Fut: Future<Output = Result<Response, Error>> + Send + 'static,
{
    AsyncServiceFn::new(f)
}

#[derive(Clone, Copy)]
pub struct AsyncServiceFn<F>(F);

impl<F> AsyncServiceFn<F> {
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F, Fut, Request, Response, Error> AsyncService<Request>
    for AsyncServiceFn<F>
where
    F: Fn(Request) -> Fut,
    Fut: Future<Output = Result<Response, Error>> + Send + 'static,
{
    type Response = Response;
    type Error = Error;

    fn serve(
        &self,
        req: Request,
    ) -> impl Future<Output = Result<Self::Response, Self::Error>> + Send + '_
    {
        (self.0)(req)
    }
}

pub trait FnAsyncService<Request> {
    fn into_service(self) -> AsyncServiceFn<Self>
    where
        Self: Sized,
    {
        AsyncServiceFn::new(self)
    }
}

impl<F, Fut, Request, Response, Error> FnAsyncService<Request> for F
where
    F: Fn(Request) -> Fut,
    Fut: Future<Output = Result<Response, Error>> + Send + 'static,
{
}
#[cfg(test)]
mod tests {
    use super::*;

    async fn echo_service(input: String) -> Result<String, &'static str> {
        Ok(input)
    }

    async fn error_service(_: String) -> Result<String, &'static str> {
        Err("service error")
    }

    #[tokio::test]
    async fn test_async_service_fn() {
        let service = async_service_fn(echo_service);
        let result = service.serve("hello".to_string()).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_async_service_fn_error() {
        let service = async_service_fn(error_service);
        let result = service.serve("hello".to_string()).await;
        assert_eq!(result.unwrap_err(), "service error");
    }

    #[tokio::test]
    async fn test_into_service() {
        let service = echo_service.into_service();
        let result = service.serve("hello".to_string()).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_service_clone() {
        let service = async_service_fn(echo_service);
        let service_clone = service.clone();

        let result = service.serve("hello".to_string()).await;
        let result_clone =
            service_clone.serve("hello clone".to_string()).await;

        assert_eq!(result.unwrap(), "hello");
        assert_eq!(result_clone.unwrap(), "hello clone");
    }
}
