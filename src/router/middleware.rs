use std::path::Path;

use crate::{handler::Handler, router::layer::LayerKind};

pub trait Middleware {
    fn target_path(&self) -> impl AsRef<Path>;
    fn create_handler(&self) -> Handler;

    fn layer_kind() -> LayerKind {
        LayerKind::Middleware
    }
}
