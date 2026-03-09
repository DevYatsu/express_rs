use super::{config::CookieAuthConfig, error::{AuthError, AuthResult}};
use crate::handler::Request;
use cookie::{Cookie, CookieJar};
use hyper::header::COOKIE;

/// Cookie handling utility using the cookie crate
#[derive(Debug, Clone)]
pub struct CookieHandler;

impl CookieHandler {
    /// Extracts a specific cookie value from the request.
    pub fn get_cookie_value(req: &Request, cookie_name: &str) -> AuthResult<Option<String>> {
        let jar = Self::build_jar(req)?;
        Ok(jar.get(cookie_name).map(|c| c.value().to_string()))
    }

    /// Gets all cookies from the request as a `CookieJar`.
    pub fn get_all_cookies(req: &Request) -> AuthResult<CookieJar> {
        Self::build_jar(req)
    }

    /// Parses the `Cookie` header into a `CookieJar`.
    fn build_jar(req: &Request) -> AuthResult<CookieJar> {
        let mut jar = CookieJar::new();

        for cookie_header in req.headers().get_all(COOKIE) {
            let cookie_str = cookie_header
                .to_str()
                .map_err(|_| AuthError::MalformedCookie)?;

            for part in cookie_str.split(';') {
                let part = part.trim();
                if !part.is_empty() {
                    // Cookie::parse requires an owned String to avoid lifetime issues.
                    if let Ok(parsed) = Cookie::parse(part.to_owned()) {
                        jar.add_original(parsed);
                    }
                }
            }
        }

        Ok(jar)
    }

    /// Creates a new session cookie with the given config.
    pub fn create_session_cookie(
        name: &str,
        value: &str,
        config: &CookieAuthConfig,
        max_age: Option<std::time::Duration>,
    ) -> Cookie<'static> {
        let mut cookie = Cookie::build((name.to_owned(), value.to_owned()))
            .path(config.cookie_path.clone())
            .secure(config.secure_cookies)
            .http_only(true);

        if let Some(domain) = &config.cookie_domain {
            cookie = cookie.domain(domain.clone());
        }

        if let Some(same_site) = config.same_site {
            cookie = cookie.same_site(same_site);
        }

        if let Some(max_age) = max_age {
            cookie = cookie.max_age(cookie::time::Duration::seconds(max_age.as_secs() as i64));
        }

        cookie.build()
    }

    /// Creates a cookie that clears the session (logout).
    pub fn create_logout_cookie(name: &str, config: &CookieAuthConfig) -> Cookie<'static> {
        Cookie::build((name.to_owned(), ""))
            .path(config.cookie_path.clone())
            .secure(config.secure_cookies)
            .http_only(true)
            .max_age(cookie::time::Duration::seconds(0))
            .build()
    }
}
