use std::borrow::Cow;
use bytes::Bytes;
use cookie::Cookie;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::header::{
    CONTENT_TYPE, EXPIRES, HeaderValue, IntoHeaderName, LOCATION, SERVER,
    SET_COOKIE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, X_XSS_PROTECTION,
};
use hyper::{StatusCode};
use lru::LruCache;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::io;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::fs::File;
use tokio::sync::Mutex;
use futures_util::StreamExt;
use http_body_util::StreamBody;
use hyper::body::Frame;
use tokio_util::io::ReaderStream;
use std::pin::Pin;

#[derive(Error, Debug)]
pub enum ResponseError {
    #[error("invalid status code: {0}")]
    InvalidStatusCode(u16),
    #[error("JSON serialization error: {0}")]
    JsonSerializationError(#[from] serde_json::Error),
    #[error("invalid header value: {0}")]
    InvalidHeaderValue(#[from] hyper::header::InvalidHeaderValue),
    #[error("file open error: {0}")]
    FileOpenError(#[from] io::Error),
    #[error("memory mapping error")]
    MmapError,
    #[error("body read error: {0}")]
    BodyReadError(String),
}

#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub headers: hyper::HeaderMap,
    pub body: ResponseBody,
    pub error: Option<ResponseError>,
}

pub enum ResponseBody {
    Empty,
    Buffered(Vec<Bytes>),
    Stream(Pin<Box<dyn futures_util::Stream<Item = Result<Frame<Bytes>, io::Error>> + Send + Sync>>),
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

static DEFAULT_HEADERS: Lazy<hyper::HeaderMap> = Lazy::new(|| {
    let mut map = hyper::HeaderMap::new();
    map.insert(EXPIRES, HeaderValue::from_static("0"));
    map.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    map.insert(X_FRAME_OPTIONS, HeaderValue::from_static("SAMEORIGIN"));
    map.insert(X_XSS_PROTECTION, HeaderValue::from_static("0"));
    map.insert(SERVER, HeaderValue::from_static(""));
    map
});

pub type ServerResponse = hyper::Response<BoxBody<Bytes, std::convert::Infallible>>;

pub trait ExpressResponse: Sized {
    fn status(self, status: StatusCode) -> Self;
    fn status_code(self, code: u16) -> Self;
    fn header<K, V>(self, key: K, value: V) -> Self
    where
        K: IntoHeaderName,
        V: Into<HeaderValue>;
    fn content_type<T: AsRef<str>>(self, mime_type: T) -> Self;
    fn location<T: AsRef<str>>(self, url: T) -> Self;
    fn body<T: Into<Bytes>>(self, data: T) -> Self;
    fn write<T: Into<Bytes>>(self, data: T) -> Self;
    fn send_text<T: Into<Cow<'static, str>>>(self, text: T) -> Self;
    fn send_html<T: Into<Cow<'static, str>>>(self, html: T) -> Self;
    fn send_json<T: Serialize>(self, data: &T) -> Self;
    fn redirect<T: AsRef<str>>(self, url: T) -> Self;
    fn cookie(self, cookie: Cookie<'_>) -> Self;
}

impl Response {
    pub fn new() -> Self {
        Self {
            status: StatusCode::OK,
            headers: hyper::HeaderMap::new(),
            body: ResponseBody::Empty,
            error: None,
        }
    }

    pub fn ok() -> Self {
        Self::new()
    }

    pub fn ok_json<T: Serialize>(data: &T) -> Self {
        Self::new().send_json(data)
    }

    pub fn ok_text<T: Into<Cow<'static, str>>>(text: T) -> Self {
        Self::new().send_text(text)
    }

    pub fn ok_html<T: Into<Cow<'static, str>>>(html: T) -> Self {
        Self::new().send_html(html)
    }

    pub fn redirect<T: AsRef<str>>(url: T) -> Self {
        Self::new().redirect(url)
    }

    pub fn not_found() -> Self {
        Self::new()
            .status(StatusCode::NOT_FOUND)
            .send_text("Not Found")
    }

    pub fn internal_server_error() -> Self {
        Self::new()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .send_text("Internal Server Error")
    }

    pub fn get_status(&self) -> StatusCode {
        self.status
    }

    pub fn into_hyper(mut self) -> ServerResponse {
        let body: BoxBody<Bytes, std::convert::Infallible> = match self.body {
            ResponseBody::Empty => Full::new(Bytes::new()).map_err(|n| match n {}).boxed(),
            ResponseBody::Buffered(mut chunks) => {
                if chunks.is_empty() {
                    Full::new(Bytes::new()).map_err(|n| match n {}).boxed()
                } else if chunks.len() == 1 {
                    Full::new(chunks.pop().unwrap()).map_err(|n| match n {}).boxed()
                } else {
                    let mut ret = bytes::BytesMut::with_capacity(chunks.iter().map(|c| c.len()).sum());
                    for chunk in chunks {
                        ret.extend_from_slice(&chunk);
                    }
                    Full::new(ret.freeze()).map_err(|n| match n {}).boxed()
                }
            }
            ResponseBody::Stream(stream) => {
                StreamBody::new(stream)
                    .map_err(|_e| {
                        // In practice, since ServerResponse expects Infallible,
                        // we can't propagate this error easily without changing types.
                        // For now we ignore it, but ideally we'd log it.
                        unreachable!("Stream error occurred but response body is Infallible")
                    })
                    .boxed()
            }
        };

        let mut hyper_res = hyper::Response::builder()
            .status(self.status)
            .body(body)
            .unwrap();

        let headers = hyper_res.headers_mut();
        // Insert default headers first, then user headers (user headers override defaults)
        for (k, v) in DEFAULT_HEADERS.iter() {
            headers.insert(k, v.clone());
        }
        headers.extend(self.headers.drain());

        hyper_res
    }

    pub fn respond_error(
        &mut self,
        status: u16,
        message: &str,
        json_body: serde_json::Value,
        json: bool,
    ) -> &mut Self {
        if json {
            self.content_type("application/json").send_json(&json_body);
        } else {
            self.content_type("text/plain; charset=utf-8")
                .send_text(message.to_string()); // Clone here because respond_error takes &str
        }

        self.status_code(status);
        self
    }

    async fn file<T: AsRef<str>>(mut self, path: T) -> Self {
        let path_str = path.as_ref();

        if let Some((mime, cached)) = FILE_CACHE.lock().await.get(path_str) {
            self = self.content_type(mime);
            self = self.body((**cached).clone());
            return self;
        }

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

        let content_type = if mime.starts_with("text/")
            || mime == "application/javascript"
            || mime == "application/json"
        {
            format!("{}; charset=utf-8", mime)
        } else {
            mime.to_string()
        };

        if metadata.len() < 1024 * 1024 {
            let bytes = match tokio::fs::read(path_str).await {
                Ok(b) => Bytes::from(b),
                Err(e) => {
                    self.error = Some(ResponseError::FileOpenError(e));
                    return self;
                }
            };

            FILE_CACHE.lock().await.put(
                path_str.to_string(),
                (content_type, Arc::new(bytes.clone())),
            );

            self.body(bytes)
        } else {
            let file = match File::open(path_str).await {
                Ok(f) => f,
                Err(e) => {
                    self.error = Some(ResponseError::FileOpenError(e));
                    return self;
                }
            };
            let stream = ReaderStream::new(file).map(|res| res.map(Frame::data));
            self.body = ResponseBody::Stream(Box::pin(stream));
            self
        }
    }

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

static FILE_CACHE: Lazy<Arc<Mutex<LruCache<String, (String, Arc<Bytes>)>>>> =
    Lazy::new(|| Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap()))));
