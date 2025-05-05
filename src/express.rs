use crate::application::App;

mod static_serve;

pub use static_serve::StaticServeMiddleware;

pub fn app() -> App {
    App::default()
}
