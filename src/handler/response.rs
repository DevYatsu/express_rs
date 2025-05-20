use bytes::BytesMut;
use error::ResponseError;
use futures_core::Stream;
use futures_util::StreamExt;
use http_body_util::StreamBody;
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
use mime_guess;
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    fmt::Debug,
    fs::{self, File},
    num::NonZeroUsize,
    path::Path,
    pin::Pin,
    str::FromStr,
    sync::{Arc, Mutex},
};

pub mod error;

/// LRU in-memory cache for file responses.
///
/// Stores MIME type and byte content for recently sent static files. Avoids redundant
/// disk reads for frequently accessed assets. Max size: 128 entries.
static FILE_CACHE: Lazy<Mutex<LruCache<String, (String, Arc<Bytes>)>>> =
    Lazy::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(128).unwrap())));

#[derive(Default)]
/// Represents an HTTP response.
///
/// Supports common patterns such as sending JSON, streaming files, setting headers,
/// and returning specific status codes. The response can be either a full buffered body
/// or a streaming body depending on usage.
pub struct Response {
    status: StatusCode,
    body: BytesMut,
    headers: HeaderMap,
    ended: bool,
    streaming: Option<Pin<Box<dyn Body<Data = Bytes, Error = ResponseError> + Send>>>,
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
    //
    //
    // STATIC METHODS
    //
    //

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

    /// 200 OK
    pub fn ok() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::OK);
        res
    }

    /// 201 Created
    pub fn created() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::CREATED);
        res
    }

    /// 204 No Content
    pub fn no_content() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::NO_CONTENT);
        res
    }

    /// 301 Moved Permanently
    pub fn moved_permanently(location: &str) -> Self {
        let mut res = Self::new();
        res.status(StatusCode::MOVED_PERMANENTLY).location(location);
        res
    }

    /// 302 Found (temporary redirect)
    pub fn found(location: &str) -> Self {
        let mut res = Self::new();
        res.status(StatusCode::FOUND).location(location);
        res
    }

    /// 400 Bad Request
    pub fn bad_request() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::BAD_REQUEST);
        res
    }

    /// 401 Unauthorized
    pub fn unauthorized() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::UNAUTHORIZED);
        res
    }

    /// 403 Forbidden
    pub fn forbidden() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::FORBIDDEN);
        res
    }

    /// 404 Not Found
    pub fn not_found() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::NOT_FOUND);
        res
    }

    /// 405 Method Not Allowed
    pub fn method_not_allowed() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::METHOD_NOT_ALLOWED);
        res
    }

    /// 413 Payload Too Large
    pub fn payload_too_large() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::PAYLOAD_TOO_LARGE);
        res
    }

    /// 415 Unsupported Media Type
    pub fn unsupported_media_type() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        res
    }

    /// 500 Internal Server Error
    pub fn internal_error() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::INTERNAL_SERVER_ERROR);
        res
    }

    /// 502 Bad Gateway
    pub fn bad_gateway() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::BAD_GATEWAY);
        res
    }

    /// 503 Service Unavailable
    pub fn service_unavailable() -> Self {
        let mut res = Self::new();
        res.status(StatusCode::SERVICE_UNAVAILABLE);
        res
    }

    /// Create with any status
    pub fn with_status(status: StatusCode) -> Self {
        let mut res = Self::new();
        res.status(status);
        res
    }

    /// Creates a response with a custom status and body message.
    pub fn with_status_and_message(status: StatusCode, message: &'static str) -> Self {
        let mut res = Response::new();
        res.status(status).send(message);
        res
    }

    //
    //
    // BUILDER METHODS
    //
    //

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

    /// Attempts to set the HTTP status code from a `u16` value.
    ///
    /// Returns an error if the provided code is not a valid HTTP status code
    /// (e.g., not between 100 and 599).
    pub fn status_code(&mut self, status: u16) -> Result<&mut Self, ResponseError> {
        self.status =
            StatusCode::from_u16(status).map_err(|_| ResponseError::InvalidStatusCode(status))?;
        Ok(self)
    }

    /// Returns the canonical reason phrase for the current status code.
    pub fn status_text(&self) -> Option<&'static str> {
        self.status.canonical_reason()
    }

    /// Sets the status and sends the corresponding reason as body text.
    pub fn send_status(&mut self, code: u16) -> Result<&mut Self, ResponseError> {
        self.status_code(code)?;
        let message = self.status_text().unwrap();
        self.r#type("text/plain; charset=utf-8").send(message);

        Ok(self)
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
    pub fn send(&mut self, data: impl AsRef<[u8]>) -> () {
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
            let mime = Self::infer(data);

            self.r#type(mime);
        }

        if self.headers.get(CONTENT_TYPE).is_none() {
            self.r#type(if std::str::from_utf8(data).is_ok() {
                "text/plain; charset=utf-8"
            } else {
                "application/octet-stream"
            });
        }

        self.end()
    }

    /// Serializes a value to JSON and sends it as the response body.
    ///
    /// Returns an error if the value fails to serialize.
    /// Automatically sets the `Content-Type` to `application/json`.
    pub fn json<T: ?Sized + serde::Serialize>(&mut self, value: &T) -> Result<(), ResponseError> {
        let json = serde_json::to_vec(&value)?;
        self.set(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(self.send(json))
    }

    /// Sends a file from disk to the client.
    ///
    /// Returns an error if the file cannot be read, memory-mapped (if large),
    /// or if header insertion fails. Content type is inferred from the file extension.
    ///
    /// For performance optimization, if the file size exceeds 512 KB, it is streamed to the client
    pub fn send_file(&mut self, path: &str) -> Result<(), ResponseError> {
        // Check cache
        if let Some((mime, cached)) = FILE_CACHE.lock().unwrap().get(path) {
            self.r#type(mime);
            return Ok(self.send(cached.as_ref()));
        }

        let file = File::open(path)?;
        let metadata = file.metadata()?;

        let extension = Path::new(path).extension().and_then(|ext| ext.to_str());
        if extension.is_some() {
            self.r#type(extension.unwrap());
        }

        // For large files, use a streaming response
        if metadata.len() > 512 * 1024 {
            let stream = tokio_util::io::ReaderStream::new(tokio::fs::File::from_std(file))
                .map(|result| result.map(Bytes::from).map_err(|e| ResponseError::from(e)));

            self.status(StatusCode::OK);
            self.stream(stream)?;
            return Ok(());
        }

        let data = fs::read(path)?;
        let bytes = Arc::new(Bytes::from(data));

        let mime = if extension.is_some() {
            extension.unwrap()
        } else {
            Self::infer(&bytes)
        };

        FILE_CACHE
            .lock()
            .unwrap()
            .put(path.to_string(), (mime.to_owned(), bytes.clone()));

        self.status(StatusCode::OK);
        Ok(self.send(&*bytes))
    }

    /// Sets a streaming response body from a `Stream`.
    ///
    /// Returns an error if headers cannot be set.
    /// The stream must yield `Bytes` or `ResponseError` values.
    pub fn stream<S>(&mut self, stream: S) -> Result<&mut Self, ResponseError>
    where
        S: Stream<Item = Result<Bytes, ResponseError>> + Send + 'static,
    {
        let wrapped_stream = stream.map(|r| match r {
            Ok(bytes) => Ok(Frame::data(bytes)),
            Err(e) => Err(e),
        });

        let stream_body = StreamBody::new(wrapped_stream);

        self.status = StatusCode::OK;
        self.ended = true;
        self.body = BytesMut::new();

        self.headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_str(Self::infer(&self.body))
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        );

        self.streaming = Some(Box::pin(stream_body));
        Ok(self)
    }

    /// Marks the response as ended.
    pub fn end(&mut self) -> () {
        self.ended = true;
    }

    /// Sends a redirect response to the given location.
    pub fn redirect(&mut self, location: impl AsRef<str>) -> () {
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

    /// Converts the response into a streaming Hyper response.
    pub fn into_hyper(
        mut self,
    ) -> HyperResponse<Pin<Box<dyn Body<Data = Bytes, Error = ResponseError> + Send>>> {
        fn build_error_response()
        -> HyperResponse<Pin<Box<dyn Body<Data = Bytes, Error = ResponseError> + Send>>> {
            HyperResponse::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Box::pin(StreamBody::new(futures_util::stream::once(async {
                    Ok::<_, ResponseError>(Frame::data(Bytes::from_static(
                        b"Internal Server Error",
                    )))
                })))
                    as Pin<
                        Box<dyn Body<Data = Bytes, Error = ResponseError> + Send>,
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
        let body: Pin<Box<dyn Body<Data = Bytes, Error = ResponseError> + Send>> =
            if let Some(stream) = self.streaming {
                stream
            } else {
                let frozen = self.body.freeze();
                Box::pin(StreamBody::new(futures_util::stream::once(async {
                    Ok::<_, ResponseError>(Frame::data(frozen))
                })))
            };

        builder
            .status(status)
            .body(body)
            .unwrap_or_else(|_| build_error_response())
    }

    fn infer(data: &[u8]) -> &str {
        let mime = if let Some(kind) = infer::get(data) {
            kind.mime_type()
        } else if std::str::from_utf8(data).is_ok() {
            "text/plain; charset=utf-8"
        } else {
            "application/octet-stream"
        };

        mime
    }
}

/// Sanitizes a header value by removing unsafe characters.
///
/// Only ASCII graphic characters and spaces are allowed. Prevents header injection attacks.
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
