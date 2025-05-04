use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default)]
pub struct Route {
    path: PathBuf,
    pub stack: Vec<Layer>,
    methods: HashMap<Method, bool>,
}

impl Route {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            stack: Vec::new(),
            methods: HashMap::new(),
        }
    }
}

use super::layer::Layer;
use crate::{handler::Handler, methods::Method};
use std::str::FromStr;

macro_rules! generate_methods {
    (
        methods: [$($method:ident),* $(,)?]
    ) => {
        impl Route {
            $(
                pub fn $method(&mut self, handler: impl Into<Handler>) -> &mut Self {
                    use super::Layer;
                    let mut layer = Layer::new("/", handler);
                    let method = Method::from_str(stringify!($method)).expect("This method is not a valid Method");
                    layer.method = Some(method);

                    self.methods.insert(method, true);
                    self.stack.push(layer);

                    self
                }
            )*
        }
    };
}

generate_methods! {
    methods: [get, post, patch, delete]
}
