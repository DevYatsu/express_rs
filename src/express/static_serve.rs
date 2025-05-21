use crate::handler::{Handler, HandlerResult, Next, Request, Response};
use futures_util::FutureExt;

#[derive(Debug, Clone)]
pub struct StaticServeMiddleware;

impl Handler for StaticServeMiddleware {
    fn call<'a>(
        &'a self,
        req: &'a mut Request,
        res: &'a mut Response,
        mut next: Next,
    ) -> HandlerResult<'a> {
        async move {
            let uri_path = req.uri().path();
            let file_path = format!(".{}", uri_path);

            if let Err(_) = res.send_file(&file_path) {
                next();
            }
        }
        .boxed()
    }
}
