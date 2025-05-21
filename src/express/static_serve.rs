use crate::{
    handler::{Handler, Next, Request, Response},
};

#[derive(Debug, Clone)]
pub struct StaticServeMiddleware;

use std::future::Future;
use std::pin::Pin;

impl Handler for StaticServeMiddleware {
    fn handle<'a>(
        &self,
        req: &'a mut Request,
        res: &'a mut Response,
        next: Next,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let uri_path = req.uri().path();
        let file_path = format!(".{}", uri_path);

        Box::pin(async move {
            if let Err(_) = res.send_file(&file_path) {
                next();
            }
        })
    }
}
