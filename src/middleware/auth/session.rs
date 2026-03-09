use super::{
    error::{AuthError, AuthResult},
    user::AuthenticatedUser,
    validator::TokenValidator,
};
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;

/// Session-based token validator with async support
#[derive(Debug, Clone)]
pub struct SessionTokenValidator {
    // In production, this would be a database connection or Redis client
    sessions: Arc<DashMap<String, SessionData>>,
}

#[derive(Debug, Clone)]
struct SessionData {
    user: AuthenticatedUser,
    expires_at: std::time::Instant,
    last_accessed: std::time::Instant,
}

impl SessionTokenValidator {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }

    pub async fn add_session(
        &self,
        token: String,
        user: AuthenticatedUser,
        ttl: std::time::Duration,
    ) {
        let now = std::time::Instant::now();
        let session_data = SessionData {
            user,
            expires_at: now + ttl,
            last_accessed: now,
        };

        self.sessions.insert(token, session_data);
    }

    pub async fn remove_session(&self, token: &str) {
        self.sessions.remove(token);
    }

    pub async fn cleanup_expired_sessions(&self) {
        let now = std::time::Instant::now();
        self.sessions.retain(|_, session| session.expires_at > now);
    }

    pub async fn update_last_accessed(&self, token: &str) -> AuthResult<()> {
        if let Some(mut session) = self.sessions.get_mut(token) {
            session.last_accessed = std::time::Instant::now();
            Ok(())
        } else {
            Err(AuthError::UserNotFound)
        }
    }
}

impl Default for SessionTokenValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TokenValidator for SessionTokenValidator {
    async fn validate_token(&self, token: &str) -> AuthResult<AuthenticatedUser> {
        if !self.is_valid_format(token) {
            return Err(AuthError::InvalidToken);
        }

        let now = std::time::Instant::now();

        // Clone the user *before* dropping the read-lock guard so we hold
        // the lock for the minimum time (no clone latency inside the lock).
        let user_opt = {
            self.sessions.get(token).map(|s| {
                if s.expires_at <= now {
                    Err(AuthError::TokenExpired)
                } else {
                    Ok(s.user.clone())
                }
            })
        }; // read-lock dropped here

        match user_opt {
            Some(result) => result,
            None => Err(AuthError::UserNotFound),
        }
    }

    fn is_valid_format(&self, token: &str) -> bool {
        !token.trim().is_empty()
            && token.len() >= 16
            && token.len() <= 1024
            && token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    }
}
