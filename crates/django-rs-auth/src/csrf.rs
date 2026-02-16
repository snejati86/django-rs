//! CSRF (Cross-Site Request Forgery) protection middleware.
//!
//! This module provides CSRF token generation, masking, validation, and a middleware
//! component that enforces CSRF protection on state-changing HTTP methods.
//!
//! ## How it works
//!
//! 1. On GET/HEAD/OPTIONS/TRACE requests, a CSRF cookie is set on the response.
//! 2. On POST/PUT/PATCH/DELETE requests, the middleware validates that the request
//!    includes a valid CSRF token (via header or form field) matching the cookie.
//! 3. Requests without a valid token receive a 403 Forbidden response.
//!
//! ## Token Masking
//!
//! Tokens are XOR-masked before being sent to the client to prevent BREACH attacks
//! on compressed HTTPS responses.

use async_trait::async_trait;
use django_rs_core::error::DjangoError;
use django_rs_http::{HttpRequest, HttpResponse};
use django_rs_views::middleware::Middleware;
use rand::RngCore;
use std::collections::HashSet;

/// The length of a CSRF token in bytes (produces 64-char hex string).
const CSRF_TOKEN_LENGTH: usize = 32;

/// The form field name used for CSRF tokens.
const CSRF_FORM_FIELD: &str = "csrfmiddlewaretoken";

/// CSRF protection middleware.
///
/// Sets a CSRF cookie on safe requests and validates the CSRF token
/// on state-changing requests. Views can be exempt from CSRF checking
/// by adding their paths to the exempt list.
#[derive(Debug, Clone)]
pub struct CsrfMiddleware {
    /// Name of the CSRF cookie.
    pub cookie_name: String,
    /// Name of the HTTP header containing the CSRF token.
    pub header_name: String,
    /// Whether the CSRF cookie should use the Secure flag.
    pub cookie_secure: bool,
    /// Whether the CSRF cookie should use the `HttpOnly` flag.
    pub cookie_httponly: bool,
    /// Origins that are trusted for CSRF validation.
    pub trusted_origins: Vec<String>,
    /// Paths that are exempt from CSRF validation.
    pub exempt_paths: HashSet<String>,
}

impl Default for CsrfMiddleware {
    fn default() -> Self {
        Self {
            cookie_name: "csrftoken".to_string(),
            header_name: "X-CSRFToken".to_string(),
            cookie_secure: false,
            cookie_httponly: false,
            trusted_origins: Vec::new(),
            exempt_paths: HashSet::new(),
        }
    }
}

impl CsrfMiddleware {
    /// Creates a new `CsrfMiddleware` with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a path to the CSRF exempt list.
    pub fn add_exempt_path(&mut self, path: &str) {
        self.exempt_paths.insert(path.to_string());
    }

    /// Returns `true` if the given HTTP method is "safe" (does not modify state).
    const fn is_safe_method(method: &http::Method) -> bool {
        matches!(
            *method,
            http::Method::GET | http::Method::HEAD | http::Method::OPTIONS | http::Method::TRACE
        )
    }

    /// Extracts the CSRF cookie value from the request.
    fn get_csrf_cookie(&self, request: &HttpRequest) -> Option<String> {
        let cookie_header = request
            .headers()
            .get(http::header::COOKIE)
            .and_then(|v| v.to_str().ok())?;

        for cookie in cookie_header.split(';') {
            let cookie = cookie.trim();
            if let Some(value) = cookie.strip_prefix(&format!("{}=", self.cookie_name)) {
                return Some(value.to_string());
            }
        }
        None
    }

    /// Extracts the CSRF token from the request (header or form field).
    fn get_request_token(&self, request: &HttpRequest) -> Option<String> {
        // Check header first
        if let Some(token) = request
            .headers()
            .get(&self.header_name)
            .and_then(|v| v.to_str().ok())
        {
            return Some(token.to_string());
        }

        // Check form field
        request.post().get(CSRF_FORM_FIELD).map(String::from)
    }

    /// Checks if the request origin is trusted.
    fn is_origin_trusted(&self, request: &HttpRequest) -> bool {
        if self.trusted_origins.is_empty() {
            return false;
        }

        let origin = request
            .headers()
            .get(http::header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .or_else(|| {
                request
                    .headers()
                    .get(http::header::REFERER)
                    .and_then(|v| v.to_str().ok())
            });

        origin.is_some_and(|origin| {
            self.trusted_origins
                .iter()
                .any(|trusted| origin.starts_with(trusted))
        })
    }

    /// Builds the Set-Cookie header value for the CSRF cookie.
    fn build_cookie(&self, token: &str) -> String {
        let mut cookie = format!("{}={}; Path=/; SameSite=Lax", self.cookie_name, token);
        if self.cookie_secure {
            cookie.push_str("; Secure");
        }
        if self.cookie_httponly {
            cookie.push_str("; HttpOnly");
        }
        cookie
    }
}

#[async_trait]
impl Middleware for CsrfMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        // Safe methods don't need CSRF validation
        if Self::is_safe_method(request.method()) {
            return None;
        }

        // Check exempt paths
        if self.exempt_paths.contains(request.path()) {
            return None;
        }

        // Check trusted origins
        if self.is_origin_trusted(request) {
            return None;
        }

        // Get the CSRF cookie
        let Some(cookie_token) = self.get_csrf_cookie(request) else {
            return Some(HttpResponse::forbidden("CSRF cookie not set."));
        };

        // Get the request token (from header or form)
        let Some(request_token) = self.get_request_token(request) else {
            return Some(HttpResponse::forbidden("CSRF token missing."));
        };

        // Validate the token
        if !validate_csrf_token(&request_token, &cookie_token) {
            return Some(HttpResponse::forbidden("CSRF token invalid."));
        }

        None
    }

    async fn process_response(
        &self,
        request: &HttpRequest,
        mut response: HttpResponse,
    ) -> HttpResponse {
        // Set CSRF cookie on safe requests if not already set
        if Self::is_safe_method(request.method()) && self.get_csrf_cookie(request).is_none() {
            let token = generate_csrf_token();
            if let Ok(value) = http::HeaderValue::from_str(&self.build_cookie(&token)) {
                response
                    .headers_mut()
                    .insert(http::header::SET_COOKIE, value);
            }
        }
        response
    }

    async fn process_exception(
        &self,
        _request: &HttpRequest,
        _error: &DjangoError,
    ) -> Option<HttpResponse> {
        None
    }
}

/// Generates a cryptographically random CSRF token as a 64-character hex string.
///
/// Uses the OS random number generator for secure randomness.
pub fn generate_csrf_token() -> String {
    let mut bytes = [0u8; CSRF_TOKEN_LENGTH];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

/// Masks a CSRF token using XOR masking to prevent BREACH attacks.
///
/// Generates a random mask, XORs it with the token bytes, and returns
/// the concatenation of mask and masked token as a hex string.
pub fn mask_csrf_token(token: &str) -> String {
    let Some(token_bytes) = hex_decode(token) else {
        return String::new();
    };

    let mut mask = vec![0u8; token_bytes.len()];
    rand::thread_rng().fill_bytes(&mut mask);

    let masked: Vec<u8> = token_bytes
        .iter()
        .zip(mask.iter())
        .map(|(t, m)| t ^ m)
        .collect();

    // Concatenate mask + masked_token
    let mut result = mask;
    result.extend_from_slice(&masked);
    hex_encode(&result)
}

/// Unmasks a previously masked CSRF token.
///
/// Splits the input into mask and masked token, then XORs them to recover
/// the original token.
pub fn unmask_csrf_token(masked: &str) -> String {
    let Some(bytes) = hex_decode(masked) else {
        return String::new();
    };

    if bytes.len() % 2 != 0 {
        return String::new();
    }

    let half = bytes.len() / 2;
    let mask = &bytes[..half];
    let masked_token = &bytes[half..];

    let unmasked: Vec<u8> = masked_token
        .iter()
        .zip(mask.iter())
        .map(|(m, k)| m ^ k)
        .collect();

    hex_encode(&unmasked)
}

/// Validates that a request CSRF token matches the cookie CSRF token.
///
/// Supports both masked and unmasked tokens. Uses constant-time comparison
/// to prevent timing attacks.
pub fn validate_csrf_token(request_token: &str, cookie_token: &str) -> bool {
    if request_token.is_empty() || cookie_token.is_empty() {
        return false;
    }

    // If the request token is masked (double length), unmask it first
    let effective_request = if request_token.len() == cookie_token.len() * 2 {
        unmask_csrf_token(request_token)
    } else {
        request_token.to_string()
    };

    constant_time_eq(effective_request.as_bytes(), cookie_token.as_bytes())
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Encodes bytes as a hex string.
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Decodes a hex string into bytes.
fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16).ok()?;
        bytes.push(byte);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Token generation tests ──────────────────────────────────────

    #[test]
    fn test_generate_csrf_token_length() {
        let token = generate_csrf_token();
        assert_eq!(token.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_generate_csrf_token_uniqueness() {
        let t1 = generate_csrf_token();
        let t2 = generate_csrf_token();
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_generate_csrf_token_hex_only() {
        let token = generate_csrf_token();
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ── Masking/unmasking tests ─────────────────────────────────────

    #[test]
    fn test_mask_unmask_roundtrip() {
        let token = generate_csrf_token();
        let masked = mask_csrf_token(&token);
        let unmasked = unmask_csrf_token(&masked);
        assert_eq!(unmasked, token);
    }

    #[test]
    fn test_masked_token_length() {
        let token = generate_csrf_token();
        let masked = mask_csrf_token(&token);
        assert_eq!(masked.len(), token.len() * 2); // Mask + masked_token
    }

    #[test]
    fn test_mask_different_each_time() {
        let token = generate_csrf_token();
        let m1 = mask_csrf_token(&token);
        let m2 = mask_csrf_token(&token);
        assert_ne!(m1, m2); // Different random masks
                            // But both unmask to the same token
        assert_eq!(unmask_csrf_token(&m1), token);
        assert_eq!(unmask_csrf_token(&m2), token);
    }

    #[test]
    fn test_mask_invalid_token() {
        assert_eq!(mask_csrf_token("not_hex"), "");
        assert_eq!(mask_csrf_token(""), "");
    }

    #[test]
    fn test_unmask_invalid_input() {
        assert_eq!(unmask_csrf_token("not_hex"), "");
        assert_eq!(unmask_csrf_token(""), "");
        assert_eq!(unmask_csrf_token("abc"), ""); // Odd length
    }

    // ── Validation tests ────────────────────────────────────────────

    #[test]
    fn test_validate_same_token() {
        let token = generate_csrf_token();
        assert!(validate_csrf_token(&token, &token));
    }

    #[test]
    fn test_validate_masked_token() {
        let token = generate_csrf_token();
        let masked = mask_csrf_token(&token);
        assert!(validate_csrf_token(&masked, &token));
    }

    #[test]
    fn test_validate_wrong_token() {
        let t1 = generate_csrf_token();
        let t2 = generate_csrf_token();
        assert!(!validate_csrf_token(&t1, &t2));
    }

    #[test]
    fn test_validate_empty_tokens() {
        assert!(!validate_csrf_token("", ""));
        assert!(!validate_csrf_token("token", ""));
        assert!(!validate_csrf_token("", "token"));
    }

    // ── hex_encode / hex_decode tests ───────────────────────────────

    #[test]
    fn test_hex_roundtrip() {
        let bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let hex = hex_encode(&bytes);
        assert_eq!(hex, "deadbeef");
        assert_eq!(hex_decode(&hex).unwrap(), bytes);
    }

    #[test]
    fn test_hex_decode_invalid() {
        assert!(hex_decode("xyz").is_none());
        assert!(hex_decode("a").is_none()); // Odd length
    }

    // ── CsrfMiddleware tests ────────────────────────────────────────

    #[test]
    fn test_csrf_middleware_default() {
        let mw = CsrfMiddleware::default();
        assert_eq!(mw.cookie_name, "csrftoken");
        assert_eq!(mw.header_name, "X-CSRFToken");
        assert!(!mw.cookie_secure);
        assert!(!mw.cookie_httponly);
    }

    #[test]
    fn test_is_safe_method() {
        assert!(CsrfMiddleware::is_safe_method(&http::Method::GET));
        assert!(CsrfMiddleware::is_safe_method(&http::Method::HEAD));
        assert!(CsrfMiddleware::is_safe_method(&http::Method::OPTIONS));
        assert!(CsrfMiddleware::is_safe_method(&http::Method::TRACE));
        assert!(!CsrfMiddleware::is_safe_method(&http::Method::POST));
        assert!(!CsrfMiddleware::is_safe_method(&http::Method::PUT));
        assert!(!CsrfMiddleware::is_safe_method(&http::Method::PATCH));
        assert!(!CsrfMiddleware::is_safe_method(&http::Method::DELETE));
    }

    #[test]
    fn test_csrf_add_exempt_path() {
        let mut mw = CsrfMiddleware::new();
        mw.add_exempt_path("/api/webhook/");
        assert!(mw.exempt_paths.contains("/api/webhook/"));
    }

    #[test]
    fn test_csrf_build_cookie() {
        let mw = CsrfMiddleware::default();
        let cookie = mw.build_cookie("test_token");
        assert!(cookie.contains("csrftoken=test_token"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(!cookie.contains("Secure"));
        assert!(!cookie.contains("HttpOnly"));
    }

    #[test]
    fn test_csrf_build_cookie_secure() {
        let mw = CsrfMiddleware {
            cookie_secure: true,
            cookie_httponly: true,
            ..CsrfMiddleware::default()
        };
        let cookie = mw.build_cookie("token");
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("HttpOnly"));
    }

    #[tokio::test]
    async fn test_csrf_middleware_allows_get() {
        let mw = CsrfMiddleware::new();
        let mut request = HttpRequest::builder().method(http::Method::GET).build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_csrf_middleware_blocks_post_without_cookie() {
        let mw = CsrfMiddleware::new();
        let mut request = HttpRequest::builder().method(http::Method::POST).build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().status(), http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_csrf_middleware_blocks_post_without_token() {
        let mw = CsrfMiddleware::new();
        let token = generate_csrf_token();
        let mut request = HttpRequest::builder()
            .method(http::Method::POST)
            .header("cookie", &format!("csrftoken={token}"))
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().status(), http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_csrf_middleware_allows_post_with_valid_token() {
        let mw = CsrfMiddleware::new();
        let token = generate_csrf_token();
        let mut request = HttpRequest::builder()
            .method(http::Method::POST)
            .header("cookie", &format!("csrftoken={token}"))
            .header("x-csrftoken", &token)
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_csrf_middleware_blocks_post_with_wrong_token() {
        let mw = CsrfMiddleware::new();
        let cookie_token = generate_csrf_token();
        let wrong_token = generate_csrf_token();
        let mut request = HttpRequest::builder()
            .method(http::Method::POST)
            .header("cookie", &format!("csrftoken={cookie_token}"))
            .header("x-csrftoken", &wrong_token)
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().status(), http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_csrf_middleware_exempt_path() {
        let mut mw = CsrfMiddleware::new();
        mw.add_exempt_path("/api/webhook/");
        let mut request = HttpRequest::builder()
            .method(http::Method::POST)
            .path("/api/webhook/")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_csrf_middleware_sets_cookie_on_get_response() {
        let mw = CsrfMiddleware::new();
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = HttpResponse::ok("test");
        let response = mw.process_response(&request, response).await;
        assert!(response.headers().get(http::header::SET_COOKIE).is_some());
    }

    #[tokio::test]
    async fn test_csrf_middleware_no_duplicate_cookie() {
        let mw = CsrfMiddleware::new();
        let token = generate_csrf_token();
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .header("cookie", &format!("csrftoken={token}"))
            .build();
        let response = HttpResponse::ok("test");
        let response = mw.process_response(&request, response).await;
        // Should NOT set cookie since one already exists
        assert!(response.headers().get(http::header::SET_COOKIE).is_none());
    }

    #[tokio::test]
    async fn test_csrf_middleware_allows_post_with_form_token() {
        let mw = CsrfMiddleware::new();
        let token = generate_csrf_token();
        let body = format!("csrfmiddlewaretoken={token}&data=test");
        let mut request = HttpRequest::builder()
            .method(http::Method::POST)
            .header("cookie", &format!("csrftoken={token}"))
            .content_type("application/x-www-form-urlencoded")
            .body(body.into_bytes())
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_csrf_middleware_trusted_origin() {
        let mw = CsrfMiddleware {
            trusted_origins: vec!["https://trusted.example.com".to_string()],
            ..CsrfMiddleware::default()
        };
        let mut request = HttpRequest::builder()
            .method(http::Method::POST)
            .header("origin", "https://trusted.example.com")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_csrf_middleware_blocks_put() {
        let mw = CsrfMiddleware::new();
        let mut request = HttpRequest::builder().method(http::Method::PUT).build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_csrf_middleware_blocks_delete() {
        let mw = CsrfMiddleware::new();
        let mut request = HttpRequest::builder().method(http::Method::DELETE).build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_csrf_middleware_allows_masked_token() {
        let mw = CsrfMiddleware::new();
        let token = generate_csrf_token();
        let masked = mask_csrf_token(&token);
        let mut request = HttpRequest::builder()
            .method(http::Method::POST)
            .header("cookie", &format!("csrftoken={token}"))
            .header("x-csrftoken", &masked)
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }
}
