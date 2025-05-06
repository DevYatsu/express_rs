use crate::{handler::Handler, router::layer::LayerKind};
use std::collections::HashMap;

pub trait Middleware {
    fn target_path(&self) -> impl Into<String>;
    fn create_handler(&self) -> impl Into<Handler>;

    fn layer_kind() -> LayerKind {
        LayerKind::Middleware
    }
}
