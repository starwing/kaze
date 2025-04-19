use super::*;

pub trait ServiceExt<Request>: AsyncService<Request> + Sized {
    fn map_response<F>(self, f: F) -> MapResponse<F, Self> {
        MapResponse::new(f, self)
    }

    fn chain<T>(self, other: T) -> Chain<Self, T> {
        Chain::new(self, other)
    }

    fn into_tower(self) -> AsyncServiceAdaptor<Self, Request> {
        self.into()
    }

    fn into_layer(self) -> ServiceLayer<Self>
    where
        Self: Clone,
    {
        self.into()
    }

    fn into_filter(self) -> FilterLayer<Self>
    where
        Self: Clone,
    {
        self.into()
    }
}

impl<Request, S> ServiceExt<Request> for S where
    S: AsyncService<Request> + Sized
{
}

impl<Request, S> From<S> for AsyncServiceAdaptor<S, Request>
where
    S: AsyncService<Request>,
{
    fn from(value: S) -> Self {
        Self::new(value)
    }
}

impl<S> From<S> for ServiceLayer<S> {
    fn from(value: S) -> Self {
        Self::new(value)
    }
}

impl<S> From<S> for FilterLayer<S> {
    fn from(value: S) -> Self {
        Self::new(value)
    }
}
