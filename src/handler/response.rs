use bytes::BytesMut;
use futures_core::Stream;
use futures_util::{StreamExt, TryStreamExt};
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
use mime_guess;
use once_cell::sync::Lazy;
use std::{collections::HashMap, fmt::Debug, pin::Pin, str::FromStr};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

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
    pub fn new() -> Self {
        Self {
            status: StatusCode::OK,
            body: BytesMut::with_capacity(512),
            headers: HeaderMap::with_capacity(8),
            ended: false,
            streaming: None,
        }
    }

    pub fn is_ended(&self) -> bool {
        self.ended
    }

    pub fn is_streaming(&self) -> bool {
        self.streaming.is_some()
    }

    pub fn status(&mut self, status: impl Into<StatusCode>) -> &mut Self {
        self.status = status.into();
        self
    }

    pub fn status_code(&mut self, status: u16) -> &mut Self {
        self.status = StatusCode::from_u16(status).expect("Expected valid status code");
        self
    }

    pub fn status_text(&self) -> &'static str {
        self.status.canonical_reason().unwrap_or("Unknown")
    }

    pub fn send_status(&mut self, code: u16) -> &mut Self {
        let status = self.status_text();
        self.status_code(code)
            .r#type("text/plain; charset=utf-8")
            .send(status)
    }

    pub fn set<K: IntoHeaderName, V: Into<HeaderValue>>(&mut self, key: K, val: V) -> &mut Self {
        self.headers.insert(key, val.into());
        self
    }

    pub fn get(&self, key: &str) -> Option<&HeaderValue> {
        HeaderName::from_str(key)
            .ok()
            .and_then(|k| self.headers.get(&k))
    }

    pub fn write(&mut self, data: impl AsRef<[u8]>) -> &mut Self {
        self.body.extend_from_slice(data.as_ref());
        self
    }

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

    pub fn json<T: serde::Serialize>(&mut self, value: T) -> &mut Self {
        let json = serde_json::to_vec(&value).expect("Failed to serialize JSON");
        self.set(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        self.send(json)
    }

    pub async fn send_file(&mut self, path: &str) -> &mut Self {
        match File::open(path).await {
            Ok(file) => {
                let stream = ReaderStream::new(file)
                    .map_ok(Bytes::from)
                    .map(|res| res.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));

                self.prepare_stream(
                    stream,
                    mime_guess::from_path(path)
                        .first_or_octet_stream()
                        .essence_str(),
                );
            }
            Err(_) => {
                self.status = StatusCode::NOT_FOUND;
                self.send("File not found");
            }
        }
        self
    }

    pub fn stream<S>(&mut self, stream: S) -> &mut Self
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    {
        self.prepare_stream(stream, "application/octet-stream");
        self
    }

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

    pub fn end(&mut self) -> &mut Self {
        self.ended = true;
        self
    }

    pub fn redirect(&mut self, location: impl AsRef<str>) -> &mut Self {
        self.status = StatusCode::FOUND; // Default 302
        self.location(location);
        self.end()
    }

    pub fn location(&mut self, url: impl AsRef<str>) -> &mut Self {
        self.set(LOCATION, sanitize_header_value(url.as_ref()));
        self
    }

    pub fn r#type(&mut self, mime: impl AsRef<str>) -> &mut Self {
        let ct = if mime.as_ref().contains('/') {
            mime.as_ref().to_string()
        } else {
            mime_guess::MimeGuess::from_ext(mime.as_ref())
                .first()
                .map(|m| m.to_string())
                .unwrap_or("application/octet-stream".to_string())
        };
        self.set(CONTENT_TYPE, HeaderValue::from_str(&ct).unwrap());
        self
    }

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
