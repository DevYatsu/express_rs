use crate::handler::{
    Request, Response,
    middleware::{Middleware, next, stop},
};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct StaticServeMiddleware;

#[async_trait]
impl Middleware for StaticServeMiddleware {
    async fn call(
        &self,
        req: &mut Request,
        res: &mut Response,
    ) -> crate::handler::middleware::MiddlewareResult {
        let uri_path = req.uri().path();
        let file_path = format!(".{}", uri_path);

        if let Err(_) = res.file(&file_path).await {
            return next();
        }

        res.status_code(200).unwrap();
        stop()
    }
}
