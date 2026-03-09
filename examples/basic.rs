use expressjs::express_state;
use expressjs::prelude::*;
use hyper::header::{self, HeaderValue};
use local_ip_address::local_ip;
use log::info;
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};

fn setup_logger() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .try_init()?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct State {
    request_count: AtomicU32,
}

express_state!(STATE, State);

#[tokio::main]
async fn main() {
    setup_logger().unwrap();

    const PORT: u16 = 9000;
    let mut app = express();

    // Middleware
    app.use_global(NormalizePathMiddleware::new())
        .use_global(CorsMiddleware::permissive())
        .use_global(LoggingMiddleware)
        .use_with("/css/{*p}", StaticServeMiddleware::new("."))
        .use_with("/expressjs_tests/{*p}", StaticServeMiddleware::new("."));

    // Routes

    // GET / - index page
    app.get("/", async |_req, res| {
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
            <h1>Welcome to expressjs</h1>
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
            .content_type("text/html; charset=utf-8")
            .send_html(html)
    });

    // GET /count - request counter
    app.get("/count", async |_req, res| {
        let count = STATE.request_count.fetch_add(1, Ordering::Relaxed);

        res.send_json(&json!({
            "request_count": count,
            "message": "Request count retrieved successfully"
        }))
    });

    // GET /json - static JSON response
    app.get("/json", async |_req, res| {
        res.header(
            hyper::header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=3600"),
        )
        .send_json(&json!({
            "message": "Hello from JSON!",
            "status": "success",
            "version": "1.0"
        }))
    });

    // GET /redirect - redirect to /
    app.get("/redirect", async |_req, res| res.redirect("/"));

    // GET /status - always 400
    app.get("/status", async |_req, res| {
        res.status_code(400).send_text("400 Bad Request")
    });

    // GET /status/:status - echo status param
    app.get("/status/{status}", async |req, res| {
        let value = req.params().get("status").unwrap_or("unknown");
        res.send_text(format!("Status is {value}"))
    });

    // GET /file - stream Cargo.lock (async, borrows res across await)
    app.get("/file", async |_req, res| {
        res.send_file("./Cargo.lock").await
    });

    // /hello - X-Powered-By middleware then response
    // app.use_with("/hello", async |_req: &mut Request, res: &mut Response| {
    //     res.header("x-powered-by", HeaderValue::from_static("DevYatsu"));
    //     stop_res()
    // });

    app.get("/hello", async |_req, res| {
        res.body("Hello, world!")
            .header(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=86400"),
            )
            .header(header::CONTENT_TYPE, HeaderValue::from_static("text/html"))
    });

    // Route builder pattern - multiple methods on the same path
    app.route("/api/v1/user")
        .get(async |_req, res| res.send_text("Get User"))
        .post(async |_req, res| res.send_text("Post User"));

    // all() - matches every HTTP method
    app.all("/ping", async |_req, res| res.send_text("pong"));

    // Custom 404 handler
    app.not_found(async |_req, res| {
        res.status_code(404)
            .send_text("Custom 404: Page not found!")
    });

    // Listen
    app.listen(PORT, async |port| {
        let local_ip = local_ip().unwrap();
        info!("🚀 Server running!");
        info!("📍 Local:   http://localhost:{port}/");
        info!("🌐 Network: http://{local_ip}:{port}/");
    })
    .await
}
