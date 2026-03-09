use expressjs::prelude::*;

#[tokio::main]
async fn main() {
    let mut app = express();

    // Static route
    app.get("/", async |_req, res| res.send_text("Welcome!"));

    // Route with one parameter
    app.get("/users/{id}", async |req, res| {
        let id = req.params().get("id").unwrap_or("unknown");
        res.send_text(format!("User ID: {id}"))
    });

    // Route with multiple parameters
    app.get(
        "/posts/{post_id}/comments/{comment_id}",
        async |req, res| {
            let post_id = req.params().get("post_id").unwrap_or("unknown");
            let comment_id = req.params().get("comment_id").unwrap_or("unknown");
            res.send_text(format!("Post ID: {post_id}, Comment ID: {comment_id}"))
        },
    );

    // Catch-all (glob)
    app.get("/static/{*path}", async |req, res| {
        let path = req.params().get("path").unwrap_or("");
        res.send_text(format!("Serving file from: {path}"))
    });

    app.listen(9000, async |port| {
        println!("🚀 Server running on http://localhost:{port}/");
    })
    .await;
}
