use bytes::Bytes;
use cookie::Cookie;
use futures_util::StreamExt;
use http_body_util::StreamBody;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::StatusCode;
use hyper::body::Frame;
use hyper::header::{CONTENT_TYPE, HeaderValue, IntoHeaderName, LOCATION, SET_COOKIE};
use once_cell::sync::Lazy;
use quick_cache::sync::Cache;
use serde::Serialize;
use std::borrow::Cow;
use std::io;
use std::path::Path;
use std::pin::Pin;
use thiserror::Error;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

/// Represents an error that occurs during response building or handling.
#[derive(Error, Debug)]
pub enum ResponseError {
    /// Invalid HTTP status code.
    #[error("invalid status code: {0}")]
    InvalidStatusCode(u16),
    /// Error serializing JSON body.
    #[error("JSON serialization error: {0}")]
    JsonSerializationError(#[from] serde_json::Error),
    /// Error due to an invalid HTTP header value.
    #[error("invalid header value: {0}")]
    InvalidHeaderValue(#[from] hyper::header::InvalidHeaderValue),
    /// Error opening a file for a response.
    #[error("file open error: {0}")]
    FileOpenError(#[from] io::Error),
    /// Error reading memory mapped file.
    #[error("memory mapping error")]
    MmapError,
    /// Error reading request body.
    #[error("body read error: {0}")]
    BodyReadError(String),
}

/// The response structure used to construct HTTP responses.
#[derive(Debug)]
pub struct Response {
    /// The HTTP status code.
    pub status: StatusCode,
    /// HTTP headers for the response.
    pub headers: hyper::HeaderMap,
    /// The body of the response.
    pub body: ResponseBody,
    /// Any error that occurred while processing the response.
    pub error: Option<ResponseError>,
}

/// The body of an HTTP response.
#[derive(Default)]
pub enum ResponseBody {
    /// An empty body.
    #[default]
    Empty,
    /// A single chunk body.
    Full(Bytes),
    /// A body buffered in memory with multiple chunks.
    Buffered(Vec<Bytes>),
    /// A streaming body.
    Stream(
        Pin<Box<dyn futures_util::Stream<Item = Result<Frame<Bytes>, io::Error>> + Send + Sync>>,
    ),
}

impl std::fmt::Debug for ResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseBody::Empty => write!(f, "Empty"),
            ResponseBody::Full(bytes) => f.debug_tuple("Full").field(bytes).finish(),
            ResponseBody::Buffered(chunks) => f.debug_tuple("Buffered").field(chunks).finish(),
            ResponseBody::Stream(_) => write!(f, "Stream(...)"),
        }
    }
}
impl ResponseBody {
    /// Returns `true` if the body is completely empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, ResponseBody::Empty)
    }
}

impl Default for Response {
    fn default() -> Self {
        Self::new()
    }
}

/// Shorthand type for the hyper service response type.
pub type ServerResponse = hyper::Response<BoxBody<Bytes, std::convert::Infallible>>;

/// Trait providing Express-like response builder methods.
pub trait ExpressResponse: Sized {
    /// Sets the HTTP status code.
    fn status(self, status: StatusCode) -> Self;
    /// Sets the HTTP status code from a `u16`.
    fn status_code(self, code: u16) -> Self;
    /// Sets an HTTP header.
    fn header<K, V>(self, key: K, value: V) -> Self
    where
        K: IntoHeaderName,
        V: Into<HeaderValue>;
    /// Sets the `Content-Type` header.
    fn content_type<T: AsRef<str>>(self, mime_type: T) -> Self;
    /// Sets the `Location` header.
    fn location<T: AsRef<str>>(self, url: T) -> Self;
    /// Sets the response body entirely.
    fn body<T: Into<Bytes>>(self, data: T) -> Self;
    /// Appends data to the response body.
    fn write<T: Into<Bytes>>(self, data: T) -> Self;
    /// Sends a plain text response.
    fn send_text<T: Into<Cow<'static, str>>>(self, text: T) -> Self;
    /// Sends an HTML response.
    fn send_html<T: Into<Cow<'static, str>>>(self, html: T) -> Self;
    /// Sends a JSON response.
    fn send_json<T: Serialize>(self, data: &T) -> Self;
    /// Sends a redirect response.
    fn redirect<T: AsRef<str>>(self, url: T) -> Self;
    /// Sets a cookie.
    fn cookie(self, cookie: Cookie<'_>) -> Self;
    /// Sets status to 200 OK.
    fn ok(self) -> Self;
    /// Sends a JSON response (alias for `send_json`).
    fn json<T: Serialize>(self, data: &T) -> Self;
    /// Sends a plain text response (alias for `send_text`).
    fn text<T: Into<Cow<'static, str>>>(self, text: T) -> Self;
    /// Sends an HTML response (alias for `send_html`).
    fn html<T: Into<Cow<'static, str>>>(self, html: T) -> Self;
}

impl Response {
    /// Creates a new, empty HTTP 200 OK response.
    #[inline]
    pub fn new() -> Self {
        Self {
            status: StatusCode::OK,
            headers: hyper::HeaderMap::new(),
            body: ResponseBody::Empty,
            error: None,
        }
    }

    /// Creates a redirect response to the specified URL.
    #[inline]
    pub fn redirect<T: AsRef<str>>(url: T) -> Self {
        Self::new().redirect(url)
    }

    /// Creates an HTTP 404 Not Found response.
    #[inline]
    pub fn not_found() -> Self {
        Self::new()
            .status(StatusCode::NOT_FOUND)
            .send_text("Not Found")
    }

    /// Creates an HTTP 500 Internal Server Error response.
    pub fn internal_server_error() -> Self {
        Self::new()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .send_text("Internal Server Error")
    }

    /// Gets the current HTTP status of the response.
    #[inline]
    pub fn get_status(&self) -> StatusCode {
        self.status
    }

    /// Converts this `Response` builder into a standard hyper response.
    pub fn into_hyper(self) -> ServerResponse {
        let body: BoxBody<Bytes, std::convert::Infallible> = match self.body {
            ResponseBody::Empty => Full::new(Bytes::new()).map_err(|n| match n {}).boxed(),
            ResponseBody::Full(bytes) => Full::new(bytes).map_err(|n| match n {}).boxed(),
            ResponseBody::Buffered(mut chunks) => {
                let len = chunks.len();
                if len == 0 {
                    Full::new(Bytes::new()).map_err(|n| match n {}).boxed()
                } else if len == 1 {
                    Full::new(chunks.pop().unwrap())
                        .map_err(|n| match n {})
                        .boxed()
                } else {
                    let total_capacity = chunks.iter().map(|c| c.len()).sum();
                    let mut ret = bytes::BytesMut::with_capacity(total_capacity);
                    for chunk in chunks {
                        ret.extend_from_slice(&chunk);
                    }
                    Full::new(ret.freeze()).map_err(|n| match n {}).boxed()
                }
            }
            ResponseBody::Stream(stream) => StreamBody::new(stream)
                .map_err(|_e| unreachable!("Stream error in Infallible response"))
                .boxed(),
        };

        let mut builder = hyper::Response::builder().status(self.status);

        if let Some(headers) = builder.headers_mut() {
            *headers = self.headers;
        }

        builder.body(body).unwrap()
    }

    /// Populate `self` with an error status, content-type and body.
    pub fn respond_error(
        &mut self,
        status: u16,
        message: &str,
        json_body: serde_json::Value,
        json: bool,
    ) -> &mut Self {
        if let Ok(s) = StatusCode::from_u16(status) {
            self.status = s;
        } else {
            self.error = Some(ResponseError::InvalidStatusCode(status));
            self.status = StatusCode::INTERNAL_SERVER_ERROR;
        }

        if json {
            self.send_json(&json_body)
        } else {
            // If it's a simple error response, we can often avoid the String allocation if it's static
            // but since we get &str, we have to clone unless we want to change API to Cow
            self.send_text(message.to_owned())
        }
    }

    async fn file<T: AsRef<str>>(mut self, path: T) -> Self {
        let path_str = path.as_ref();

        // Fast path: concurrent lock-free cache hit.
        if let Some((header_val, cached)) = FILE_CACHE.get(path_str) {
            self.headers.insert(CONTENT_TYPE, header_val);
            self.body = ResponseBody::Full(cached);
            return self;
        }

        match self.load_file(path_str).await {
            Ok((header_val, body)) => {
                if let ResponseBody::Full(ref bytes) = body {
                    FILE_CACHE.insert(path_str.to_owned(), (header_val.clone(), bytes.clone()));
                }
                self.headers.insert(CONTENT_TYPE, header_val);
                self.body = body;
                self
            }
            Err(e) => {
                self.error = Some(e);
                self
            }
        }
    }

    async fn load_file(
        &self,
        path_str: &str,
    ) -> Result<(HeaderValue, ResponseBody), ResponseError> {
        let file = File::open(path_str).await?;
        let metadata = file.metadata().await?;

        let mime = Path::new(path_str)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(ext_to_mime)
            .unwrap_or("application/octet-stream");

        let header_val = if mime.starts_with("text/")
            || mime == "application/javascript"
            || mime == "application/json"
        {
            // Optimize common charset inclusions
            let mut s = String::with_capacity(mime.len() + 16);
            s.push_str(mime);
            s.push_str("; charset=utf-8");
            mime_to_header_value(&s).unwrap()
        } else {
            mime_to_header_value(mime).unwrap()
        };

        if metadata.len() < 1024 * 1024 {
            let bytes = tokio::fs::read(path_str).await?;
            Ok((header_val, ResponseBody::Full(Bytes::from(bytes))))
        } else {
            let stream = ReaderStream::new(file).map(|res| res.map(Frame::data));
            Ok((header_val, ResponseBody::Stream(Box::pin(stream))))
        }
    }

    /// Sends a file as the response.
    pub async fn send_file<T: AsRef<str>>(self, path: T) -> Self {
        self.file(path).await
    }
}

macro_rules! impl_express_response {
    ($target:ty) => {
        impl ExpressResponse for $target {
            #[inline]
            #[allow(unused_mut)]
            fn status(mut self, status: StatusCode) -> Self {
                self.status = status;
                self
            }

            #[inline]
            #[allow(unused_mut)]
            fn status_code(mut self, code: u16) -> Self {
                match StatusCode::from_u16(code) {
                    Ok(s) => self.status = s,
                    Err(_) => self.error = Some(ResponseError::InvalidStatusCode(code)),
                }
                self
            }

            #[inline]
            #[allow(unused_mut)]
            fn header<K, V>(mut self, key: K, value: V) -> Self
            where
                K: IntoHeaderName,
                V: Into<HeaderValue>,
            {
                self.headers.insert(key, value.into());
                self
            }

            #[inline]
            #[allow(unused_mut)]
            fn content_type<T: AsRef<str>>(mut self, mime_type: T) -> Self {
                if let Some(val) = mime_to_header_value(mime_type.as_ref()) {
                    self.headers.insert(CONTENT_TYPE, val);
                }
                self
            }

            #[inline]
            #[allow(unused_mut)]
            fn location<T: AsRef<str>>(mut self, url: T) -> Self {
                self.headers
                    .insert(LOCATION, sanitize_header_value(url.as_ref()));
                self
            }

            #[inline]
            #[allow(unused_mut)]
            fn body<T: Into<Bytes>>(mut self, data: T) -> Self {
                self.body = ResponseBody::Full(data.into());
                self
            }

            #[allow(unused_mut)]
            fn write<T: Into<Bytes>>(mut self, data: T) -> Self {
                let bytes = data.into();
                match std::mem::replace(&mut self.body, ResponseBody::Empty) {
                    ResponseBody::Empty => {
                        self.body = ResponseBody::Full(bytes);
                    }
                    ResponseBody::Full(prev) => {
                        self.body = ResponseBody::Buffered(vec![prev, bytes]);
                    }
                    ResponseBody::Buffered(mut chunks) => {
                        chunks.push(bytes);
                        self.body = ResponseBody::Buffered(chunks);
                    }
                    _ => {
                        // If it was a stream or something else, we overwrite it.
                        // Or we could decide to keep it, but body/write usually imply replacement or appending to buffer.
                        self.body = ResponseBody::Full(bytes);
                        // Logically if it was a stream we're dropping it.
                    }
                }
                self
            }

            #[inline]
            fn send_text<T: Into<Cow<'static, str>>>(self, text: T) -> Self {
                let bytes = cow_to_bytes(text.into());
                self.content_type("text/plain; charset=utf-8").body(bytes)
            }

            #[inline]
            fn send_html<T: Into<Cow<'static, str>>>(self, html: T) -> Self {
                let bytes = cow_to_bytes(html.into());
                self.content_type("text/html; charset=utf-8").body(bytes)
            }

            #[allow(unused_mut)]
            fn send_json<T: Serialize>(self, data: &T) -> Self {
                match serde_json::to_vec(data) {
                    Ok(json) => self.content_type("application/json").body(json),
                    Err(e) => {
                        let mut s = self;
                        s.error = Some(ResponseError::JsonSerializationError(e));
                        s
                    }
                }
            }

            #[inline]
            #[allow(unused_mut)]
            fn redirect<T: AsRef<str>>(mut self, url: T) -> Self {
                self.status = StatusCode::FOUND;
                self.location(url)
            }

            #[inline]
            #[allow(unused_mut)]
            fn cookie(mut self, cookie: Cookie<'_>) -> Self {
                if let Ok(val) = HeaderValue::from_str(&cookie.to_string()) {
                    self.headers.append(SET_COOKIE, val);
                }
                self
            }

            #[inline]
            fn ok(self) -> Self {
                self.status(StatusCode::OK)
            }

            #[inline]
            fn json<T: Serialize>(self, data: &T) -> Self {
                self.send_json(data)
            }

            #[inline]
            fn text<T: Into<Cow<'static, str>>>(self, text: T) -> Self {
                self.send_text(text)
            }

            #[inline]
            fn html<T: Into<Cow<'static, str>>>(self, html: T) -> Self {
                self.send_html(html)
            }
        }
    };
}

fn cow_to_bytes(cow: Cow<'static, str>) -> Bytes {
    match cow {
        Cow::Borrowed(s) => Bytes::from_static(s.as_bytes()),
        Cow::Owned(s) => Bytes::from(s),
    }
}

/// Fast lookup for common MIME types to avoid `HeaderValue::from_str` validation and allocation.
#[inline]
fn mime_to_header_value(mime: &str) -> Option<HeaderValue> {
    match mime {
        "application/json" => Some(HeaderValue::from_static("application/json")),
        "text/plain; charset=utf-8" => Some(HeaderValue::from_static("text/plain; charset=utf-8")),
        "text/html; charset=utf-8" => Some(HeaderValue::from_static("text/html; charset=utf-8")),
        "text/css; charset=utf-8" => Some(HeaderValue::from_static("text/css; charset=utf-8")),
        "application/javascript; charset=utf-8" => Some(HeaderValue::from_static(
            "application/javascript; charset=utf-8",
        )),
        "text/plain" => Some(HeaderValue::from_static("text/plain; charset=utf-8")),
        "text/html" => Some(HeaderValue::from_static("text/html; charset=utf-8")),
        "text/css" => Some(HeaderValue::from_static("text/css; charset=utf-8")),
        "application/javascript" => Some(HeaderValue::from_static(
            "application/javascript; charset=utf-8",
        )),
        _ => HeaderValue::from_str(mime).ok(),
    }
}

impl_express_response!(Response);
impl_express_response!(&mut Response);

fn sanitize_header_value(val: &str) -> HeaderValue {
    HeaderValue::from_str(val).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// Maps a file extension to its canonical MIME type string.
///
/// Returns `"application/octet-stream"` as the fallback — callers apply
/// `; charset=utf-8` for text types via [`mime_to_header_value`].
#[inline]
fn ext_to_mime(ext: &str) -> &'static str {
    match ext {
        // Text
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "txt" | "text" => "text/plain",
        "csv" => "text/csv",
        "xml" => "text/xml",
        "md" => "text/markdown",
        // Scripts
        "js" | "mjs" => "application/javascript",
        "ts" => "application/typescript",
        // Data
        "json" => "application/json",
        "jsonl" | "ndjson" => "application/x-ndjson",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        // Images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "tiff" | "tif" => "image/tiff",
        // Fonts
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        // Audio / Video
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        _ => "application/octet-stream",
    }
}

/// Concurrent, shard-sharded file cache.
static FILE_CACHE: Lazy<Cache<String, (HeaderValue, Bytes)>> = Lazy::new(|| Cache::new(100));


#[cfg(test)]
mod tests {
    use super::*;
    use hyper::StatusCode;

    #[test]
    fn test_response_builder() {
        let res = Response::new()
            .status(StatusCode::CREATED)
            .header("X-Test", HeaderValue::from_static("value"))
            .body("hello");

        assert_eq!(res.status, StatusCode::CREATED);
        assert_eq!(res.headers.get("X-Test").unwrap(), "value");
        match res.body {
            ResponseBody::Full(bytes) => assert_eq!(bytes, Bytes::from("hello")),
            _ => panic!("Expected full body"),
        }
    }

    #[test]
    fn test_response_json() {
        let data = serde_json::json!({"foo": "bar"});
        let res = Response::new().send_json(&data);

        assert_eq!(res.headers.get(CONTENT_TYPE).unwrap(), "application/json");
        match res.body {
            ResponseBody::Full(bytes) => assert_eq!(bytes, Bytes::from("{\"foo\":\"bar\"}")),
            _ => panic!("Expected full body"),
        }
    }

    #[test]
    fn test_respond_error_sets_body() {
        let mut res = Response::new();
        res.respond_error(
            429,
            "Rate limit exceeded",
            serde_json::json!({"error": "rate limited"}),
            false,
        );
        assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
        assert!(!res.body.is_empty(), "respond_error must set a body");
    }

    #[test]
    fn test_respond_error_json_sets_body() {
        let mut res = Response::new();
        res.respond_error(
            400,
            "bad request",
            serde_json::json!({"error": "bad"}),
            true,
        );
        assert_eq!(res.status, StatusCode::BAD_REQUEST);
        assert_eq!(res.headers.get(CONTENT_TYPE).unwrap(), "application/json");
        assert!(!res.body.is_empty());
    }

    #[test]
    fn test_response_status_code() {
        let res = Response::new().status_code(404);
        assert_eq!(res.status, StatusCode::NOT_FOUND);

        let res_err = Response::new().status_code(1000);
        assert!(res_err.error.is_some());
    }
}
