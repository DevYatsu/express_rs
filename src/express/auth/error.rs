use thiserror::Error;

/// Error types for authentication middleware
#[derive(Debug, Clone, PartialEq, Error, Eq)]
pub enum AuthError {
    #[error("Invalid authentication token")]
    InvalidToken,
    #[error("Insufficient permissions")]
    InsufficientPermissions,
    #[error("Authentication token expired")]
    TokenExpired,
    #[error("Malformed cookie header")]
    MalformedCookie,
    #[error("User not found")]
    UserNotFound,
    #[error("Failed to parse cookie")]
    CookieParseError,
}

/// Result type for authentication operations
pub type AuthResult<T> = Result<T, AuthError>;
