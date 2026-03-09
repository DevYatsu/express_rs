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
use std::sync::Arc;
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
pub enum ResponseBody {
    /// An empty body.
    Empty,
    /// A body buffered in memory.
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

impl Default for ResponseBody {
    fn default() -> Self {
        ResponseBody::Empty
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
}

impl Response {
    /// Creates a new, empty HTTP 200 OK response.
    pub fn new() -> Self {
        Self {
            status: StatusCode::OK,
            headers: hyper::HeaderMap::new(),
            body: ResponseBody::Empty,
            error: None,
        }
    }

    /// Creates a new HTTP 200 OK response. Alias for `new()`.
    pub fn ok() -> Self {
        Self::new()
    }

    /// Creates a new HTTP 200 OK response with a JSON body.
    pub fn ok_json<T: Serialize>(data: &T) -> Self {
        Self::new().send_json(data)
    }

    /// Creates a new HTTP 200 OK response with a text body.
    pub fn ok_text<T: Into<Cow<'static, str>>>(text: T) -> Self {
        Self::new().send_text(text)
    }

    /// Creates a new HTTP 200 OK response with an HTML body.
    pub fn ok_html<T: Into<Cow<'static, str>>>(html: T) -> Self {
        Self::new().send_html(html)
    }

    /// Creates a redirect response to the specified URL.
    pub fn redirect<T: AsRef<str>>(url: T) -> Self {
        Self::new().redirect(url)
    }

    /// Creates an HTTP 404 Not Found response.
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
    pub fn get_status(&self) -> StatusCode {
        self.status
    }

    /// Converts this `Response` builder into a standard hyper response.
    pub fn into_hyper(mut self) -> ServerResponse {
        let body: BoxBody<Bytes, std::convert::Infallible> = match self.body {
            ResponseBody::Empty => Full::new(Bytes::new()).map_err(|n| match n {}).boxed(),
            ResponseBody::Buffered(mut chunks) => {
                if chunks.is_empty() {
                    Full::new(Bytes::new()).map_err(|n| match n {}).boxed()
                } else if chunks.len() == 1 {
                    Full::new(chunks.pop().unwrap())
                        .map_err(|n| match n {})
                        .boxed()
                } else {
                    let mut ret =
                        bytes::BytesMut::with_capacity(chunks.iter().map(|c| c.len()).sum());
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

        let mut hyper_res = hyper::Response::builder()
            .status(self.status)
            .body(body)
            .unwrap();

        hyper_res.headers_mut().extend(self.headers.drain());
        hyper_res
    }

    /// Populate `self` with an error status, content-type and body.
    ///
    /// Previously this used the consuming builder on `&mut Self`, silently
    /// dropping the returned value. Now we build the finished response and
    /// assign it back with `*self = …` so the mutation actually takes effect.
    pub fn respond_error(
        &mut self,
        status: u16,
        message: &str,
        json_body: serde_json::Value,
        json: bool,
    ) -> &mut Self {
        let built = if json {
            Response::new()
                .status_code(status)
                .content_type("application/json")
                .send_json(&json_body)
        } else {
            Response::new()
                .status_code(status)
                .content_type("text/plain; charset=utf-8")
                .send_text(message.to_owned())
        };
        // Preserve any error that may have already been set.
        let prev_error = self.error.take();
        *self = built;
        if self.error.is_none() {
            self.error = prev_error;
        }
        self
    }

    async fn file<T: AsRef<str>>(mut self, path: T) -> Self {
        let path_str = path.as_ref();

        // Fast path: concurrent lock-free cache hit.
        if let Some((mime, cached)) = FILE_CACHE.get(path_str) {
            self = self.content_type(&*mime);
            self = self.body((*cached).clone());
            return self;
        }

        // Open the file once and reuse the handle for both metadata and content.
        let file = match File::open(path_str).await {
            Ok(f) => f,
            Err(e) => {
                self.error = Some(ResponseError::FileOpenError(e));
                return self;
            }
        };
        let metadata = match file.metadata().await {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(ResponseError::FileOpenError(e));
                return self;
            }
        };

        let mime = Path::new(path_str)
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| mime_guess::from_ext(ext).first_raw())
            .unwrap_or("application/octet-stream");

        let content_type: Arc<str> = if mime.starts_with("text/")
            || mime == "application/javascript"
            || mime == "application/json"
        {
            format!("{}: charset=utf-8", mime).into()
        } else {
            mime.into()
        };

        if metadata.len() < 1024 * 1024 {
            // Small file: buffer and cache.
            let bytes = match tokio::fs::read(path_str).await {
                Ok(b) => Bytes::from(b),
                Err(e) => {
                    self.error = Some(ResponseError::FileOpenError(e));
                    return self;
                }
            };
            let cached = Arc::new(bytes.clone());
            FILE_CACHE.insert(path_str.to_owned(), (Arc::clone(&content_type), cached));
            self.content_type(&*content_type).body(bytes)
        } else {
            // Large file: stream directly from the already-opened handle.
            let stream = ReaderStream::new(file).map(|res| res.map(Frame::data));
            self.body = ResponseBody::Stream(Box::pin(stream));
            self.content_type(&*content_type)
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
            #[allow(unused_mut)]
            fn status(mut self, status: StatusCode) -> Self {
                self.status = status;
                self
            }

            #[allow(unused_mut)]
            fn status_code(mut self, code: u16) -> Self {
                match StatusCode::from_u16(code) {
                    Ok(s) => self.status = s,
                    Err(_) => self.error = Some(ResponseError::InvalidStatusCode(code)),
                }
                self
            }

            #[allow(unused_mut)]
            fn header<K, V>(mut self, key: K, value: V) -> Self
            where
                K: IntoHeaderName,
                V: Into<HeaderValue>,
            {
                self.headers.insert(key, value.into());
                self
            }

            #[allow(unused_mut)]
            fn content_type<T: AsRef<str>>(mut self, mime_type: T) -> Self {
                if let Ok(val) = HeaderValue::from_str(mime_type.as_ref()) {
                    self.headers.insert(CONTENT_TYPE, val);
                }
                self
            }

            #[allow(unused_mut)]
            fn location<T: AsRef<str>>(mut self, url: T) -> Self {
                self.headers
                    .insert(LOCATION, sanitize_header_value(url.as_ref()));
                self
            }

            #[allow(unused_mut)]
            fn body<T: Into<Bytes>>(mut self, data: T) -> Self {
                self.body = ResponseBody::Buffered(vec![data.into()]);
                self
            }

            #[allow(unused_mut)]
            fn write<T: Into<Bytes>>(mut self, data: T) -> Self {
                let bytes = data.into();
                match &mut self.body {
                    ResponseBody::Buffered(chunks) => {
                        chunks.push(bytes);
                    }
                    _ => {
                        self.body = ResponseBody::Buffered(vec![bytes]);
                    }
                }
                self
            }

            fn send_text<T: Into<Cow<'static, str>>>(self, text: T) -> Self {
                let cow = text.into();
                let bytes = match cow {
                    Cow::Borrowed(s) => Bytes::from_static(s.as_bytes()),
                    Cow::Owned(s) => Bytes::from(s),
                };
                self.content_type("text/plain; charset=utf-8").body(bytes)
            }

            fn send_html<T: Into<Cow<'static, str>>>(self, html: T) -> Self {
                let cow = html.into();
                let bytes = match cow {
                    Cow::Borrowed(s) => Bytes::from_static(s.as_bytes()),
                    Cow::Owned(s) => Bytes::from(s),
                };
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

            #[allow(unused_mut)]
            fn redirect<T: AsRef<str>>(mut self, url: T) -> Self {
                self.status = StatusCode::FOUND;
                self.location(url)
            }

            #[allow(unused_mut)]
            fn cookie(mut self, cookie: Cookie<'_>) -> Self {
                if let Ok(val) = HeaderValue::from_str(&cookie.to_string()) {
                    self.headers.append(SET_COOKIE, val);
                }
                self
            }
        }
    };
}

impl_express_response!(Response);
impl_express_response!(&mut Response);

fn sanitize_header_value(val: &str) -> HeaderValue {
    HeaderValue::from_str(val).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// Concurrent, shard-sharded file cache — replaces the previous
/// `tokio::sync::Mutex<LruCache>` global bottleneck.
/// `quick_cache::sync::Cache` is lock-free on reads and uses fine-grained
/// sharding on writes.
static FILE_CACHE: Lazy<Cache<String, (Arc<str>, Arc<Bytes>)>> = Lazy::new(|| Cache::new(100));

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
        if let ResponseBody::Buffered(chunks) = res.body {
            assert_eq!(chunks[0], Bytes::from("hello"));
        } else {
            panic!("Expected buffered body");
        }
    }

    #[test]
    fn test_response_json() {
        let data = serde_json::json!({"foo": "bar"});
        let res = Response::new().send_json(&data);

        assert_eq!(res.headers.get(CONTENT_TYPE).unwrap(), "application/json");
        if let ResponseBody::Buffered(chunks) = res.body {
            assert_eq!(chunks[0], Bytes::from("{\"foo\":\"bar\"}"));
        } else {
            panic!("Expected buffered body");
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
