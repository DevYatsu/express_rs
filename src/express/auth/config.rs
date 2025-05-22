/// Configuration for cookie authentication
#[derive(Debug, Clone)]
pub struct CookieAuthConfig {
    /// Name of the session cookie expected in the request
    pub cookie_name: String,
    /// The path to redirect unauthenticated users to
    pub login_redirect: String,
    /// Whether to use secure cookies (HTTPS only)
    pub secure_cookies: bool,
    /// Maximum token length for security
    pub max_token_length: usize,
    /// Minimum token length for security
    pub min_token_length: usize,
    /// Whether to log authentication attempts
    pub enable_logging: bool,
    /// Cookie domain restriction
    pub cookie_domain: Option<String>,
    /// Cookie path restriction
    pub cookie_path: String,
    /// Cookie SameSite policy
    pub same_site: Option<cookie::SameSite>,
}

impl Default for CookieAuthConfig {
    fn default() -> Self {
        Self {
            cookie_name: "session_token".to_string(),
            login_redirect: "/login".to_string(),
            secure_cookies: true,
            max_token_length: 1024,
            min_token_length: 16,
            enable_logging: false,
            cookie_domain: None,
            cookie_path: "/".to_string(),
            same_site: Some(cookie::SameSite::Strict),
        }
    }
}
