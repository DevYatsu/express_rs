use super::{
    config::CookieAuthConfig,
    cookies::CookieHandler,
    error::{AuthError, AuthResult},
    jwt::JwtTokenValidator,
    session::SessionTokenValidator,
    user::{AuthLevel, AuthenticatedUser},
    validator::TokenValidator,
};
use crate::handler::{
    Request, Response,
    middleware::{Middleware, MiddlewareResult, next, stop},
};
use async_trait::async_trait;
use hyper::header::{HeaderValue, LOCATION};
use std::sync::Arc;

mod builder;
pub use builder::AuthMiddlewareBuilder;

/// Main authentication middleware
#[derive(Clone)]
pub struct AuthMiddleware {
    config: CookieAuthConfig,
    protected_routes: matchthem::Router<AuthLevel>,
    token_validator: Arc<dyn TokenValidator>,
}

impl AuthMiddleware {
    /// Creates a new `AuthMiddleware` with JWT validation
    pub fn with_jwt(
        config: CookieAuthConfig,
        protected_routes: matchthem::Router<AuthLevel>,
        jwt_validator: JwtTokenValidator,
    ) -> Self {
        Self {
            config,
            protected_routes,
            token_validator: Arc::new(jwt_validator),
        }
    }

    /// Creates a new `AuthMiddleware` with session validation
    pub fn with_sessions(
        config: CookieAuthConfig,
        protected_routes: matchthem::Router<AuthLevel>,
        validator: SessionTokenValidator,
    ) -> Self {
        Self {
            config,
            protected_routes,
            token_validator: Arc::new(validator),
        }
    }

    /// Creates a new `AuthMiddleware` with custom token validator
    pub fn with_validator(
        config: CookieAuthConfig,
        protected_routes: matchthem::Router<AuthLevel>,
        validator: Arc<dyn TokenValidator>,
    ) -> Self {
        Self {
            config,
            protected_routes,
            token_validator: validator,
        }
    }

    /// Builder pattern for easier configuration
    pub fn builder() -> AuthMiddlewareBuilder {
        AuthMiddlewareBuilder::new()
    }

    /// Checks if a path is protected and returns the required authorization level
    fn get_required_auth_level(&self, path: &str) -> Option<&AuthLevel> {
        // Find the longest matching prefix for more specific route matching
        self.protected_routes.at(path).ok().map(|m| m.value)
    }

    /// Validates token length against configured limits
    fn validate_token_length(&self, token: &str) -> bool {
        let len = token.len();
        len >= self.config.min_token_length && len <= self.config.max_token_length
    }

    /// Extracts and validates authentication token from request
    async fn extract_and_validate_token(
        &self,
        req: &Request,
    ) -> AuthResult<Option<AuthenticatedUser>> {
        let token = CookieHandler::get_cookie_value(req, &self.config.cookie_name)?;

        match token {
            Some(token) => {
                if !self.validate_token_length(&token) {
                    return Err(AuthError::InvalidToken);
                }

                let user = self.token_validator.validate_token(&token).await?;
                Ok(Some(user))
            }
            None => Ok(None),
        }
    }

    /// Checks if user has sufficient permissions for the required level
    fn check_permissions(&self, user: &AuthenticatedUser, required_level: &AuthLevel) -> bool {
        user.level >= *required_level
    }

    /// Creates a redirect response to the login page
    fn create_redirect_response(&self, res: &mut Response) -> MiddlewareResult {
        res.status_code(302);
        res.header(
            LOCATION,
            HeaderValue::from_str(&self.config.login_redirect)
                .unwrap_or_else(|_| HeaderValue::from_static("/login")),
        );
        stop()
    }

    /// Logs authentication attempts if logging is enabled
    fn log_auth_attempt(&self, path: &str, result: &Result<(), AuthError>) {
        if self.config.enable_logging {
            match result {
                Ok(_) => println!("Auth success for path: {}", path),
                Err(e) => println!("Auth failed for path {}: {}", path, e),
            }
        }
    }
}

impl std::fmt::Debug for AuthMiddleware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthMiddleware")
            .field("config", &self.config)
            .field("protected_routes", &self.protected_routes)
            .field("token_validator", &"<validator>")
            .finish()
    }
}

#[async_trait]
impl Middleware for AuthMiddleware {
    async fn call(&self, req: &mut Request, res: &mut Response) -> MiddlewareResult {
        let path = req.uri().path().to_owned();

        // Check if path requires authentication
        let required_level = match self.get_required_auth_level(&path) {
            Some(level) => level,
            None => return next(), // Path is not protected
        };

        // Attempt authentication
        let auth_result = self
            .extract_and_validate_token(req)
            .await
            .and_then(|user_opt| match user_opt {
                Some(user) => {
                    if self.check_permissions(&user, required_level) {
                        Ok(user)
                    } else {
                        Err(AuthError::InsufficientPermissions)
                    }
                }
                None => Err(AuthError::InvalidToken),
            });

        match auth_result {
            Ok(user) => {
                // Authentication successful - store user in request extensions
                req.extensions_mut().insert(user);
                self.log_auth_attempt(&path, &Ok(()));
                next()
            }
            Err(auth_error) => {
                // Authentication failed - redirect to login
                self.log_auth_attempt(&path, &Err(auth_error));
                self.create_redirect_response(res)
            }
        }
    }
}

// Utility functions for easy setup
impl AuthMiddleware {
    /// Quick setup for JWT-based authentication
    pub fn jwt_auth(
        jwt_validator: JwtTokenValidator,
        protected_routes: matchthem::Router<AuthLevel>,
    ) -> Self {
        Self::builder()
            .protect_routes(protected_routes)
            .build_with_jwt(jwt_validator)
    }

    /// Quick setup for session-based authentication
    pub fn session_auth(
        sessions: SessionTokenValidator,
        protected_routes: matchthem::Router<AuthLevel>,
    ) -> Self {
        Self::builder()
            .protect_routes(protected_routes)
            .build_with_sessions(sessions)
    }
}
