use tower::Layer;

use super::chain_service::Chain;

pub struct ChainLayer<S> {
    service: S,
}

impl<S> ChainLayer<S> {
    pub fn new(service: S) -> Self {
        Self { service }
    }
}

impl<Inner: Clone, Outer> Layer<Outer> for ChainLayer<Inner> {
    type Service = Chain<Inner, Outer>;

    fn layer(&self, outer: Outer) -> Self::Service {
        Chain::new(self.service.clone(), outer)
    }
}

impl<Inner: Clone> Clone for ChainLayer<Option<Inner>> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::{service_fn, ServiceExt as _};

    #[tokio::test]
    async fn test_chain_layer() {
        let svc1 = service_fn(|a: u32| async move {
            Ok::<_, anyhow::Error>(a as u64 + 1)
        });
        let svc2 =
            service_fn(
                |a: u64| async move { Ok::<_, anyhow::Error>(a as u8 + 2) },
            );
        let layer = ChainLayer::new(svc1);
        let svc = layer.layer(svc2);
        let r: u8 = svc.oneshot(1).await.unwrap();
        assert_eq!(r, 4);
    }
}
