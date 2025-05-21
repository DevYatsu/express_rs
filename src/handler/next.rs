use std::sync::Arc;

pub type Next = Arc<dyn Fn() + Send + Sync + 'static>;