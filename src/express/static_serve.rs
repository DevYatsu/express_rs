use crate::{
    handler::{Handler, Next, Request, Response},
    router::Middleware,
};

#[derive(Debug, Clone)]
pub struct StaticServeMiddleware(pub &'static str);

impl Middleware for StaticServeMiddleware {
    fn target_path(&self) -> impl Into<String> {
        format!("/{}/{{*p}}", self.0.trim_start_matches('/'))
    }

    fn create_handler(&self) -> impl Into<Handler> {
        |req: &Request, res: &mut Response, next: Next| {
            let uri_path = req.uri().path();
            let file_path = format!(".{}", uri_path);

            if let Err(_) = res.send_file(&file_path) {
                next();
            }
        }
    }
}
