use crate::{handler::Handler, router::layer::LayerKind};

pub trait Middleware {
    fn target_path(&self) -> impl Into<String>;
    fn create_handler(&self) -> Handler;

    fn layer_kind() -> LayerKind {
        LayerKind::Middleware
    }
}
