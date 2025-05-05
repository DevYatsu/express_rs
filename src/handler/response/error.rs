use std::fmt::Debug;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResponseError {
    #[error("Invalid status code: {0}")]
    InvalidStatusCode(u16),

    #[error("Failed to serialize JSON: {0}")]
    JsonSerializationError(#[from] serde_json::Error),

    #[error("Failed to open file: {0}")]
    FileOpenError(#[from] std::io::Error),

    #[error("Failed to mmap file")]
    MmapError,

    #[error("Invalid header value")]
    InvalidHeaderValue(#[from] hyper::header::InvalidHeaderValue),
}
