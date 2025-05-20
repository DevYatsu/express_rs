# express_rs

> A minimal and blazing-fast web framework for Rust, inspired by [Express.js](https://expressjs.com/)

## 🚀 Overview

**express_rs** aims to provide a clean, expressive, and flexible web framework modeled after the ergonomics of Express.js, but with the performance and safety guarantees of Rust.

The goal is to keep things simple, fast, and ergonomic — no macros, no ceremony — just modular, async-first routing, middlewares, and handler composition.

---

## ✅ Features (Implemented)

- [x] Express-style routing with `.get()`, `.post()`, etc.
- [x] Middleware system (`app.use(...)`) with layered composition
- [x] Minimal `App` type (no macro or derive required)
- [x] Request + Response types with mutable APIs
- [x] Route parameter extraction (`/user/:id`)
- [x] Route handler abstraction (`Handler`) with `Next`
- [x] Logging with method, path and elapsed time
- [x] Built-in response helpers (e.g., `.send()`, `.json()`, `.status()`)
- [x] File serving with streaming optimization + LRU cache
- [x] MIME type detection (via [`mime_guess`] and [`infer`])
- [x] Route matching via fast radix tree (`matchthem`)
- [x] Global string interner for path and param deduplication
- [x] Type-safe per-request parameter access via extension API
- [x] MethodKind enum + method bitflag support

---

## 🛠️ Goals & Roadmap

### Core Architecture

- [x] Internal `App` struct with lazy router initialization
- [x] `Router` type with per-method route trees
- [x] Middleware pipeline with early return support
- [x] Symbol-based interned parameters

### Developer Experience

- [ ] Static type-safe API for request extensions (`.locals`, etc.)
- [ ] Global `AppState<T>` support like Actix
- [ ] Built-in logging/tracing macros with levels
- [ ] Built-in `.env` support for config (optional)
- [ ] Pluggable middleware (logger, body-parser, etc.)

### Advanced Routing

- [ ] Path prefix mounting (`app.use('/api', apiApp)`)
- [ ] Route grouping (`router.route('/users').get(...).post(...)`)
- [ ] Optional trailing slash handling
- [ ] Regex or wildcard segments (`*` / `.*`)

### Performance

- [ ] Benchmarks against Axum/Actix/Warp
- [ ] Zero-copy response body writing
- [ ] Avoid all heap allocations in hot path
- [x] Smallvec for handler/index lists

---

## 🧪 Example

```rust
use express_rs::App;

fn main() {
    let mut app = App::default();

    app.get("/", |req, res, _| {
        res.send("Hello from Rust!");
    });

    app.listen(3000, || {
        println!("🚀 Listening on http://localhost:3000");
    });
}
```
