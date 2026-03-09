use expressjs::prelude::*;
use hyper::header::HeaderValue;

#[tokio::main]
async fn main() {
    let mut app = express();

    // Global middleware: Cors
    app.use_global(CorsMiddleware::permissive());

    // Global middleware: Normalize path
    app.use_global(NormalizePathMiddleware::new());

    // Route-specific middleware (using prefix)
    app.use_with("/api", |req: &mut Request, _res: &mut Response| {
        let path = req.uri().path().to_owned();
        async move {
            println!("API middleware triggered for: {}", path);
            next_res()
        }
    });

    app.get("/api/v1", async |_req, res| res.send_text("API Response"));

    // Header manipulation in middleware (global logging)
    app.use_global(LoggingMiddleware);

    // Dynamic middleware logic
    app.use_global(|req: &mut Request, res: &mut Response| {
        let uri = req.uri().to_string();
        res.header("X-Custom-Global", HeaderValue::from_static("Enabled"));
        async move {
            println!("Custom global middleware for: {}", uri);
            next_res()
        }
    });

    app.get("/hello", async |_req, res| res.send_text("Hello!"));

    app.listen(9000, async |port| {
        println!("🚀 Server running on http://localhost:{port}/");
    })
    .await;
}
