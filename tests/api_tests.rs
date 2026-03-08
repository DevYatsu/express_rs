use express_rs::prelude::*;
use async_trait::async_trait;
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
    app.route("/user")
        .get(get_handler)
        .post(post_handler);

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
        req.locals()
            .0
            .insert("flag".to_string(), json!(true));
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
