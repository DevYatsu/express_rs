use async_trait::async_trait;
use express_rs::prelude::*;
use serde_json::json;

// Define standard async handlers
async fn all_handler<B: Send + 'static>(_req: Request<B>, res: Response) -> Response {
    res.send_text("all")
}

async fn get_handler<B: Send + 'static>(_req: Request<B>, res: Response) -> Response {
    res.send_text("get")
}

async fn post_handler<B: Send + 'static>(_req: Request<B>, res: Response) -> Response {
    res.send_text("post")
}

#[tokio::test]
async fn test_app_all() {
    let mut app = App::<()>::default();

    app.all("/test", all_handler);

    // Test GET
    let res = app
        .handle(
            hyper::Request::builder()
                .uri("/test")
                .method("GET")
                .body(())
                .unwrap(),
            Response::new(),
        )
        .await;
    assert_eq!(res.get_status(), hyper::StatusCode::OK);

    // Test POST
    let res = app
        .handle(
            hyper::Request::builder()
                .uri("/test")
                .method("POST")
                .body(())
                .unwrap(),
            Response::new(),
        )
        .await;
    assert_eq!(res.get_status(), hyper::StatusCode::OK);
}

#[tokio::test]
async fn test_app_route() {
    let mut app = App::<()>::default();
    app.route("/user").get(get_handler).post(post_handler);

    let res = app
        .handle(
            hyper::Request::builder()
                .uri("/user")
                .method("GET")
                .body(())
                .unwrap(),
            Response::new(),
        )
        .await;
    assert_eq!(res.get_status(), hyper::StatusCode::OK);

    let res = app
        .handle(
            hyper::Request::builder()
                .uri("/user")
                .method("POST")
                .body(())
                .unwrap(),
            Response::new(),
        )
        .await;
    assert_eq!(res.get_status(), hyper::StatusCode::OK);
}

// Middleware must implement the Middleware trait
#[derive(Clone)]
struct LocalsMiddleware;

#[async_trait]
impl<B: Send + Sync + 'static> Middleware<B> for LocalsMiddleware {
    async fn call(&self, req: &mut Request<B>, _res: &mut Response) -> MiddlewareResult {
        req.locals_mut().0.insert("flag".to_string(), json!(true));
        next_res()
    }
}

async fn final_handler<B: Send + 'static>(req: Request<B>, res: Response) -> Response {
    let has_flag = req.locals().0.contains_key("flag");
    if has_flag {
        res.send_text("has flag")
    } else {
        res.status_code(500).send_text("no flag")
    }
}

#[tokio::test]
async fn test_request_locals_persistence() {
    let mut app = App::<()>::default();

    app.use_with("/", LocalsMiddleware);
    app.get("/final", final_handler);

    let res = app
        .handle(
            hyper::Request::builder()
                .uri("/final")
                .method("GET")
                .body(())
                .unwrap(),
            Response::new(),
        )
        .await;

    assert_eq!(res.get_status(), hyper::StatusCode::OK);
}
#[tokio::test]
async fn test_nested_router() {
    let mut app = App::<()>::default();
    let mut api = express_rs::router::Router::<()>::default();

    api.get("/hello", |_, res: Response| async move {
        res.send_text("api hello")
    });

    app.use_router("/api", api);

    let res = app
        .handle(
            hyper::Request::builder()
                .uri("/api/hello")
                .method("GET")
                .body(())
                .unwrap(),
            Response::new(),
        )
        .await;

    assert_eq!(res.get_status(), hyper::StatusCode::OK);
}

#[derive(Clone)]
struct StopMiddleware;

#[async_trait]
impl<B: Send + Sync + 'static> Middleware<B> for StopMiddleware {
    async fn call(&self, _req: &mut Request<B>, res: &mut Response) -> MiddlewareResult {
        res.status_code(401).send_text("Unauthorized");
        stop_res()
    }
}

#[tokio::test]
async fn test_middleware_stop_chain() {
    let mut app = App::<()>::default();

    app.use_with("/", StopMiddleware);

    app.get("/secret", |_, res: Response| async move {
        res.send_text("secret data")
    });

    let res = app
        .handle(
            hyper::Request::builder()
                .uri("/secret")
                .method("GET")
                .body(())
                .unwrap(),
            Response::new(),
        )
        .await;

    assert_eq!(res.get_status(), hyper::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_error_handlers() {
    let mut app = App::<()>::default();
    app.get("/exists", |_, res: Response| async move { res });

    // Test 404
    let res_404 = app
        .handle(
            hyper::Request::builder()
                .uri("/not-found")
                .method("GET")
                .body(())
                .unwrap(),
            Response::new(),
        )
        .await;
    assert_eq!(res_404.get_status(), hyper::StatusCode::NOT_FOUND);

    // Test 405
    let res_405 = app
        .handle(
            hyper::Request::builder()
                .uri("/exists")
                .method("POST")
                .body(())
                .unwrap(),
            Response::new(),
        )
        .await;
    assert_eq!(res_405.get_status(), hyper::StatusCode::METHOD_NOT_ALLOWED);
}
