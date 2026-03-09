<div align="center">
  <h1>express_rs</h1>
  <p>
    <strong>A blazing-fast, minimal, and ergonomic web framework for Rust, inspired by Express.js.</strong>
  </p>
  <p>
    <a href="https://crates.io/crates/express_rs"><img src="https://img.shields.io/crates/v/express_rs.svg" alt="Crates.io" /></a>
    <a href="https://docs.rs/express_rs"><img src="https://docs.rs/express_rs/badge.svg" alt="docs.rs" /></a>
    <a href="https://github.com/DevYatsu/express_rs/actions"><img src="https://img.shields.io/github/actions/workflow/status/DevYatsu/express_rs/ci.yml" alt="CI Status" /></a>
    <a href="https://crates.io/crates/express_rs"><img src="https://img.shields.io/crates/d/express_rs.svg" alt="Downloads" /></a>
    <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT" /></a>
  </p>
</div>

---

## Overview

**express_rs** aims to provide a clean, expressive, and flexible web framework modeled after the ergonomics of Express.js, but with the performance, safety guarantees, and zero-cost abstractions of Rust.

Our goal is simple: **no macro magic, no boilerplate—just modular, async-first routing, powerful middleware, and composable handlers.**

## Features

- **Ergonomic Routing**: Express-style routing with `.get()`, `.post()`, `.all()`, etc.
- **Robust Middleware System**: Layered composition via `app.use(...)` enabling powerful request pipelines.
- **Zero-Macro Abstraction**: Minimal `App` and `Router` types without the need for complex procedural macros.
- **Fast Route Matching**: Powered by an efficient radix tree (`matchthem`), supporting parameter extraction (`/user/:id`).
- **Memory Efficient**: Avoids allocations in the hot path. Uses `SmallVec` and a global string interner for path and parameter deduplication.
- **Extensions & State API**: Type-safe, per-request parameter access for seamless state sharing across handlers.
- **Comprehensive Built-in Middleware**:
  - `cors`: Cross-Origin Resource Sharing.
  - `auth`: Basic, Bearer, and JWT authentication flows.
  - `rate_limit`: IP-based request throttling.
  - `logging`: Method, path, and elapsed time tracing.
  - `security_headers`: Secure defaults (HSTS, X-Frame-Options, etc.).
  - `static_serve`: Streaming optimization & LRU cache for static files.
  - `limit_body`: Payload size protections to prevent DoS.
  - `normalize_path`: Clean routing by normalizing trailing slashes.

## Getting Started

Add `express_rs` to your `Cargo.toml`:

```toml
[dependencies]
express_rs = "0.1.0"
```

### Basic Example

```rust,ignore
use express_rs::prelude::*;
use express_rs::reexports::Lazy;

#[tokio::main]
async fn main() {
    let mut app = express();

    // Built-in middleware
    app.use_middleware(express_rs::middleware::logging::LoggingMiddleware);

    // Simple routing
    app.get("/", |_req, mut res, _| {
        res.send("Hello from express_rs!");
    });

    // JSON response
    app.get("/api/ping", |_req, mut res, _| {
        res.json(serde_json::json!({ "status": "ok", "message": "pong" }));
    });

    // Start server
    let _ = app.listen(3000, || async {
        println!("🚀 Server listening on http://localhost:3000");
    }).await;
}
```

## Modularity & Routing

Just like Express, you can mount routers to organize your controllers:

```rust,ignore
use express_rs::router::Router;

let mut users_router = Router::new();
users_router.get("/", |_req, mut res, _| { res.send("List of users"); });
users_router.post("/", |_req, mut res, _| { res.send("User created"); });
users_router.get("/:id", |_req, mut res, _| { res.send("User details"); });

app.use_router("/users", users_router);
```

## Security & Middleware

A robust set of middlewares is provided out of the box to help secure and optimize your app. For instance, setting up secure headers and rate limiting:

```rust,ignore
use express_rs::middleware::{rate_limit::*, security_headers::*};

// Restrict to 100 requests per 15 minutes per IP
app.use_middleware(RateLimit::new(100, std::time::Duration::from_secs(900)).into_middleware());

// Apply secure HTTP headers
app.use_middleware(SecurityHeaders::default().into_middleware());
```

## Performance

`express_rs` is built for speed:
- Routes are resolved natively using a specialized Radix tree (`matchthem`).
- Minimal to **zero** heap allocations on a typical incoming HTTP request due to clever internal optimizations.
- Handlers are represented as lightweight function pointers/closures under a uniform `Handler` trait.

## Contributing

We welcome contributions! Whether you're fixing a bug, proposing a new feature, or improving documentation, please feel free to open an issue or a pull request. 

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## License

Distributed under the MIT License. See `LICENSE` for more information.
