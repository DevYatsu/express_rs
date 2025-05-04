use std::path::Path;

use crate::{handler::Handler, router::layer::LayerKind};

pub trait Middleware {
    fn path(&self) -> impl AsRef<Path>;
    fn new() -> Handler;

    fn layer_kind() -> LayerKind {
        LayerKind::Middleware
    }
}
