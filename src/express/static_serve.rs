use futures_util::FutureExt;

use crate::handler::{
    Request, Response,
    middleware::{Middleware, MiddlewareFuture, next, stop},
};

#[derive(Debug, Clone)]
pub struct StaticServeMiddleware;

impl Middleware for StaticServeMiddleware {
    fn call<'a, 'b>(&'a self, req: &'b mut Request, res: &'b mut Response) -> MiddlewareFuture<'b> {
        async move {
            let uri_path = req.uri().path();
            let file_path = format!(".{}", uri_path);

            if let Err(_) = res.send_file(&file_path) {
                return next().await;
            }

            res.status_code(200).unwrap();
            stop().await
        }
        .boxed()
    }
}
