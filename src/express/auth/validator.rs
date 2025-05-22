use async_trait::async_trait;

use super::{error::AuthResult, user::AuthenticatedUser};

/// Trait for token validation strategies
#[async_trait]
pub trait TokenValidator: Send + Sync {
    /// Validates a token and returns the associated user
    async fn validate_token(&self, token: &str) -> AuthResult<AuthenticatedUser>;

    /// Checks if a token meets basic format requirements
    fn is_valid_format(&self, token: &str) -> bool;

    /// Optional: Refresh token if needed
    async fn refresh_token(&self, _token: &str) -> AuthResult<Option<String>> {
        Ok(None)
    }
}
