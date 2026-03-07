use async_trait::async_trait;

use super::{
    error::{AuthError, AuthResult},
    user::{AuthLevel, AuthenticatedUser},
    validator::TokenValidator,
};

/// JWT-based token validator
#[derive(Debug, Clone)]
pub struct JwtTokenValidator {
    secret: String,
    issuer: Option<String>,
    audience: Option<String>,
}

impl JwtTokenValidator {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
            issuer: None,
            audience: None,
        }
    }

    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }
}

#[async_trait]
impl TokenValidator for JwtTokenValidator {
    async fn validate_token(&self, token: &str) -> AuthResult<AuthenticatedUser> {
        if !self.is_valid_format(token) {
            return Err(AuthError::InvalidToken);
        }

        // TODO: Implement actual JWT validation with jsonwebtoken crate
        // This is a placeholder implementation
        // In production, you would:
        // 1. Parse the JWT token using jsonwebtoken::decode
        // 2. Verify the signature with the secret
        // 3. Check expiration, issuer, audience
        // 4. Extract user claims (id, role, etc.)

        // For now, simulate validation based on token format
        if token.starts_with("expired.") {
            return Err(AuthError::TokenExpired);
        }

        let auth_level = if token.contains("admin") {
            AuthLevel::Admin
        } else if token.contains("user") {
            AuthLevel::User
        } else {
            AuthLevel::Guest
        };

        Ok(AuthenticatedUser {
            token: token.to_string(),
            level: auth_level,
        })
    }

    fn is_valid_format(&self, token: &str) -> bool {
        // Basic JWT format check (3 parts separated by dots)
        let parts: Vec<&str> = token.split('.').collect();
        parts.len() == 3 && parts.iter().all(|part| !part.is_empty())
    }

    async fn refresh_token(&self, token: &str) -> AuthResult<Option<String>> {
        // TODO: Implement JWT refresh logic
        // Check if token is close to expiry and generate new one
        Ok(None)
    }
}
