use expressjs::prelude::*;

#[tokio::main]
async fn main() {
    let mut app = express();

    app.get("/", async |_req, res| res.send_text("Hello World!"));

    app.listen(9000, async |port| {
        println!("🚀 Server running on http://localhost:{port}/");
    })
    .await;
}
