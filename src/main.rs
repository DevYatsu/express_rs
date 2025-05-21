use express_rs::{
    app,
    express::StaticServeMiddleware,
    handler::{Next, Request, Response, request::RequestExt},
};
use hyper::{
    StatusCode,
    header::{self, HeaderValue},
};
use local_ip_address::local_ip;
use log::info;
use serde_json::json;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    const PORT: u16 = 9000;
    let mut app = app();

    app.use_with(StaticServeMiddleware("src"));
    app.use_with(StaticServeMiddleware("css"));
    app.use_with(StaticServeMiddleware("src"));
    app.use_with(StaticServeMiddleware("expressjs_tests"));

    app.get("/", |_req: &mut Request, res: &mut Response, _| {
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
            .r#type("text/html; charset=utf-8")
            .send(html);

        async {};
    }); 

    app.get("/json", |_req: &mut Request, res: &mut Response, _| {
        res.json(&json!({
            "message": "Hello from JSON!",
            "status": "success",
            "version": "1.0"
        }))
        .unwrap();
    });

    app.get("/redirect", |_req: &mut Request, res: &mut Response, _| {
        res.redirect("/");
    });

    app.get("/status", |_req: &mut Request, res: &mut Response, _| {
        res.status(StatusCode::BAD_REQUEST).send("400 Bad Request");
    });

    app.get(
        "/status/{status}",
        |req: &mut Request, res: &mut Response, _| {
            res.send(format!("Status is {}", req.params().get("status").unwrap()));
        },
    );

    app.get("/file", |_req: &mut Request, res: &mut Response, _| {
        res.send_file("./Cargo.lock")
            .map_err(|_| {
                *res = Response::internal_error();
            })
            .unwrap();
    });

    app.get(
        "/hello",
        |_req: &mut Request, res: &mut Response, next: Next| {
            res.write("Hello, world")
                .set(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=86400"),
                )
                .set(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));

            next()
        },
    );

    app.get("/hello", |_req: &mut Request, res: &mut Response, _| {
        res.write("!").end();
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
