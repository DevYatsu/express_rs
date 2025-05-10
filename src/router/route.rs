use super::layer::Layer;
use crate::handler::Handler;
use hyper::Method;
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct Route {
    pub path: String,
    pub stack: Vec<Layer>,
    pub methods: HashSet<Method>,
}

impl Route {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            stack: Vec::new(),
            methods: HashSet::new(),
        }
    }
}

macro_rules! generate_methods {
    (
        methods: [$($method:ident),* $(,)?]
    ) => {
        impl Route {
            $(
                pub fn $method(&mut self, handler: impl Into<Handler>) -> &mut Self {
                    use std::str::FromStr;
                    use super::Layer;
                    let mut layer = Layer::new("/", handler);
                    let method = Method::from_str(&stringify!($method).to_uppercase()).expect("This method is not a valid Method");
                    layer.method = Some(method.clone());

                    self.methods.insert(method);
                    self.stack.push(layer);

                    self
                }
            )*
        }
    };
}

generate_methods! {
    methods: [get, post, put, delete, patch, head, connect, trace]
}
