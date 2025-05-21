use crate::{handler::Handler, router::layer::LayerKind};

/// Trait representing a middleware component in the framework.
///
/// Middleware is a reusable logic layer that can intercept and
/// manipulate requests and responses before they reach a route handler.
pub trait Middleware {
    /// Produces the actual handler function that will be invoked when the middleware runs.
    ///
    /// The handler must conform to the standard middleware signature and will be wrapped
    /// into the framework's `Handler` type.
    fn create_handler(&self) -> impl Handler;

    /// Returns the kind of layer this represents (always middleware here).
    ///
    /// Used internally to differentiate middleware layers from route layers.
    fn layer_kind() -> LayerKind {
        LayerKind::Middleware
    }
}
