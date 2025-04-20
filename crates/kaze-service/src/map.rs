use super::*;

#[derive(Clone, Copy)]
pub struct MapRequest<F, S> {
    f: F,
    service: S,
}

impl<F, S> MapRequest<F, S> {
    pub fn new(f: F, service: S) -> Self {
        Self { f, service }
    }
}

impl<Request, Middle, F, S> AsyncService<Request> for MapRequest<F, S>
where
    Request: Send + 'static,
    Middle: 'static,
    S: AsyncService<Middle> + Sync,
    F: (Fn(Request) -> Middle) + Sync,
{
    type Response = S::Response;
    type Error = S::Error;

    async fn serve(
        &self,
        req: Request,
    ) -> Result<Self::Response, Self::Error> {
        self.service.serve((self.f)(req)).await
    }
}

#[derive(Clone, Copy)]
pub struct MapResponse<F, S> {
    f: F,
    service: S,
}

impl<F, S> MapResponse<F, S> {
    pub fn new(f: F, service: S) -> Self {
        Self { f, service }
    }
}

impl<Request, Response, F, S> AsyncService<Request> for MapResponse<F, S>
where
    Request: Send + 'static,
    Response: 'static,
    S: AsyncService<Request> + Sync,
    F: (Fn(S::Response) -> Response) + Sync,
{
    type Response = Response;
    type Error = S::Error;

    async fn serve(
        &self,
        req: Request,
    ) -> Result<Self::Response, Self::Error> {
        Ok((self.f)(self.service.serve(req).await?))
    }
}

#[derive(Clone, Copy)]
pub struct MapError<F, S> {
    f: F,
    service: S,
}

impl<F, S> MapError<F, S> {
    pub fn new(f: F, service: S) -> Self {
        Self { f, service }
    }
}

impl<Request, Error, F, S> AsyncService<Request> for MapError<F, S>
where
    Request: Send + 'static,
    Error: 'static,
    S: AsyncService<Request> + Sync,
    F: (Fn(S::Error) -> Error) + Sync,
{
    type Response = S::Response;
    type Error = Error;

    async fn serve(
        &self,
        req: Request,
    ) -> Result<Self::Response, Self::Error> {
        self.service.serve(req).await.map_err(|e| (self.f)(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestService {
        response: String,
    }

    impl AsyncService<String> for TestService {
        type Response = String;
        type Error = String;

        async fn serve(
            &self,
            req: String,
        ) -> Result<Self::Response, Self::Error> {
            if req == "error" || req == "ERROR" {
                Err("service error".to_string())
            } else {
                Ok(format!("{}-{}", req, self.response))
            }
        }
    }

    #[tokio::test]
    async fn test_map_request() {
        let service = TestService {
            response: "response".to_string(),
        };
        let map = service.map_request(|req: &str| req.to_uppercase());

        let result = map.serve("hello").await;
        assert_eq!(result, Ok("HELLO-response".to_string()));

        let result = map.serve("error").await;
        assert_eq!(result, Err("service error".to_string()));
    }

    #[tokio::test]
    async fn test_map_response() {
        let service = TestService {
            response: "response".to_string(),
        };
        let map = service.map_response(|resp: String| resp.to_uppercase());

        let result = map.serve("hello".to_string()).await;
        assert_eq!(result, Ok("HELLO-RESPONSE".to_string()));

        let result = map.serve("error".to_string()).await;
        assert_eq!(result, Err("service error".to_string()));
    }

    #[tokio::test]
    async fn test_map_error() {
        let service = TestService {
            response: "response".to_string(),
        };
        let map = service.map_err(|err: String| format!("mapped: {}", err));

        let result = map.serve("hello".to_string()).await;
        assert_eq!(result, Ok("hello-response".to_string()));

        let result = map.serve("error".to_string()).await;
        assert_eq!(result, Err("mapped: service error".to_string()));
    }

    #[tokio::test]
    async fn test_combined_maps() {
        let service = TestService {
            response: "response".to_string(),
        };
        let map = service
            .map_request(|req: &str| req.to_uppercase())
            .map_response(|resp: String| resp.replace("-", ":"))
            .map_err(|err: String| format!("{{error: {}}}", err));

        let result = map.serve("hello").await;
        assert_eq!(result, Ok("HELLO:response".to_string()));

        let result = map.serve("error").await;
        assert_eq!(result, Err("{error: service error}".to_string()));
    }
}
