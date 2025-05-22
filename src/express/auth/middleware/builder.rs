use super::AuthMiddleware;
use crate::express::auth::{
    config::CookieAuthConfig, jwt::JwtTokenValidator, session::SessionTokenValidator,
    user::AuthLevel, validator::TokenValidator,
};
use std::sync::Arc;

/// Builder for creating AuthMiddleware instances
#[derive(Debug)]
pub struct AuthMiddlewareBuilder {
    config: CookieAuthConfig,
    protected_routes: matchthem::Router<AuthLevel>,
}

impl AuthMiddlewareBuilder {
    pub fn new() -> Self {
        Self {
            config: CookieAuthConfig::default(),
            protected_routes: matchthem::Router::new(),
        }
    }

    pub fn cookie_name(mut self, name: impl Into<String>) -> Self {
        self.config.cookie_name = name.into();
        self
    }

    pub fn login_redirect(mut self, redirect: impl Into<String>) -> Self {
        self.config.login_redirect = redirect.into();
        self
    }

    pub fn secure_cookies(mut self, secure: bool) -> Self {
        self.config.secure_cookies = secure;
        self
    }

    pub fn cookie_domain(mut self, domain: impl Into<String>) -> Self {
        self.config.cookie_domain = Some(domain.into());
        self
    }

    pub fn cookie_path(mut self, path: impl Into<String>) -> Self {
        self.config.cookie_path = path.into();
        self
    }

    pub fn same_site(mut self, same_site: cookie::SameSite) -> Self {
        self.config.same_site = Some(same_site);
        self
    }

    pub fn token_length_limits(mut self, min: usize, max: usize) -> Self {
        self.config.min_token_length = min;
        self.config.max_token_length = max;
        self
    }

    pub fn enable_logging(mut self, enable: bool) -> Self {
        self.config.enable_logging = enable;
        self
    }

    pub fn protect_route(mut self, route: impl Into<String>, level: AuthLevel) -> Self {
        self.protected_routes.insert(route.into(), level);
        self
    }

    pub fn protect_routes(mut self, routes: matchthem::Router<AuthLevel>) -> Self {
        self.protected_routes.merge(routes);
        self
    }

    pub fn build_with_jwt(self, jwt_validator: JwtTokenValidator) -> AuthMiddleware {
        AuthMiddleware::with_jwt(self.config, self.protected_routes, jwt_validator)
    }

    pub fn build_with_sessions(self, validator: SessionTokenValidator) -> AuthMiddleware {
        AuthMiddleware::with_sessions(self.config, self.protected_routes, validator)
    }

    pub fn build_with_validator(self, validator: Arc<dyn TokenValidator>) -> AuthMiddleware {
        AuthMiddleware::with_validator(self.config, self.protected_routes, validator)
    }
}

impl Default for AuthMiddlewareBuilder {
    fn default() -> Self {
        Self::new()
    }
}
