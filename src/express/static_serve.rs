use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, RwLock},
};

use bytes::Bytes;
use hyper::header::{CACHE_CONTROL, HeaderValue};
use once_cell::sync::Lazy;

use crate::{
    handler::{Handler, Next, Request, Response},
    router::Middleware,
};

// Global file cache: maps path -> content
static FILE_CACHE: Lazy<RwLock<HashMap<String, Arc<Bytes>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

#[derive(Debug, Clone)]
pub struct StaticServeMiddleware<P: AsRef<Path>>(pub P);

impl<P: AsRef<Path> + Send + Sync> Middleware for StaticServeMiddleware<P> {
    fn target_path(&self) -> impl Into<String> {
        "*"
    }

    fn create_handler(&self) -> Handler {
        let base_path = self.0.as_ref().to_path_buf();

        Handler::from(move |req: &Request, res: &mut Response, next: Next| {
            let uri_path = req.uri().path();

            res.set(
                CACHE_CONTROL,
                HeaderValue::from_static("no-store, no-cache, must-revalidate, proxy-revalidate"),
            );

            // Prevent directory traversal (e.g., "..")
            if !uri_path.starts_with(&*base_path.to_string_lossy()) {
                return next();
            }

            let full_path = format!(".{}", uri_path);

            // Check the cache first
            {
                let cache = FILE_CACHE.read().unwrap();
                if let Some(cached) = cache.get(&full_path) {
                    res.send(cached.as_ref());
                    return;
                }
            }

            // Read the file and cache it
            match fs::read(&full_path) {
                Ok(content) => {
                    let bytes = Arc::new(Bytes::from(content));
                    FILE_CACHE
                        .write()
                        .unwrap()
                        .insert(full_path.clone(), bytes.clone());
                    res.send(bytes.as_ref());
                }
                Err(_) => next(),
            }
        })
    }
}
