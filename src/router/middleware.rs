use crate::{handler::Handler, router::layer::LayerKind};

pub trait Middleware {
    fn target_path(&self) -> impl AsRef<str>;
    fn create_handler(&self) -> impl Into<Handler>;

    fn layer_kind() -> LayerKind {
        LayerKind::Middleware
    }
}
