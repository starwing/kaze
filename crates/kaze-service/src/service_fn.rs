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
