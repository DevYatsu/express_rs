use crate::{
    handler::{Handler, Next, Request, Response},
    router::Middleware,
};
use bytes::Bytes;
use lru::LruCache;
use once_cell::sync::Lazy;
use std::num::NonZeroUsize;
use std::{
    fs,
    sync::{Arc, Mutex},
};

// Global file cache with LRU eviction
static FILE_CACHE: Lazy<Mutex<LruCache<String, Arc<Bytes>>>> =
    Lazy::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(128).unwrap())));

#[derive(Debug, Clone)]
pub struct StaticServeMiddleware(pub &'static str);

impl Middleware for StaticServeMiddleware {
    fn target_path(&self) -> impl Into<String> {
        format!("/{}/{{*p}}", self.0.trim_start_matches('/'))
    }

    fn create_handler(&self) -> Handler {
        Handler::new(move |req: &Request, res: &mut Response, next: Next| {
            let uri_path = req.uri().path();
            let file_path = format!(".{}", uri_path);

            // Check cache
            if let Some(cached) = FILE_CACHE.lock().unwrap().get(&file_path) {
                res.send(cached.as_ref());
                return;
            }

            // Attempt to read and maybe cache
            match fs::read(&file_path) {
                Ok(data) => {
                    let len = data.len();
                    let arc_bytes = Arc::new(Bytes::from(data));

                    if len < 512 * 1024 {
                        FILE_CACHE
                            .lock()
                            .unwrap()
                            .put(file_path.clone(), arc_bytes.clone());
                    }

                    res.send(arc_bytes.as_ref());
                }
                Err(_) => next(),
            }
        })
    }
}
