use expressjs::prelude::*;
use serde_json::json;

#[tokio::main]
async fn main() {
    let mut app = express();

    app.get("/json", async |_req, res| {
        res.send_json(&json!({
            "status": "success",
            "message": "Hello from JSON!",
            "data": {
                "id": 1,
                "name": "Aragorn",
                "role": "King"
            }
        }))
    });

    app.listen(9000, async |port| {
        println!("🚀 Server running on http://localhost:{port}/json");
    })
    .await;
}
