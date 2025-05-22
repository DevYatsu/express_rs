use express_rs::{
    app,
    express::StaticServeMiddleware,
    handler::{Request, Response, middleware::next_fut, request::RequestExt},
};
use hyper::{
    StatusCode,
    header::{self, HeaderValue},
};
use local_ip_address::local_ip;
use log::{error, info};
use serde_json::json;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    const PORT: u16 = 9000;
    let mut app = app();

    app.use_with("/src/{{*p}}", StaticServeMiddleware);
    app.use_with("/css/{{*p}}", StaticServeMiddleware);
    app.use_with("/expressjs_tests/{{*p}}", StaticServeMiddleware);

    app.get("/", async |_req: Request, mut res: Response| {
        let html = r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Welcome</title>
            <link rel="stylesheet" href="/css/index.css"/>
        </head>
        <body>
            <h1>Welcome to express_rs</h1>
            <p>This is a minimal HTML page served from Rust.</p>
            <ul>
                <li><a href="/json">JSON Endpoint</a></li>
                <li><a href="/redirect">Redirect to Home</a></li>
                <li><a href="status">Trigger 400</a></li>
                <li><a href="hello">Say Hello</a></li>
            </ul>
        </body>
        </html>
        "#;

        res.status_code(200)
            .unwrap()
            .content_type("text/html; charset=utf-8");

        res.send_html(html)
    });

    app.get("/json", async |_req: Request, mut res: Response| {
        res.json(&json!({
            "message": "Hello from JSON!",
            "status": "success",
            "version": "1.0"
        }))
        .unwrap();

        res
    });

    app.get("/redirect", async |_req: Request, _res: Response| {
        Response::redirect("/")
    });

    app.get("/status", async |_req: Request, mut res: Response| {
        res.status(StatusCode::BAD_REQUEST);
        res.send_text("400 Bad Request")
    });

    app.get("/status/{status}", async |req: Request, res: Response| {
        res.send_text(format!("Status is {}", req.params().get("status").unwrap()))
    });

    app.get("/file", async |_req: Request, res: Response| {
        match res.send_file("./Cargo.lock").await {
            Ok(r) => r,
            Err(e) => {
                error!("Error sending file: {}", e);
                Response::internal_server_error()
            }
        }
    });

    app.use_with("/hello", |req: &mut Request, res: &mut Response| {
        let path = req.uri().path();
        #[cfg(debug_assertions)]
        info!("Request received for path: {}", path);

        res.header("x-powered-by", HeaderValue::from_static("DevYatsu"));

        let path = req.uri().path();
        #[cfg(debug_assertions)]
        info!("Middleware processing for path: {}", path);

        next_fut()
    });

    app.get("/hello", async |_req: Request, mut res: Response| {
        res.body("Hello, world")
            .header(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=86400"),
            )
            .header(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));

        res.write("!");

        res
    });

    println!("{:?}", app.router.routes);

    app.listen(PORT, async || {
        let local_ip = local_ip().unwrap();
        info!("🚀 Server running!");
        info!("📍 Local:   http://localhost:{PORT}/");
        info!("🌐 Network: http://{local_ip}:{PORT}/");
    })
    .await
}
