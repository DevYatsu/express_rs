use express_rs::{
    app,
    handler::{Request, Response},
};

#[tokio::main]
async fn main() {
    let mut app = app();
    const PORT: u16 = 8080;

    app.get("/", |_req: Request, res: &mut Response, _next| {
        res.write_head(200).write("Test").end();
    });

    app.listen(PORT, || println!("Server listening on port {}", PORT))
        .await
}
