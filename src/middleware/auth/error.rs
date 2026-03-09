use thiserror::Error;

/// Error types for authentication middleware
#[derive(Debug, Clone, PartialEq, Error, Eq)]
pub enum AuthError {
    /// The token is invalid.
    #[error("Invalid authentication token")]
    InvalidToken,
    /// The user does not have sufficient permissions.
    #[error("Insufficient permissions")]
    InsufficientPermissions,
    /// The token has expired.
    #[error("Authentication token expired")]
    TokenExpired,
    /// The cookie header is malformed.
    #[error("Malformed cookie header")]
    MalformedCookie,
    /// The user could not be found.
    #[error("User not found")]
    UserNotFound,
    /// The cookie could not be parsed.
    #[error("Failed to parse cookie")]
    CookieParseError,
}

/// Result type for authentication operations
pub type AuthResult<T> = Result<T, AuthError>;
