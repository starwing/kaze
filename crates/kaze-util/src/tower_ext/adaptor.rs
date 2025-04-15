use std::{
    future::{self},
    sync::Arc,
};

use futures::ready;
use parking_lot::Mutex;
use tokio_util::sync::ReusableBoxFuture;
use tower::Service;

pub trait AsyncService<Request> {
    type Response: 'static;
    type Error: 'static;

    fn call(
        &self,
        req: Request,
    ) -> impl Future<Output = Result<Self::Response, Self::Error>> + Send + 'static;
}

pub struct AsyncServiceAdaptor<S, T>
where
    S: AsyncService<T>,
{
    service: S,
    state: Arc<Mutex<SharedState<S::Response, S::Error>>>,
}

struct SharedState<R, E> {
    in_flight: bool,
    waker: Option<std::task::Waker>,
    future: ReusableBoxFuture<'static, Result<R, E>>,
}

impl<R: 'static, E: 'static> SharedState<R, E> {
    fn new() -> Self {
        Self {
            in_flight: false,
            waker: None,
            future: ReusableBoxFuture::new(future::pending()),
        }
    }
}

impl<S, T> AsyncServiceAdaptor<S, T>
where
    S: AsyncService<T>,
{
    pub fn new(service: S) -> Self {
        Self {
            service,
            state: Arc::new(Mutex::new(SharedState::new())),
        }
    }
}

impl<S, T> Service<T> for AsyncServiceAdaptor<S, T>
where
    S: AsyncService<T>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = AsyncFuture<S::Response, S::Error>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        let mut state = self.state.lock();
        if state.in_flight {
            state.waker.replace(cx.waker().clone());
            return std::task::Poll::Pending;
        }
        Ok(()).into()
    }

    fn call(&mut self, req: T) -> Self::Future {
        let mut state = self.state.lock();
        if state.in_flight {
            panic!("Service must be ready before calling");
        }
        state.in_flight = true;
        state.future.set(self.service.call(req));
        AsyncFuture {
            state: self.state.clone(),
        }
    }
}

pub struct AsyncFuture<R, E> {
    state: Arc<Mutex<SharedState<R, E>>>,
}

impl<R, E> Drop for AsyncFuture<R, E> {
    fn drop(&mut self) {
        let mut state = self.state.lock();
        state.in_flight = false;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
}

impl<R, E> Future for AsyncFuture<R, E> {
    type Output = Result<R, E>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut state = self.state.lock();
        let result = ready!(state.future.poll(cx));
        state.in_flight = false;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
        result.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    use tower::ServiceExt;

    // Simple service that returns the length of a string
    struct EchoLengthService;

    impl AsyncService<String> for EchoLengthService {
        type Response = usize;
        type Error = Infallible;

        fn call(
            &self,
            req: String,
        ) -> impl Future<Output = Result<usize, Infallible>> + 'static
        {
            async move { Ok(req.len()) }
        }
    }

    // Service that introduces a delay to test async behavior
    struct DelayService {
        delay_ms: u64,
    }

    impl AsyncService<String> for DelayService {
        type Response = String;
        type Error = Infallible;

        fn call(
            &self,
            req: String,
        ) -> impl Future<Output = Result<String, Infallible>> + 'static
        {
            let delay = self.delay_ms;
            async move {
                tokio::time::sleep(Duration::from_millis(delay)).await;
                Ok(req)
            }
        }
    }

    // Service that fails on specific input
    struct FailingService;

    impl AsyncService<String> for FailingService {
        type Response = String;
        type Error = String;

        fn call(
            &self,
            req: String,
        ) -> impl Future<Output = Result<String, String>> + 'static {
            async move {
                if req == "fail" {
                    Err("Request failed".to_string())
                } else {
                    Ok(req)
                }
            }
        }
    }

    // Service that tracks call count
    #[derive(Clone)]
    struct CountingService {
        count: Arc<AtomicUsize>,
    }

    impl CountingService {
        fn new() -> Self {
            Self {
                count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn get_count(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
    }

    impl AsyncService<String> for CountingService {
        type Response = usize;
        type Error = Infallible;

        fn call(
            &self,
            req: String,
        ) -> impl Future<Output = Result<usize, Infallible>> + 'static
        {
            let count = self.count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(req.len())
            }
        }
    }

    #[tokio::test]
    async fn test_basic_success() {
        // Test basic successful request/response flow
        let service = EchoLengthService;
        let mut adaptor = AsyncServiceAdaptor::new(service);

        adaptor.ready().await.unwrap();
        let result = adaptor.call("hello".to_string()).await;
        assert_eq!(result.unwrap(), 5);

        // Ensure service can be reused
        adaptor.ready().await.unwrap();
        let result = adaptor.call("world!".to_string()).await;
        assert_eq!(result.unwrap(), 6);
    }

    #[tokio::test]
    async fn test_service_busy_state() {
        // Test that service properly handles busy state
        let service = DelayService { delay_ms: 100 };
        let mut adaptor = AsyncServiceAdaptor::new(service);

        // Start a request
        adaptor.ready().await.unwrap();
        let future1 = adaptor.call("test".to_string());

        // Try to ready immediately - should not be ready
        let ready_fut = async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            adaptor.ready().await
        };

        // Check that poll_ready blocks while busy
        let timeout_result =
            tokio::time::timeout(Duration::from_millis(50), ready_fut).await;
        assert!(
            timeout_result.is_err(),
            "poll_ready should block while service is busy"
        );

        // Complete the first request
        let result = future1.await;
        assert_eq!(result.unwrap(), "test");

        // Service should be ready again
        assert!(adaptor.ready().await.is_ok());
    }

    #[tokio::test]
    async fn test_error_handling() {
        // Test error propagation and recovery
        let service = FailingService;
        let mut adaptor = AsyncServiceAdaptor::new(service);

        // Normal request should succeed
        adaptor.ready().await.unwrap();
        let result = adaptor.call("success".to_string()).await;
        assert_eq!(result.unwrap(), "success");

        // Failing request should return error
        adaptor.ready().await.unwrap();
        let result = adaptor.call("fail".to_string()).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Request failed");

        // Service should recover after error
        adaptor.ready().await.unwrap();
        let result = adaptor.call("after_error".to_string()).await;
        assert_eq!(result.unwrap(), "after_error");
    }

    #[tokio::test]
    async fn test_sequential_calls() {
        // Test multiple sequential calls
        let service = CountingService::new();
        let count_before = service.get_count();
        let mut adaptor = AsyncServiceAdaptor::new(service.clone());

        for i in 0..5 {
            adaptor.ready().await.unwrap();
            let result = adaptor.call(format!("req{}", i)).await.unwrap();
            assert_eq!(result, 3 + i.to_string().len());
        }

        assert_eq!(service.get_count(), count_before + 5);
    }

    #[tokio::test]
    async fn test_dropped_future() {
        // Test that dropping a future properly resets service state
        let service = CountingService::new();
        let mut adaptor = AsyncServiceAdaptor::new(service.clone());

        adaptor.ready().await.unwrap();

        // Create and immediately drop a future
        {
            let _future = adaptor.call("drop me".to_string());
            // Future gets dropped here
        }

        // Allow time for cleanup
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Service should be ready again
        assert!(adaptor.ready().await.is_ok());

        let result = adaptor.call("after drop".to_string()).await.unwrap();
        assert_eq!(result, 10);

        // Count should reflect only completed calls
        assert_eq!(service.get_count(), 1);
    }

    #[tokio::test]
    #[should_panic(expected = "Service must be ready before calling")]
    async fn test_call_without_ready() {
        // Test that calling service without waiting for ready causes panic
        let service = DelayService { delay_ms: 50 };
        let mut adaptor = AsyncServiceAdaptor::new(service);

        // First call - fine
        let _fut1 = adaptor.call("request1".to_string());

        // Second call without ready - should panic
        let _fut2 = adaptor.call("request2".to_string());
    }
}
