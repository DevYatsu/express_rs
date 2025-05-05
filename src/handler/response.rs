use bytes::BytesMut;
use futures_core::Stream;
use futures_util::StreamExt;
use http_body_util::{Full, StreamBody};
use hyper::{
    HeaderMap, Response as HyperResponse, StatusCode,
    body::{Body, Bytes, Frame},
    header::{
        self, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, EXPIRES, HeaderName, HeaderValue,
        IntoHeaderName, LINK, LOCATION, PRAGMA, SERVER, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
        X_XSS_PROTECTION,
    },
};
use itoa::Buffer;
use lru::LruCache;
use memmap2::Mmap;
use mime_guess;
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    fmt::Debug,
    fs::{self, File},
    io::ErrorKind,
    num::NonZeroUsize,
    pin::Pin,
    str::FromStr,
    sync::{Arc, Mutex},
};

static FILE_CACHE: Lazy<Mutex<LruCache<String, Arc<Bytes>>>> =
    Lazy::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(128).unwrap())));

#[derive(Default)]
pub struct Response {
    status: StatusCode,
    body: BytesMut,
    headers: HeaderMap,
    ended: bool,
    streaming: Option<Pin<Box<dyn Body<Data = Bytes, Error = hyper::Error> + Send>>>,
}

const DEFAULT_HEADERS: Lazy<HeaderMap> = Lazy::new(|| {
    let mut map = HeaderMap::new();
    map.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    map.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    map.insert(EXPIRES, HeaderValue::from_static("0"));
    map.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    map.insert(X_FRAME_OPTIONS, HeaderValue::from_static("SAMEORIGIN"));
    map.insert(X_XSS_PROTECTION, HeaderValue::from_static("0"));
    map.insert(SERVER, HeaderValue::from_static(""));
    map
});

impl Response {
    /// Creates a new `Response` with default headers and empty body.
    pub fn new() -> Self {
        Self {
            status: StatusCode::OK,
            body: BytesMut::with_capacity(512),
            headers: HeaderMap::with_capacity(8),
            ended: false,
            streaming: None,
        }
    }

    /// Returns `true` if the response has been marked as ended.
    pub fn is_ended(&self) -> bool {
        self.ended
    }

    /// Returns `true` if the response body is a streaming one.
    pub fn is_streaming(&self) -> bool {
        self.streaming.is_some()
    }

    /// Sets the HTTP status code.
    pub fn status(&mut self, status: impl Into<StatusCode>) -> &mut Self {
        self.status = status.into();
        self
    }

    /// Sets the HTTP status code from a `u16`.
    ///
    /// ### Panics
    /// Panics if the u16 status is not a valid status (must be >100 and <1000)
    pub fn status_code(&mut self, status: u16) -> &mut Self {
        self.status = StatusCode::from_u16(status).expect("Expected valid status code");
        self
    }

    /// Returns the canonical reason phrase for the current status code.
    pub fn status_text(&self) -> &'static str {
        self.status.canonical_reason().unwrap_or("Unknown")
    }

    /// Sets the status and sends the corresponding reason as body text.
    pub fn send_status(&mut self, code: u16) -> &mut Self {
        self.status_code(code);
        let message = self.status_text();
        self.r#type("text/plain; charset=utf-8").send(message)
    }

    /// Sets the `Content-Length` header.
    pub fn content_length(&mut self, len: usize) -> &mut Self {
        let mut buf = Buffer::new();
        let len_str = buf.format(len);
        if let Ok(val) = HeaderValue::from_str(len_str) {
            self.set(header::CONTENT_LENGTH, val);
        }
        self
    }

    /// Inserts or overrides a header.
    pub fn set<K: IntoHeaderName, V: Into<HeaderValue>>(&mut self, key: K, val: V) -> &mut Self {
        self.headers.insert(key, val.into());
        self
    }

    /// Gets a header by string name.
    pub fn get(&self, key: &str) -> Option<&HeaderValue> {
        HeaderName::from_str(key)
            .ok()
            .and_then(|k| self.headers.get(&k))
    }

    /// Appends data to the response body without ending it.
    pub fn write(&mut self, data: impl AsRef<[u8]>) -> &mut Self {
        self.body.extend_from_slice(data.as_ref());
        self
    }

    /// Clears and sets the body, updates headers, and ends the response.
    pub fn send(&mut self, data: impl AsRef<[u8]>) -> &mut Self {
        let data = data.as_ref();
        self.body.clear();
        self.body.reserve(data.len());
        self.body.extend_from_slice(data);

        let mut buf = Buffer::new();
        let len_str = buf.format(data.len());
        self.set(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(len_str).unwrap(),
        );

        if self.headers.get(CONTENT_TYPE).is_none() {
            self.r#type(if std::str::from_utf8(data).is_ok() {
                "text/plain; charset=utf-8"
            } else {
                "application/octet-stream"
            });
        }

        self.end()
    }

    /// Sends a JSON response with appropriate content type.
    pub fn json<T: serde::Serialize>(&mut self, value: T) -> &mut Self {
        let json = serde_json::to_vec(&value).expect("Failed to serialize JSON");
        self.set(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        self.send(json)
    }

    /// Sends a file from disk to the client.  
    ///
    /// If the file has already been cached in memory (based on path), it is served directly from the cache.
    /// Otherwise, the file is read from disk.  
    ///
    /// For performance optimization, if the file size exceeds 512 KB, it attempts to use memory-mapped I/O
    /// via the `memmap2` crate to avoid extra allocations and reduce read overhead.
    ///
    /// ⚠️ This function uses `unsafe` internally when invoking `memmap2::Mmap::map`, as required by the API.  
    /// The memory-mapped content is copied into an owned buffer (`Bytes`) before being cached and sent, which ensures safety.
    ///
    /// In all cases, the file is cached for future requests, and the correct `Content-Type` is inferred using the file extension.
    pub fn send_file(&mut self, path: &str) -> &mut Self {
        // Check cache
        if let Some(cached) = FILE_CACHE.lock().unwrap().get(path) {
            return self.send(cached.as_ref());
        }

        match File::open(path) {
            Ok(file) => {
                let metadata = match file.metadata() {
                    Ok(m) => m,
                    Err(_) => {
                        return self
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .send("Failed to read metadata");
                    }
                };

                if metadata.len() > 512 * 1024 {
                    // Use mmap for files > 512KB
                    match unsafe { Mmap::map(&file) } {
                        Ok(mmap) => {
                            let bytes = Arc::new(Bytes::copy_from_slice(&mmap));
                            self.status(StatusCode::OK);
                            self.set(
                                header::CONTENT_TYPE,
                                HeaderValue::from_str(
                                    mime_guess::from_path(path)
                                        .first_or_octet_stream()
                                        .essence_str(),
                                )
                                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
                            );
                            return self.send(&*bytes);
                        }
                        Err(_) => {
                            return self
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .send("Failed to mmap file");
                        }
                    }
                }

                // Fallback: regular read
                match fs::read(path) {
                    Ok(data) => {
                        let bytes = Arc::new(Bytes::from(data));

                        FILE_CACHE
                            .lock()
                            .unwrap()
                            .put(path.to_string(), bytes.clone());

                        self.status(StatusCode::OK);
                        self.set(
                            header::CONTENT_TYPE,
                            HeaderValue::from_str(
                                mime_guess::from_path(path)
                                    .first_or_octet_stream()
                                    .essence_str(),
                            )
                            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
                        );
                        self.send(&*bytes)
                    }
                    Err(e) if e.kind() == ErrorKind::NotFound => {
                        self.status(StatusCode::NOT_FOUND).send("File not found")
                    }
                    Err(_) => self
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .send("Failed to read file"),
                }
            }
            Err(_) => self.status(StatusCode::NOT_FOUND).send("File not found"),
        }
    }

    /// Streams a custom body.
    pub fn stream<S>(&mut self, stream: S) -> &mut Self
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    {
        self.prepare_stream(stream, "application/octet-stream");
        self
    }

    /// Internal helper for stream setup.
    fn prepare_stream<S>(&mut self, stream: S, mime: &str)
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    {
        let wrapped_stream = stream.map(|r| {
            match r {
                Ok(bytes) => Ok(Frame::data(bytes)),
                Err(e) => {
                    eprintln!("Stream error: {}", e);
                    panic!("Streaming error occurred"); // You could also return a fallback here
                }
            }
        });

        let stream_body = StreamBody::new(wrapped_stream);

        self.status = StatusCode::OK;
        self.ended = true;
        self.body.clear();
        self.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(mime)
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        );
        self.streaming = Some(Box::pin(stream_body));
    }

    /// Marks the response as ended.
    pub fn end(&mut self) -> &mut Self {
        self.ended = true;
        self
    }

    /// Sends a redirect response to the given location.
    pub fn redirect(&mut self, location: impl AsRef<str>) -> &mut Self {
        self.status = StatusCode::FOUND; // Default 302
        self.location(location);
        self.end()
    }

    /// Sets the `Location` header.
    pub fn location(&mut self, url: impl AsRef<str>) -> &mut Self {
        self.set(LOCATION, sanitize_header_value(url.as_ref()));
        self
    }

    /// Sets the `Content-Type` header based on MIME string or extension.
    pub fn r#type(&mut self, mime: impl AsRef<str>) -> &mut Self {
        let mime_str = mime.as_ref();
        let mime_type = if mime_str.contains('/') {
            mime_str.to_string()
        } else {
            mime_guess::from_ext(mime_str)
                .first_raw()
                .unwrap_or("application/octet-stream")
                .to_string()
        };

        self.set(
            CONTENT_TYPE,
            HeaderValue::from_str(&mime_type)
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        );
        self
    }

    /// Suggests to the client that the response should be downloaded as a file.
    pub fn attachment(&mut self, filename: Option<&str>) -> &mut Self {
        if let Some(name) = filename {
            self.r#type(
                mime_guess::from_path(name)
                    .first_or_octet_stream()
                    .essence_str(),
            );
            self.set(
                CONTENT_DISPOSITION,
                HeaderValue::from_str(&format!("attachment; filename=\"{}\"", name)).unwrap(),
            );
        } else {
            self.set(CONTENT_DISPOSITION, HeaderValue::from_static("attachment"));
        }
        self
    }

    /// Sets the `Link` header for multiple related resources.
    pub fn links(&mut self, links: HashMap<&str, &str>) -> &mut Self {
        let value = links
            .into_iter()
            .map(|(rel, url)| format!("<{}>; rel=\"{}\"", url, rel))
            .collect::<Vec<_>>()
            .join(", ");

        if let Ok(val) = HeaderValue::from_str(&value) {
            self.set(LINK, val);
        }

        self
    }

    /// Converts the response into a Hyper `Response<Full<Bytes>>`.
    pub fn into_hyper(mut self) -> HyperResponse<Full<Bytes>> {
        fn build_error_response() -> HyperResponse<Full<Bytes>> {
            HyperResponse::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::from_static(b"Internal Server Error")))
                .unwrap()
        }

        if !self.ended {
            self.end();
        }

        let mut builder = HyperResponse::builder();

        let is_redirect = self.status.is_redirection();
        let has_location = self.headers.contains_key(LOCATION);

        let suppress_body = matches!(
            self.status,
            StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED
        );

        let body = if suppress_body {
            Bytes::new()
        } else {
            self.body.freeze()
        };

        if is_redirect && body.is_empty() && has_location {
            self.headers
                .insert(header::CONTENT_LENGTH, HeaderValue::from_static("0"));
        }

        for (key, value) in DEFAULT_HEADERS.iter() {
            self.headers.entry(key.clone()).or_insert(value.clone());
        }

        for (k, v) in std::mem::take(&mut self.headers) {
            if let Some(k) = k {
                builder = builder.header(k, v);
            }
        }

        builder
            .status(self.status)
            .body(Full::new(body))
            .unwrap_or_else(|_| build_error_response())
    }

    /// Converts the response into a streaming Hyper response.
    pub fn into_hyper_streaming(
        mut self,
    ) -> HyperResponse<Pin<Box<dyn Body<Data = Bytes, Error = hyper::Error> + Send>>> {
        fn build_error_response()
        -> HyperResponse<Pin<Box<dyn Body<Data = Bytes, Error = hyper::Error> + Send>>> {
            HyperResponse::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Box::pin(StreamBody::new(futures_util::stream::once(async {
                    Ok::<_, hyper::Error>(Frame::data(Bytes::from_static(b"Internal Server Error")))
                })))
                    as Pin<
                        Box<dyn Body<Data = Bytes, Error = hyper::Error> + Send>,
                    >)
                .unwrap()
        }

        if !self.ended {
            self.end();
        }

        let mut builder = HyperResponse::builder();

        let is_redirect = self.status.is_redirection();
        let has_location = self.headers.contains_key(header::LOCATION);

        if is_redirect && !has_location {
            self.set(header::LOCATION, HeaderValue::from_static("/"));
        }

        for (key, value) in DEFAULT_HEADERS.iter() {
            self.headers.entry(key.clone()).or_insert(value.clone());
        }

        for (k, v) in std::mem::take(&mut self.headers) {
            if let Some(k) = k {
                builder = builder.header(k, v);
            }
        }

        let status = self.status;
        let body: Pin<Box<dyn Body<Data = Bytes, Error = hyper::Error> + Send>> =
            if let Some(stream) = self.streaming {
                stream
            } else {
                let frozen = self.body.freeze();
                Box::pin(StreamBody::new(futures_util::stream::once(async {
                    Ok::<_, hyper::Error>(Frame::data(frozen))
                })))
            };

        builder
            .status(status)
            .body(body)
            .unwrap_or_else(|_| build_error_response())
    }
}

impl From<Response> for HyperResponse<Full<Bytes>> {
    fn from(resp: Response) -> Self {
        resp.into_hyper()
    }
}

fn sanitize_header_value(input: &str) -> HeaderValue {
    let cleaned: String = input
        .chars()
        .filter(|&c| c.is_ascii_graphic() || c == ' ')
        .collect();

    HeaderValue::from_str(&cleaned).unwrap_or_else(|_| HeaderValue::from_static("/"))
}

impl Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("body", &self.body)
            .field("headers", &self.headers)
            .field("ended", &self.ended)
            .finish()
    }
}
