pub mod config;
pub mod cookies;
pub mod error;
pub mod jwt;
pub mod middleware;
pub mod session;
pub mod tests;
pub mod user;
pub mod validator;

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::time::Duration;

//     // Helper function to create a mock request with cookies
//     // fn create_request_with_cookie_header(path: &str, cookie_header: &str) -> Request {
//     //     let mut req = HyperRequest::builder()
//     //         .method(Method::GET)
//     //         .uri(Uri::from_str(path).unwrap())
//     //         .body(Body::empty())
//     //         .unwrap();

//     //     if !cookie_header.is_empty() {
//     //         req.headers_mut().insert(COOKIE, HeaderValue::from_str(cookie_header).unwrap());
//     //     }

//     //     req
//     // }

//     fn create_request_with_cookies(path: &str, cookies: &[(&str, &str)]) -> Request {
//         let cookie_header = cookies
//             .iter()
//             .map(|(name, value)| format!("{}={}", name, value))
//             .collect::<Vec<_>>()
//             .join("; ");

//         create_request_with_cookie_header(path, &cookie_header)
//     }

//     #[test]
//     fn test_cookie_handler_single_cookie() {
//         let req = create_request_with_cookies("/test", &[("session_token", "abc123")]);
//         let result = CookieHandler::get_cookie_value(&req, "session_token").unwrap();
//         assert_eq!(result, Some("abc123".to_string()));
//     }

//     #[test]
//     fn test_cookie_handler_multiple_cookies() {
//         let req = create_request_with_cookies(
//             "/test",
//             &[
//                 ("session_token", "abc123"),
//                 ("user_pref", "dark_mode"),
//                 ("lang", "en"),
//             ],
//         );

//         assert_eq!(
//             CookieHandler::get_cookie_value(&req, "session_token").unwrap(),
//             Some("abc123".to_string())
//         );
//         assert_eq!(
//             CookieHandler::get_cookie_value(&req, "user_pref").unwrap(),
//             Some("dark_mode".to_string())
//         );
//         assert_eq!(
//             CookieHandler::get_cookie_value(&req, "lang").unwrap(),
//             Some("en".to_string())
//         );
//     }

//     #[test]
//     fn test_cookie_handler_missing_cookie() {
//         let req = create_request_with_cookies("/test", &[("other_cookie", "value")]);
//         let result = CookieHandler::get_cookie_value(&req, "session_token").unwrap();
//         assert_eq!(result, None);
//     }

//     #[test]
//     fn test_cookie_handler_no_cookies() {
//         let req = create_request_with_cookies("/test", &[]);
//         let result = CookieHandler::get_cookie_value(&req, "session_token").unwrap();
//         assert_eq!(result, None);
//     }

//     #[test]
//     fn test_cookie_handler_complex_cookies() {
//         // Test cookies with attributes (the cookie crate should handle these)
//         let req = create_request_with_cookie_header(
//             "/test",
//             "session=abc123; Path=/; HttpOnly; Secure; SameSite=Strict",
//         );
//         let result = CookieHandler::get_cookie_value(&req, "session").unwrap();
//         assert_eq!(result, Some("abc123".to_string()));
//     }

//     #[test]
//     fn test_cookie_handler_get_all_cookies() {
//         let req = create_request_with_cookies(
//             "/test",
//             &[
//                 ("session_token", "abc123"),
//                 ("user_pref", "dark_mode"),
//                 ("lang", "en"),
//             ],
//         );

//         let jar = CookieHandler::get_all_cookies(&req).unwrap();
//         assert!(jar.get("session_token").is_some());
//         assert!(jar.get("user_pref").is_some());
//         assert!(jar.get("lang").is_some());
//         assert_eq!(jar.get("session_token").unwrap().value(), "abc123");
//     }

//     #[test]
//     fn test_cookie_creation() {
//         let config = CookieAuthConfig::default();
//         let cookie = CookieHandler::create_session_cookie(
//             "session",
//             "token123",
//             &config,
//             Some(Duration::from_secs(3600)),
//         );

//         assert_eq!(cookie.name(), "session");
//         assert_eq!(cookie.value(), "token123");
//         assert_eq!(cookie.path(), Some("/"));
//         assert_eq!(cookie.secure(), Some(true));
//         assert_eq!(cookie.http_only(), Some(true));
//         assert!(cookie.max_age().is_some());
//     }

//     #[test]
//     fn test_logout_cookie_creation() {
//         let config = CookieAuthConfig::default();
//         let cookie = CookieHandler::create_logout_cookie("session", &config);

//         assert_eq!(cookie.name(), "session");
//         assert_eq!(cookie.value(), "");
//         assert_eq!(cookie.max_age(), Some(cookie::time::Duration::seconds(0)));
//     }

//     #[tokio::test]
//     async fn test_session_token_validator() {
//         let validator = SessionTokenValidator::new();
//         let user = AuthenticatedUser {
//             token: "valid_token_12345".to_string(),
//             level: AuthLevel::User,
//         };

//         validator
//             .add_session(
//                 "valid_token_12345".to_string(),
//                 user.clone(),
//                 Duration::from_secs(3600),
//             )
//             .await;

//         let result = validator.validate_token("valid_token_12345").await.unwrap();
//         assert_eq!(result.level, AuthLevel::User);
//         assert_eq!(result.token, "valid_token_12345");

//         let invalid_result = validator.validate_token("invalid_token").await;
//         assert!(matches!(invalid_result, Err(AuthError::UserNotFound)));
//     }

//     #[tokio::test]
//     async fn test_session_expiration() {
//         let validator = SessionTokenValidator::new();
//         let user = AuthenticatedUser {
//             token: "short_token".to_string(),
//             level: AuthLevel::User,
//         };

//         // Add session with very short TTL
//         validator
//             .add_session("short_token".to_string(), user, Duration::from_millis(1))
//             .await;

//         // Wait for expiration
//         tokio::time::sleep(Duration::from_millis(10)).await;

//         let result = validator.validate_token("short_token").await;
//         assert!(matches!(result, Err(AuthError::TokenExpired)));
//     }

//     #[tokio::test]
//     async fn test_session_cleanup() {
//         let validator = SessionTokenValidator::new();
//         let user = AuthenticatedUser {
//             token: "cleanup_token".to_string(),
//             level: AuthLevel::User,
//         };

//         validator
//             .add_session("cleanup_token".to_string(), user, Duration::from_millis(1))
//             .await;
//         tokio::time::sleep(Duration::from_millis(10)).await;

//         validator.cleanup_expired_sessions().await;

//         let result = validator.validate_token("cleanup_token").await;
//         assert!(matches!(result, Err(AuthError::UserNotFound)));
//     }

//     #[tokio::test]
//     async fn test_jwt_validator_format() {
//         let validator = JwtTokenValidator::new("secret");

//         // Valid JWT format
//         assert!(validator.is_valid_format("header.payload.signature"));

//         // Invalid formats
//         assert!(!validator.is_valid_format("only.two"));
//         assert!(!validator.is_valid_format("too.many.parts.here"));
//         assert!(!validator.is_valid_format(""));
//         assert!(!validator.is_valid_format("no_dots"));
//     }

//     #[tokio::test]
//     async fn test_jwt_validator_token_validation() {
//         let validator = JwtTokenValidator::new("secret");

//         // Test expired token
//         let result = validator.validate_token("expired.token.signature").await;
//         assert!(matches!(result, Err(AuthError::TokenExpired)));

//         // Test admin token
//         let result = validator
//             .validate_token("admin.payload.signature")
//             .await
//             .unwrap();
//         assert_eq!(result.level, AuthLevel::Admin);

//         // Test user token
//         let result = validator
//             .validate_token("user.payload.signature")
//             .await
//             .unwrap();
//         assert_eq!(result.level, AuthLevel::User);

//         // Test invalid format
//         let result = validator.validate_token("invalid_format").await;
//         assert!(matches!(result, Err(AuthError::InvalidToken)));
//     }

//     #[test]
//     fn test_auth_level_ordering() {
//         assert!(AuthLevel::Guest < AuthLevel::User);
//         assert!(AuthLevel::User < AuthLevel::Admin);
//         assert!(AuthLevel::Admin >= AuthLevel::User);
//         assert!(AuthLevel::User >= AuthLevel::Guest);
//     }
// }
