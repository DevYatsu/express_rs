use crate::handler::{ExpressResponse, Request, Response};
use crate::middleware::{Middleware, MiddlewareResult, next_res, stop_res};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct StaticServeMiddleware;

#[async_trait]
impl Middleware for StaticServeMiddleware {
    async fn call(&self, req: &mut Request, res: &mut Response) -> MiddlewareResult {
        let uri_path = req.uri().path();
        let file_path = format!(".{}", uri_path);

        if let Err(_) = res.file(&file_path).await {
            return next_res();
        }

        res.status_code(200).unwrap();
        stop_res()
    }
}
