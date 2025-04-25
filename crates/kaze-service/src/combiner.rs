use super::AsyncService;

#[derive(Clone, Copy)]
pub struct ServiceLayer<S> {
    service: S,
}

impl<S> ServiceLayer<S> {
    pub fn new(service: S) -> Self {
        Self { service }
    }
}

impl<S: Clone, T> tower::Layer<T> for ServiceLayer<S> {
    type Service = Chain<S, T>;

    fn layer(&self, outer: T) -> Self::Service {
        Chain::new(self.service.clone(), outer)
    }
}

impl<Request, S> AsyncService<Request> for ServiceLayer<S>
where
    S: AsyncService<Request>,
{
    type Response = S::Response;
    type Error = S::Error;

    fn serve(
        &self,
        req: Request,
    ) -> impl Future<Output = Result<Self::Response, Self::Error>> + Send + '_
    {
        self.service.serve(req)
    }
}

#[derive(Clone, Copy)]
pub struct Chain<S, T> {
    inner: S,
    outer: T,
}

impl<S, T> Chain<S, T> {
    pub fn new(inner: S, outer: T) -> Self {
        Self { inner, outer }
    }
}

impl<Request, S, T> AsyncService<Request> for Chain<S, T>
where
    Request: Send + 'static,
    S: AsyncService<Request> + Sync,
    T: AsyncService<S::Response> + Sync,
    S::Error: Into<T::Error>,
{
    type Response = T::Response;
    type Error = T::Error;

    async fn serve(
        &self,
        req: Request,
    ) -> Result<Self::Response, Self::Error> {
        let result = self.inner.serve(req).await.map_err(Into::into)?;
        self.outer.serve(result).await
    }
}

#[derive(Clone, Copy)]
pub struct FilterLayer<F> {
    filter: F,
}

impl<F> FilterLayer<F> {
    pub fn new(filter: F) -> Self {
        Self { filter }
    }
}

impl<F: Clone, T> tower::Layer<T> for FilterLayer<F> {
    type Service = FilterChain<F, T>;

    fn layer(&self, outer: T) -> Self::Service {
        FilterChain::new(self.filter.clone(), outer)
    }
}

#[derive(Clone, Copy)]
pub struct FilterChain<S, T> {
    inner: S,
    outer: T,
}

impl<S, T> FilterChain<S, T> {
    pub fn new(inner: S, outer: T) -> Self {
        Self { inner, outer }
    }
}

impl<S, T, U> tower::Layer<U> for FilterChain<S, T>
where
    Self: Clone,
{
    type Service = FilterChain<FilterChain<S, T>, U>;

    fn layer(&self, outer: U) -> Self::Service {
        FilterChain::new(self.clone(), outer)
    }
}

impl<Request, Middle, Response, S, T> AsyncService<Request>
    for FilterChain<S, T>
where
    Request: Send + 'static,
    Middle: Send + 'static,
    Response: Send + 'static,
    S: AsyncService<Request, Response = Option<Middle>> + Sync,
    T: AsyncService<Middle, Response = Option<Response>> + Sync,
    S::Error: Into<T::Error>,
{
    type Response = T::Response;
    type Error = T::Error;

    async fn serve(
        &self,
        req: Request,
    ) -> Result<Self::Response, Self::Error> {
        let result = self.inner.serve(req).await.map_err(Into::into)?;
        if let Some(result) = result {
            self.outer.serve(result).await
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Copy)]
pub enum Either<A, B> {
    Left(A),
    Right(B),
}

impl<Request, A, B> AsyncService<Request> for Either<A, B>
where
    Request: Send + 'static,
    A: AsyncService<Request> + Sync,
    B: AsyncService<Request, Response = A::Response, Error = A::Error> + Sync,
{
    type Response = A::Response;
    type Error = A::Error;

    async fn serve(
        &self,
        req: Request,
    ) -> Result<Self::Response, Self::Error> {
        match self {
            Self::Left(a) => a.serve(req).await,
            Self::Right(b) => b.serve(req).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ServiceExt as _;

    use super::*;
    use std::convert::Infallible;
    use std::fmt::{Debug, Display};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::time::sleep;
    use tower::{Layer, Service, ServiceBuilder};

    // ====== define test services ======

    #[derive(Debug, PartialEq)]
    struct Error(String);

    impl std::error::Error for Error {}

    impl Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl From<Infallible> for Error {
        fn from(value: Infallible) -> Self {
            Self(format!("{value}"))
        }
    }

    // simple string transformation service
    #[derive(Clone)]
    struct StringService {
        prefix: String,
    }

    impl StringService {
        fn new(prefix: impl Into<String>) -> Self {
            Self {
                prefix: prefix.into(),
            }
        }
    }

    impl AsyncService<String> for StringService {
        type Response = String;
        type Error = Error;

        async fn serve(
            &self,
            req: String,
        ) -> Result<Self::Response, Self::Error> {
            Ok(format!("{}{}", self.prefix, req))
        }
    }

    // delay service - simulates a delay in processing
    #[derive(Clone)]
    struct DelayService {
        delay_ms: u64,
    }

    impl DelayService {
        fn new(delay_ms: u64) -> Self {
            Self { delay_ms }
        }
    }

    impl AsyncService<String> for DelayService {
        type Response = String;
        type Error = Error;

        async fn serve(
            &self,
            req: String,
        ) -> Result<Self::Response, Self::Error> {
            sleep(Duration::from_millis(self.delay_ms)).await;
            Ok(req + " [delayed]")
        }
    }

    // service may fail
    #[derive(Clone)]
    struct FailableService {
        should_fail: bool,
    }

    impl FailableService {
        fn new(should_fail: bool) -> Self {
            Self { should_fail }
        }
    }

    impl AsyncService<String> for FailableService {
        type Response = String;
        type Error = Error;

        async fn serve(
            &self,
            req: String,
        ) -> Result<Self::Response, Self::Error> {
            if self.should_fail {
                Err(Error("Service failed".to_string()))
            } else {
                Ok(req + " [ok]")
            }
        }
    }

    // filter service
    #[derive(Clone)]
    struct FilterService {
        allow_pattern: String,
    }

    impl FilterService {
        fn new(allow_pattern: impl Into<String>) -> Self {
            Self {
                allow_pattern: allow_pattern.into(),
            }
        }
    }

    impl AsyncService<String> for FilterService {
        type Response = Option<String>;
        type Error = Infallible;

        async fn serve(
            &self,
            req: String,
        ) -> Result<Self::Response, Self::Error> {
            if req.contains(&self.allow_pattern) {
                Ok(Some(req))
            } else {
                Ok(None)
            }
        }
    }

    // request counter service - used to validate service calls
    #[derive(Clone)]
    struct CounterService {
        count: Arc<Mutex<usize>>,
    }

    impl CounterService {
        fn new() -> Self {
            Self {
                count: Arc::new(Mutex::new(0)),
            }
        }

        fn get_count(&self) -> usize {
            *self.count.lock().unwrap()
        }
    }

    impl AsyncService<String> for CounterService {
        type Response = String;
        type Error = Infallible;

        async fn serve(
            &self,
            req: String,
        ) -> Result<Self::Response, Self::Error> {
            let mut count = self.count.lock().unwrap();
            *count += 1;
            Ok(req)
        }
    }

    // ====== basic functionality tests ======

    #[tokio::test]
    async fn test_string_service() {
        let service = StringService::new("Hello, ");
        let result = service.serve("world".to_string()).await;
        assert_eq!(result, Ok("Hello, world".to_string()));
    }

    #[tokio::test]
    async fn test_delay_service() {
        let service = DelayService::new(10);
        let result = service.serve("test".to_string()).await;
        assert_eq!(result, Ok("test [delayed]".to_string()));
    }

    #[tokio::test]
    async fn test_failable_service() {
        let success_service = FailableService::new(false);
        let result = success_service.serve("test".to_string()).await;
        assert_eq!(result, Ok("test [ok]".to_string()));

        let fail_service = FailableService::new(true);
        let result = fail_service.serve("test".to_string()).await;
        assert_eq!(result, Err(Error("Service failed".to_string())));
    }

    #[tokio::test]
    async fn test_filter_service() {
        let filter = FilterService::new("allow");

        let result = filter.serve("allow this".to_string()).await;
        assert_eq!(result, Ok(Some("allow this".to_string())));

        let result = filter.serve("block this".to_string()).await;
        assert_eq!(result, Ok(None));
    }

    #[tokio::test]
    async fn test_counter_service() {
        let counter = CounterService::new();
        assert_eq!(counter.get_count(), 0);

        counter.serve("test1".to_string()).await.unwrap();
        assert_eq!(counter.get_count(), 1);

        counter.serve("test2".to_string()).await.unwrap();
        assert_eq!(counter.get_count(), 2);
    }

    // ====== ServiceExt conversion tests ======

    #[tokio::test]
    async fn test_into_tower() {
        let service = StringService::new("Hello, ");
        let mut tower_service = service.into_tower();

        let result = tower_service.call("world".to_string()).await;
        assert_eq!(result, Ok("Hello, world".to_string()));
    }

    #[tokio::test]
    async fn test_into_layer() {
        let service_layer = StringService::new("Hello, ").into_layer();
        let base_service = StringService::new("Base: ");

        let combined = service_layer.layer(base_service);
        let result = combined.serve("world".to_string()).await;
        assert_eq!(result, Ok("Base: Hello, world".to_string()));

        let combined = ServiceBuilder::new()
            .layer(StringService::new("Hello, ").into_layer())
            .layer(StringService::new("Second: ").into_layer())
            .service(StringService::new("Last: "));
        let result = combined.serve("world".to_string()).await;
        assert_eq!(result, Ok("Last: Second: Hello, world".to_string()));
    }

    // ====== combination functionality tests ======

    #[tokio::test]
    async fn test_chain() {
        let first = StringService::new("First: ");
        let second = StringService::new("Second: ");

        let chain = first.chain(second);
        let result = chain.serve("test".to_string()).await;
        assert_eq!(result, Ok("Second: First: test".to_string()));
    }

    #[tokio::test]
    async fn test_filter() {
        let filter = FilterService::new("allow");
        let second_filter = FilterService::new("second");

        let chain = ServiceBuilder::new()
            .layer(filter.into_filter())
            .layer(second_filter.into_filter())
            .service(StringService::new("").map_response(Some));

        // first filter pass, second filter pass
        let result = chain.serve("allow second".to_string()).await;
        assert_eq!(result, Ok(Some("allow second".to_string())));

        // first filter pass, second filter not pass
        let result = chain.serve("allow only".to_string()).await;
        assert_eq!(result, Ok(None));

        // first filter not pass
        let result = chain.serve("reject".to_string()).await;
        assert_eq!(result, Ok(None));
    }

    // ====== error handling tests ======

    #[tokio::test]
    async fn test_error_propagation_in_chain() {
        let first = StringService::new("First: ");
        let second = FailableService::new(true);

        let chain = first.chain(second);
        let result = chain.serve("test".to_string()).await;
        assert_eq!(
            result.map_err(|e| format!("{e}")),
            Err("Service failed".to_string())
        );
    }

    #[tokio::test]
    async fn test_first_service_error_in_chain() {
        let first = FailableService::new(true);
        let second = StringService::new("Second: ");

        let chain = first.chain(second);
        let result = chain.serve("test".to_string()).await;
        assert_eq!(
            result.map_err(|e| format!("{e}")),
            Err("Service failed".to_string())
        );
    }

    // ====== complex composition tests ======

    #[tokio::test]
    async fn test_complex_service_composition() {
        let counter = CounterService::new();
        let counter_clone = counter.clone();

        // service link: Counter -> StringService -> DelayService -> FailableService
        let service = counter
            .clone()
            .chain(StringService::new("Step1: "))
            .chain(DelayService::new(10))
            .chain(FailableService::new(false));

        let result = service.serve("request".to_string()).await;
        assert_eq!(result, Ok("Step1: request [delayed] [ok]".to_string()));
        assert_eq!(counter_clone.get_count(), 1);

        let service = ServiceBuilder::new()
            .layer(counter.into_layer())
            .layer(StringService::new("Step1: ").into_layer())
            .layer(DelayService::new(10).into_layer())
            .service(FailableService::new(false));
        let result = service.serve("request".to_string()).await;
        assert_eq!(result, Ok("Step1: request [delayed] [ok]".to_string()));
        assert_eq!(counter_clone.get_count(), 2);
    }

    #[tokio::test]
    async fn test_complex_filter_chain() {
        // create two filters
        let filter1 = FilterService::new("first");
        let filter2 = FilterService::new("second");

        // create data processing service
        let processor = StringService::new("Processed: ");

        // build filter chain and then attach processing service
        let service_chain = ServiceBuilder::new()
            .layer(filter1.into_filter())
            .layer(filter2.into_filter())
            .service(processor.map_response(Some));

        // test various inputs
        let result =
            service_chain.serve("first second test".to_string()).await;
        assert_eq!(
            result,
            Ok(Some("Processed: first second test".to_string()))
        );

        let result = service_chain.serve("first only".to_string()).await;
        assert_eq!(result, Ok(None));

        let result = service_chain.serve("second only".to_string()).await;
        assert_eq!(result, Ok(None));

        let result = service_chain.serve("none".to_string()).await;
        assert_eq!(result, Ok(None));
    }

    #[tokio::test]
    async fn test_service_layer_composition() {
        let prefix_layer = StringService::new("Prefix: ").into_layer();
        let suffix_layer = StringService::new("")
            .chain(DelayService::new(10))
            .into_layer();

        // create base service
        let base_service = StringService::new("Base: ");

        // apply layers
        let with_prefix = prefix_layer.layer(base_service);
        let with_prefix_and_suffix = suffix_layer.layer(with_prefix);

        let result = with_prefix_and_suffix.serve("request".to_string()).await;
        assert_eq!(result, Ok("Base: Prefix: request [delayed]".to_string()));
    }

    #[tokio::test]
    async fn test_either_service() {
        type StringEither = Either<StringService, StringService>;
        // Test Left variant
        let left_service = StringEither::Left(StringService::new("Left: "));
        let result = left_service.serve("test".to_string()).await;
        assert_eq!(result, Ok("Left: test".to_string()));

        // Test Right variant
        let right_service = StringEither::Right(StringService::new("Right: "));
        let result = right_service.serve("test".to_string()).await;
        assert_eq!(result, Ok("Right: test".to_string()));

        type FailableEither = Either<FailableService, FailableService>;
        // Test with failables
        let success_service =
            FailableEither::Left(FailableService::new(false));
        let result = success_service.serve("test".to_string()).await;
        assert_eq!(result, Ok("test [ok]".to_string()));

        let fail_service = FailableEither::Right(FailableService::new(true));
        let result = fail_service.serve("test".to_string()).await;
        assert_eq!(
            result.map_err(|e| format!("{e}")),
            Err("Service failed".to_string())
        );
    }
}
