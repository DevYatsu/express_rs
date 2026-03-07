use express_rs::{express, handler::response::Response};

#[tokio::main]
async fn main() {
    let mut app = express();

    app.get("/", |_, res: Response| async move {
        res.send_text("Hello Secure World!")
    });

    // To run HTTPS you would load your keys and certs into the rustls ServerConfig:
    // let certs = certs(&mut BufReader::new(File::open("cert.pem")?))
    //     .map(|c| c.unwrap())
    //     .collect();
    // let key = private_key(&mut BufReader::new(File::open("key.pem")?))
    //     .unwrap()
    //     .unwrap();
    // let config = ServerConfig::builder()
    //     .with_no_client_auth()
    //     .with_single_cert(certs, key)
    //     .unwrap();

    // app.listen_https(443, config, || async {
    //     println!("Secure server running on https://localhost:443");
    // }).await;
}
