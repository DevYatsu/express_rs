use error::ResponseError;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{
    body::{ Bytes}, header::{
         HeaderValue, IntoHeaderName, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, EXPIRES, LOCATION, PRAGMA, SERVER, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, X_XSS_PROTECTION
    }, HeaderMap, Response as HyperResponse, StatusCode
};
use itoa::Buffer;
use log::info;
use lru::LruCache;
use mime_guess;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::{num::NonZeroUsize, path::Path, sync::Arc};
use tokio::{fs::File, io::AsyncReadExt, sync::Mutex};

pub mod error;

/// LRU in-memory cache for file responses.
///
/// Stores MIME type and byte content for recently sent static files. Avoids redundant
/// disk reads for frequently accessed assets. Max size: 128 entries.
static FILE_CACHE: Lazy<Mutex<LruCache<String, (String, Arc<Bytes>)>>> =
    Lazy::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(128).unwrap())));

/// Represents an HTTP response.
///
/// Supports common patterns such as sending JSON, streaming files, setting headers,
/// and returning specific status codes. The response can be either a full buffered body
/// or a streaming body depending on usage.
pub struct Response {
    status: StatusCode,
    body: ResponseBody,
    headers: HeaderMap,
}

#[derive(Default)]
enum ResponseBody {
    #[default]
    Empty,
    Buffered(Bytes),
}

/// Default security and cache-related headers injected into all responses.
///
/// Includes no-store caching, XSS and MIME-type protections, and disables frame embedding.
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
    /// Creates a new empty response with 200 OK status.
    pub fn new() -> Self {
        Self {
            status: StatusCode::OK,
            body: ResponseBody::Empty,
            headers: HeaderMap::with_capacity(8),
        }
    }

    /// Sets the HTTP status code (mutable).
    pub fn status(&mut self, status: StatusCode) -> &mut Self {
        self.status = status;
        self
    }

    /// Sets the HTTP status code from a u16 value (mutable).
    pub fn status_code(&mut self, code: u16) -> Result<&mut Self, ResponseError> {
        self.status =
            StatusCode::from_u16(code).map_err(|_| ResponseError::InvalidStatusCode(code))?;
        Ok(self)
    }

    /// Sets a header value (mutable).
    pub fn header<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: IntoHeaderName,
        V: Into<HeaderValue>,
    {
        self.headers.insert(key, value.into());
        self
    }

    /// Sets multiple headers from an iterator (mutable).
    pub fn headers<I, K, V>(&mut self, headers: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: IntoHeaderName,
        V: Into<HeaderValue>,
    {
        for (key, value) in headers {
            self.headers.insert(key, value.into());
        }
        self
    }

    /// Sets the Content-Type header (mutable).
    pub fn content_type<T: AsRef<str>>(&mut self, mime_type: T) -> &mut Self {
        match HeaderValue::from_str(mime_type.as_ref()) {
            Ok(val) => {
                self.headers.insert(CONTENT_TYPE, val);
            }
            Err(_) => {
                info!(
                    "Invalid Content-Type header value: {}, error silenced",
                    mime_type.as_ref()
                );
            }
        }
        self
    }

    /// Sets the Location header for redirects (mutable).
    pub fn location<T: AsRef<str>>(&mut self, url: T) -> &mut Self {
        self.headers
            .insert(LOCATION, sanitize_header_value(url.as_ref()));
        self
    }

    /// Sets response body from bytes (mutable).
    pub fn body<T: Into<Bytes>>(&mut self, data: T) -> &mut Self {
        let bytes = data.into();
        self.body = ResponseBody::Buffered(bytes);
        self
    }

    /// Extends response body from bytes (mutable).
    pub fn write<T: Into<Bytes>>(&mut self, data: T) -> &mut Self {
        let bytes = data.into();
        match &mut self.body {
            ResponseBody::Buffered(existing) => {
                *existing = [existing.as_ref(), bytes.as_ref()].concat().into();
            }
            _ => {
                self.body = ResponseBody::Buffered(bytes);
            }
        }

        self
    }

    /// Sets response body from a string (mutable).
    pub fn text<T: Into<String>>(&mut self, text: T) -> &mut Self {
        self.content_type("text/plain; charset=utf-8");
        self.body(text.into())
    }

    /// Sets response body from HTML string (mutable).
    pub fn html<T: Into<String>>(&mut self, html: T) -> &mut Self {
        self.content_type("text/html; charset=utf-8");
        self.body(html.into())
    }

    /// Serializes data to JSON and sets as response body (mutable).
    pub fn json<T: Serialize>(&mut self, data: &T) -> Result<&mut Self, ResponseError> {
        let json = serde_json::to_vec(data)?;
        self.content_type("application/json");
        Ok(self.body(json))
    }

    /// Sends a file as the response body (mutable).
    pub async fn file<T: AsRef<str>>(&mut self, path: T) -> Result<&mut Self, ResponseError> {
        let path_str = path.as_ref();

        // Check cache first
        if let Some((mime, cached)) = FILE_CACHE.lock().await.get(path_str) {
            self.content_type(mime);
            return Ok(self.body((**cached).clone()));
        }

        let file = File::open(path_str).await?;
        let metadata = file.metadata().await?;

        // Infer content type from extension
        let content_type = Path::new(path_str)
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| mime_guess::from_ext(ext).first_raw())
            .unwrap_or("application/octet-stream");

        self.content_type(content_type);

        let data = tokio::fs::read(path_str).await?;
        let bytes = Arc::new(Bytes::from(data));

        // buffer small files
        if metadata.len() < 512 * 1024 {
            // Cache the file
            FILE_CACHE.lock().await.put(
                path_str.to_string(),
                (content_type.to_string(), bytes.clone()),
            );

        }

        Ok(self.body((*bytes).clone()))

    }

    /// Sets Content-Disposition header for file downloads (mutable).
    pub fn attachment<T: AsRef<str>>(&mut self, filename: Option<T>) -> &mut Self {
        match filename {
            Some(name) => {
                let name_str = name.as_ref();
                self.headers.insert(
                    CONTENT_DISPOSITION,
                    HeaderValue::from_str(&format!("attachment; filename=\"{}\"", name_str))
                        .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
                );
            }
            None => {
                self.headers
                    .insert(CONTENT_DISPOSITION, HeaderValue::from_static("attachment"));
            }
        }
        self
    }

    /// Gets a header value by name.
    pub fn get_header(&self, key: &str) -> Option<&HeaderValue> {
        self.headers.get(key)
    }

    /// Returns the current status code.
    pub fn get_status(&self) -> StatusCode {
        self.status
    }

    /// Checks if the response has a body.
    pub fn has_body(&self) -> bool {
        !matches!(self.body, ResponseBody::Empty)
    }

    /// Converts the response into a Hyper response (consumes self).
    pub fn into_hyper(self) -> hyper::Response<http_body_util::Full<hyper::body::Bytes>> {
        let mut headers = self.headers;
        let status = self.status;

        // Add default headers
        for (key, value) in DEFAULT_HEADERS.iter() {
            headers.entry(key.clone()).or_insert(value.clone());
        }

        // Handle redirect without location
        if status.is_redirection() && !headers.contains_key(LOCATION) {
            headers.insert(LOCATION, HeaderValue::from_static("/"));
        }

        let body = match self.body {
            ResponseBody::Empty => {
                Full::new(Bytes::new())
            }
            ResponseBody::Buffered(bytes) => {
                // Set content length for buffered responses
                let mut buf = Buffer::new();
                let len_str = buf.format(bytes.len());
                if let Ok(val) = HeaderValue::from_str(len_str) {
                    headers.insert(CONTENT_LENGTH, val);
                }

                // Infer content type if not set
                if !headers.contains_key(CONTENT_TYPE) {
                    let content_type = infer_content_type(&bytes);
                    if let Ok(val) = HeaderValue::from_str(content_type) {
                        headers.insert(CONTENT_TYPE, val);
                    }
                }

                Full::new(bytes)
            }
        };

        let mut builder = HyperResponse::builder().status(status);

        for (key, value) in headers {
            if let Some(key) = key {
                builder = builder.header(key, value);
            }
        }

        builder
            .body(body)
            .unwrap_or_else(|_| {
                HyperResponse::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Full::new(Bytes::from_static(b"Internal Server Error")))
                    .unwrap()
            })
    }

}

impl Default for Response {
    fn default() -> Self {
        Self::new()
    }
}

// Convenience constructors for common response types (consume self for direct returns)
impl Response {
    /// 200 OK with empty body
    pub fn ok() -> Self {
        Self::new()
    }

    /// 201 Created
    pub fn created() -> Self {
        Self::new().with_status(StatusCode::CREATED)
    }

    /// 204 No Content
    pub fn no_content() -> Self {
        Self::new().with_status(StatusCode::NO_CONTENT)
    }

    /// 400 Bad Request
    pub fn bad_request() -> Self {
        Self::new().with_status(StatusCode::BAD_REQUEST)
    }

    /// 401 Unauthorized
    pub fn unauthorized() -> Self {
        Self::new().with_status(StatusCode::UNAUTHORIZED)
    }

    /// 403 Forbidden
    pub fn forbidden() -> Self {
        Self::new().with_status(StatusCode::FORBIDDEN)
    }

    /// 404 Not Found
    pub fn not_found() -> Self {
        Self::new().with_status(StatusCode::NOT_FOUND)
    }

    /// 500 Internal Server Error
    pub fn internal_server_error() -> Self {
        Self::new().with_status(StatusCode::INTERNAL_SERVER_ERROR)
    }

    /// 302 Found (temporary redirect) - consumes self for direct return
    pub fn redirect<T: AsRef<str>>(url: T) -> Self {
        Self::new()
            .with_status(StatusCode::FOUND)
            .with_location(url)
    }

    /// 301 Moved Permanently - consumes self for direct return
    pub fn permanent_redirect<T: AsRef<str>>(url: T) -> Self {
        Self::new()
            .with_status(StatusCode::MOVED_PERMANENTLY)
            .with_location(url)
    }

    // Builder methods that consume self (for direct returns like Express.js res.send())

    /// Sets status and consumes self for direct return
    fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Sets location and consumes self for direct return
    fn with_location<T: AsRef<str>>(mut self, url: T) -> Self {
        self.headers
            .insert(LOCATION, sanitize_header_value(url.as_ref()));
        self
    }

    /// Returns a JSON response with 200 OK status (consumes self for direct return)
    pub fn send_json<T: Serialize>(mut self, data: &T) -> Result<Self, ResponseError> {
        let json = serde_json::to_vec(data)?;
        self.headers
            .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        self.body = ResponseBody::Buffered(json.into());
        Ok(self)
    }

    /// Returns a text response with 200 OK status (consumes self for direct return)
    pub fn send_text<T: Into<String>>(mut self, text: T) -> Self {
        self.headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        self.body = ResponseBody::Buffered(text.into().into());
        self
    }

    /// Returns an HTML response with 200 OK status (consumes self for direct return)
    pub fn send_html<T: Into<String>>(mut self, html: T) -> Self {
        self.headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        self.body = ResponseBody::Buffered(html.into().into());
        self
    }

    /// Returns bytes response with 200 OK status (consumes self for direct return)
    pub fn send_bytes<T: Into<Bytes>>(mut self, bytes: T) -> Self {
        self.body = ResponseBody::Buffered(bytes.into());
        self
    }


/// Sends a file as response (consumes self for direct return)
pub async fn send_file<T: AsRef<str>>(mut self, path: T) -> Result<Self, ResponseError> {
    let path_str = path.as_ref();

    // Check cache first
    if let Some((mime, cached)) = FILE_CACHE.lock().await.get(path_str) {
        self.headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_str(mime)
                .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
        );
        self.headers.insert(
            CONTENT_LENGTH,
            HeaderValue::from_str(&cached.len().to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("0")),
        );
        self.body = ResponseBody::Buffered((**cached).clone());
        return Ok(self);
    }

    let mut file = File::open(path_str).await?;
    let metadata = file.metadata().await?;

    let content_type = Path::new(path_str)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| mime_guess::from_ext(ext).first_raw())
        .unwrap_or("application/octet-stream");

    self.headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );

    self.headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&metadata.len().to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );

    // Cache and buffer small files
    if metadata.len() < 512 * 1024 {
        let mut buf = vec![0u8; metadata.len() as usize];
        file.read_exact(&mut buf).await?;
        let bytes = Arc::new(Bytes::from(buf));

        FILE_CACHE.lock().await.put(
            path_str.to_string(),
            (content_type.to_string(), bytes.clone()),
        );

        self.body = ResponseBody::Buffered((*bytes).clone());
    } else {
        // ❗ Optionally: implement ResponseBody::Stream variant for real streaming
        let mut buf = vec![0u8; metadata.len() as usize];
        file.read_exact(&mut buf).await?;
        self.body = ResponseBody::Buffered(Bytes::from(buf));
    }

    Ok(self)
}
}

// Additional convenience methods for common use cases (consume self for direct returns)
impl Response {
    /// Returns a JSON response with 200 OK status
    pub fn ok_json<T: Serialize>(data: &T) -> Result<Self, ResponseError> {
        Self::ok().send_json(data)
    }

    /// Returns a text response with 200 OK status
    pub fn ok_text<T: Into<String>>(text: T) -> Self {
        Self::ok().send_text(text)
    }

    /// Returns an HTML response with 200 OK status
    pub fn ok_html<T: Into<String>>(html: T) -> Self {
        Self::ok().send_html(html)
    }

    /// Returns a 404 Not Found with custom message
    pub fn not_found_text<T: Into<String>>(message: T) -> Self {
        Self::not_found().send_text(message)
    }

    /// Returns a 400 Bad Request with custom message
    pub fn bad_request_text<T: Into<String>>(message: T) -> Self {
        Self::bad_request().send_text(message)
    }

    /// Returns a 500 Internal Server Error with custom message
    pub fn internal_error_text<T: Into<String>>(message: T) -> Self {
        Self::internal_server_error().send_text(message)
    }
}

/// Sanitizes a header value by removing unsafe characters.
fn sanitize_header_value(input: &str) -> HeaderValue {
    let cleaned: String = input
        .chars()
        .filter(|&c| c.is_ascii_graphic() || c == ' ')
        .collect();

    HeaderValue::from_str(&cleaned).unwrap_or_else(|_| HeaderValue::from_static("/"))
}

/// Infers content type from byte content.
fn infer_content_type(data: &[u8]) -> &str {
    if let Some(kind) = infer::get(data) {
        kind.mime_type()
    } else if std::str::from_utf8(data).is_ok() {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

// Implement From traits for easy conversion from common types
impl From<String> for Response {
    fn from(text: String) -> Self {
        Response::ok_text(text)
    }
}

impl From<&str> for Response {
    fn from(text: &str) -> Self {
        Response::ok_text(text)
    }
}

impl From<Bytes> for Response {
    fn from(bytes: Bytes) -> Self {
        Response::ok().send_bytes(bytes)
    }
}

impl From<Vec<u8>> for Response {
    fn from(bytes: Vec<u8>) -> Self {
        Response::ok().send_bytes(bytes)
    }
}

impl From<StatusCode> for Response {
    fn from(status: StatusCode) -> Self {
        Response::new().with_status(status)
    }
}

// Error conversion
impl From<ResponseError> for HyperResponse<String> {
    fn from(err: ResponseError) -> Self {
        let status = match err {
            ResponseError::InvalidStatusCode(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ResponseError::JsonSerializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ResponseError::InvalidHeaderValue(_) => StatusCode::BAD_REQUEST,
            ResponseError::FileOpenError(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
                StatusCode::NOT_FOUND
            }
            ResponseError::FileOpenError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ResponseError::MmapError => StatusCode::INTERNAL_SERVER_ERROR,
        };

        HyperResponse::builder()
            .status(status)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(err.to_string())
            .unwrap()
    }
}
