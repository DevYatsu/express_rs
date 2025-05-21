use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Clone)]
pub struct Next {
    called: Arc<AtomicBool>,
}

impl Next {
    pub fn new() -> Self {
        Self {
            called: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn call(&self) {
        // Could await something, or just set a flag
        self.called.store(true, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.called.store(false, Ordering::Relaxed);
    }

    pub fn was_called(&self) -> bool {
        self.called.load(Ordering::Relaxed)
    }
}
