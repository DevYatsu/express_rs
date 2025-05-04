use bytes::BytesMut;
use http_body_util::Full;
use hyper::{
    HeaderMap, Response as HyperResponse, StatusCode,
    body::Bytes,
    header::{HeaderName, HeaderValue, IntoHeaderName},
};
use std::str::FromStr;

#[derive(Debug, Clone, Default)]
pub struct Response {
    status: StatusCode,
    body: BytesMut,
    headers: HeaderMap,
    ended: bool,
}

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

    pub fn status(&mut self, status: u16) -> &mut Self {
        self.status = StatusCode::from_u16(status).expect("Invalid status code");
        self
    }

    pub fn status_text(&self) -> &'static str {
        self.status.canonical_reason().unwrap_or("Unknown")
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

        self.set(
            HeaderName::from_static("Content-Length"),
            HeaderValue::from(data.len()),
        );

        if self.headers.get("content-type").is_none() {
            // Best guess: plain text if it's utf8
            if std::str::from_utf8(data).is_ok() {
                self.set(
                    HeaderName::from_static("Content-Type"),
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                );
            } else {
                self.set(
                    HeaderName::from_static("Content-Type"),
                    HeaderValue::from_static("application/octet-stream"),
                );
            }
        }

        self.end()
    }

    pub fn json<T: serde::Serialize>(&mut self, value: T) -> &mut Self {
        let json = serde_json::to_vec(&value).expect("Failed to serialize JSON");
        self.set(
            HeaderName::from_static("Content-Type"),
            HeaderValue::from_static("application/json"),
        );
        self.send(json)
    }

    #[inline]
    pub fn end(&mut self) -> &mut Self {
        self.ended = true;
        self
    }

    pub fn redirect(&mut self, location: impl Into<HeaderValue>) -> &mut Self {
        self.status = StatusCode::FOUND;
        self.set(HeaderName::from_static("Location"), location);
        self.set(
            HeaderName::from_static("Content-Length"),
            HeaderValue::from_static("0"),
        );
        self.end()
    }

    pub fn r#type(&mut self, mime: impl Into<HeaderValue>) -> &mut Self {
        self.set(HeaderName::from_static("Content-Type"), mime);
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

        {
            let headers = std::mem::take(&mut self.headers);
            for (k, v) in headers {
                builder = builder.header(k.unwrap(), v);
            }
        }

        let body = self.body.freeze();

        let status = if body.is_empty() && self.status == StatusCode::OK {
            StatusCode::NO_CONTENT
        } else {
            self.status
        };

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
