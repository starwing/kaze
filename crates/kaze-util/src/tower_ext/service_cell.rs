use std::sync::OnceLock;

use tower::Service;

#[derive(Debug, Clone)]
pub struct ServiceCell<S> {
    inner: OnceLock<S>,
}

impl<S> ServiceCell<S> {
    pub fn new() -> Self {
        Self {
            inner: OnceLock::new(),
        }
    }

    pub fn set(&self, inner: S) -> Option<S> {
        self.inner.set(inner).err()
    }

    pub fn get_inner(&self) -> Option<&S> {
        self.inner.get()
    }
}

impl<T, S: Service<T>> Service<T> for ServiceCell<S> {
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        if let Some(inner) = self.inner.get_mut() {
            return inner.poll_ready(cx);
        }
        panic!("ServiceCell is not initialized")
    }

    fn call(&mut self, req: T) -> Self::Future {
        if let Some(inner) = self.inner.get_mut() {
            return inner.call(req);
        }
        panic!("ServiceCell is not initialized")
    }
}
