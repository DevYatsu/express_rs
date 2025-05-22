use super::{
    config::CookieAuthConfig,
    error::{AuthError, AuthResult},
};
use crate::handler::Request;
use cookie::{Cookie, CookieJar};
use hyper::header::COOKIE;

/// Cookie handling utility using the cookie crate
#[derive(Debug, Clone)]
pub struct CookieHandler;

impl CookieHandler {
    /// Extracts a specific cookie value from the request using cookie crate
    pub fn get_cookie_value(req: &Request, cookie_name: &str) -> AuthResult<Option<String>> {
        let mut jar = CookieJar::new();
        let headers = req.headers();

        // Parse all cookie headers
        for cookie_header in headers.get_all(COOKIE) {
            let cookie_str = cookie_header
                .to_str()
                .map_err(|_| AuthError::MalformedCookie)?;

            // Parse each cookie in the header
            for cookie_str in cookie_str.split(';') {
                let cookie_str = cookie_str.trim();
                if !cookie_str.is_empty() {
                    match Cookie::parse(cookie_str) {
                        Ok(cookie) => jar.add_original(cookie),
                        Err(_) => continue, // Skip malformed individual cookies
                    }
                }
            }
        }

        Ok(jar
            .get(cookie_name)
            .map(|cookie| cookie.value().to_string()))
    }

    /// Gets all cookies from the request
    pub fn get_all_cookies(req: &Request) -> AuthResult<CookieJar> {
        let mut jar = CookieJar::new();

        for cookie_header in req.headers().get_all(COOKIE) {
            let cookie_str = cookie_header
                .to_str()
                .map_err(|_| AuthError::MalformedCookie)?;

            for cookie_str in cookie_str.split(';') {
                let cookie_str = cookie_str.trim();
                if !cookie_str.is_empty() {
                    match Cookie::parse(cookie_str) {
                        Ok(cookie) => jar.add_original(cookie),
                        Err(_) => continue,
                    }
                }
            }
        }

        Ok(jar)
    }

    /// Creates a new session cookie
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

    /// Creates a cookie to clear/logout
    pub fn create_logout_cookie(name: &str, config: &CookieAuthConfig) -> Cookie<'static> {
        Cookie::build((name.to_owned(), ""))
            .path(config.cookie_path.clone())
            .secure(config.secure_cookies)
            .http_only(true)
            .max_age(cookie::time::Duration::seconds(0))
            .build()
    }
}
