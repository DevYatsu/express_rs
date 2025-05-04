use bytes::BytesMut;
use http_body_util::Full;
use hyper::{
    HeaderMap, Response as HyperResponse, StatusCode,
    body::Bytes,
    header::{
        self, CACHE_CONTROL, EXPIRES, HeaderName, HeaderValue, IntoHeaderName, PRAGMA, SERVER,
        X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, X_XSS_PROTECTION,
    },
};
use itoa::Buffer;
use once_cell::sync::Lazy;
use std::str::FromStr;

#[derive(Debug, Clone, Default)]
pub struct Response {
    status: StatusCode,
    body: BytesMut,
    headers: HeaderMap,
    ended: bool,
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
        Response {
            status: StatusCode::OK,
            body: BytesMut::with_capacity(512),
            headers: HeaderMap::with_capacity(8),
            ended: false,
        }
    }

    pub fn write_head(&mut self, status: u16) -> &mut Self {
        self.status = StatusCode::from_u16(status).expect("write_head status code must be valid");

        self
    }

    #[inline]
    pub fn is_ended(&self) -> bool {
        self.ended
    }

    #[inline]
    pub fn status(&mut self, status: impl Into<StatusCode>) -> &mut Self {
        self.status = status.into();
        self
    }

    #[inline]
    pub fn status_code(&mut self, status: u16) -> &mut Self {
        self.status = StatusCode::from_u16(status).expect("Expected status code in 100..=1000");
        self
    }

    #[inline]
    pub fn status_text(&self) -> &'static str {
        self.status.canonical_reason().unwrap_or("Unknown")
    }

    pub fn send_status(&mut self, code: u16) -> &mut Self {
        self.status_code(code);
        let message = self.status_text();
        self.r#type(HeaderValue::from_static("text/plain; charset=utf-8"))
            .send(message)
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

        // retrieve len and stringify it, faster than to_string
        let mut buf = Buffer::new();
        let len_str = buf.format(data.len());

        if let Ok(val) = HeaderValue::from_str(len_str) {
            self.set(header::CONTENT_LENGTH, val);
        }

        if self.headers.get("content-type").is_none() {
            if std::str::from_utf8(data).is_ok() {
                self.set(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                );
            } else {
                self.set(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
                );
            }
        }

        self.end()
    }

    pub fn send_static(&mut self, data: &'static [u8]) -> &mut Self {
        self.body.clear();
        self.body.extend_from_slice(data);

        // retrieve len and stringify it, faster than to_string
        let mut buf = Buffer::new();
        let len_str = buf.format(data.len());

        if let Ok(val) = HeaderValue::from_str(len_str) {
            self.set(header::CONTENT_LENGTH, val);
        }

        self.end()
    }

    pub fn json<T: serde::Serialize>(&mut self, value: T) -> &mut Self {
        let json = serde_json::to_vec(&value).expect("Failed to serialize JSON");
        self.set(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.send(json)
    }

    #[inline]
    pub fn end(&mut self) -> &mut Self {
        self.ended = true;
        self
    }

    pub fn redirect(&mut self, location: impl AsRef<str>) -> &mut Self {
        self.status = StatusCode::PERMANENT_REDIRECT;
        self.location(location);
        self.end()
    }

    pub fn location(&mut self, url: impl AsRef<str>) -> &mut Self {
        self.set(header::LOCATION, sanitize_header_value(url.as_ref()));
        self
    }

    #[inline]
    pub fn r#type(&mut self, mime: impl Into<HeaderValue>) -> &mut Self {
        self.set(header::CONTENT_TYPE, mime);
        self
    }

    pub fn attachment(&mut self, filename: Option<&str>) -> &mut Self {
        if let Some(name) = filename {
            self.r#type(
                HeaderValue::from_str(
                    mime_guess::from_path(name)
                        .first_or_octet_stream()
                        .essence_str(),
                )
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
            );
        }

        if filename.is_none() {
            self.set(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_static("attachment"),
            );
            return self;
        }

        self.set(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename.unwrap()))
                .unwrap_or(HeaderValue::from_static("attachment")),
        );
        self
    }

    pub fn links(&mut self, links: std::collections::HashMap<&str, &str>) -> &mut Self {
        let value = links
            .into_iter()
            .map(|(rel, url)| format!("<{}>; rel=\"{}\"", url, rel))
            .collect::<Vec<_>>()
            .join(", ");
        if let Ok(header_value) = HeaderValue::from_str(&value) {
            self.set(header::LINK, header_value);
        } else {
            eprintln!("Invalid header value for LINK: {}", value);
        }
        self
    }

    pub fn into_hyper(mut self) -> HyperResponse<Full<Bytes>> {
        #[inline(always)]
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
        let has_location = self.headers.contains_key(header::LOCATION);

        let suppress_body = matches!(
            self.status,
            StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED
        ) || self.status == StatusCode::NO_CONTENT;

        let body = if suppress_body {
            Bytes::new()
        } else {
            self.body.freeze()
        };

        let status = if body.is_empty() && self.status == StatusCode::OK {
            StatusCode::NO_CONTENT
        } else {
            self.status
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
            } else {
                eprintln!("Warning: skipping invalid header key");
            }
        }

        builder
            .status(status)
            .body(Full::new(body))
            .unwrap_or_else(|e| {
                eprintln!("SERVER ERROR, failed to build response: {}", e);
                build_error_response()
            })
    }
}

impl From<Response> for HyperResponse<Full<Bytes>> {
    fn from(resp: Response) -> Self {
        resp.into_hyper()
    }
}

#[inline]
fn sanitize_header_value(input: &str) -> HeaderValue {
    // Keep only visible ASCII (33 to 126)
    let cleaned: String = input
        .chars()
        .filter(|&c| c.is_ascii_graphic() || c == ' ') // ' ' is optional, often allowed
        .collect();

    HeaderValue::from_str(&cleaned).unwrap_or_else(|_| {
        eprintln!(
            "HeaderValue sanitization failed: input '{}', sanitized '{}'",
            input, cleaned
        );
        HeaderValue::from_static("/")
    })
}
