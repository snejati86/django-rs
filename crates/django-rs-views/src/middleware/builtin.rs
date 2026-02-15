//! Built-in middleware components for django-rs.
//!
//! This module provides standard middleware that mirrors Django's built-in middleware:
//!
//! - [`SecurityMiddleware`] - Sets security-related HTTP headers
//! - [`CommonMiddleware`] - Handles trailing slashes and disallowed user agents
//! - [`GZipMiddleware`] - Compresses response bodies using gzip
//! - [`ConditionalGetMiddleware`] - Handles ETag and Last-Modified conditional requests
//! - [`CorsMiddleware`] - Adds CORS headers for cross-origin requests

use async_trait::async_trait;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;

use django_rs_core::DjangoError;
use django_rs_http::{HttpRequest, HttpResponse};

use super::Middleware;

// ── SecurityMiddleware ──────────────────────────────────────────────────

/// Middleware that sets security-related HTTP headers on every response.
///
/// Sets the following headers:
/// - `X-Content-Type-Options: nosniff`
/// - `X-Frame-Options: DENY`
/// - `X-XSS-Protection: 1; mode=block`
/// - `Strict-Transport-Security` (if `hsts_seconds > 0`)
///
/// This mirrors Django's `SecurityMiddleware`.
#[derive(Debug, Clone)]
pub struct SecurityMiddleware {
    /// Number of seconds for the HSTS max-age directive. Set to 0 to disable.
    pub hsts_seconds: u64,
    /// Whether to include subdomains in the HSTS header.
    pub hsts_include_subdomains: bool,
    /// Whether to include the preload directive in the HSTS header.
    pub hsts_preload: bool,
    /// The value for the X-Frame-Options header.
    pub x_frame_options: String,
}

impl Default for SecurityMiddleware {
    fn default() -> Self {
        Self {
            hsts_seconds: 0,
            hsts_include_subdomains: false,
            hsts_preload: false,
            x_frame_options: "DENY".to_string(),
        }
    }
}

#[async_trait]
impl Middleware for SecurityMiddleware {
    async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
        None
    }

    async fn process_response(
        &self,
        _request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        let mut resp = response;

        // X-Content-Type-Options
        resp.headers_mut().insert(
            http::header::HeaderName::from_static("x-content-type-options"),
            http::header::HeaderValue::from_static("nosniff"),
        );

        // X-Frame-Options
        if let Ok(value) = http::header::HeaderValue::from_str(&self.x_frame_options) {
            resp.headers_mut().insert(
                http::header::HeaderName::from_static("x-frame-options"),
                value,
            );
        }

        // X-XSS-Protection
        resp.headers_mut().insert(
            http::header::HeaderName::from_static("x-xss-protection"),
            http::header::HeaderValue::from_static("1; mode=block"),
        );

        // Strict-Transport-Security
        if self.hsts_seconds > 0 {
            let mut hsts_value = format!("max-age={}", self.hsts_seconds);
            if self.hsts_include_subdomains {
                hsts_value.push_str("; includeSubDomains");
            }
            if self.hsts_preload {
                hsts_value.push_str("; preload");
            }
            if let Ok(value) = http::header::HeaderValue::from_str(&hsts_value) {
                resp.headers_mut().insert(
                    http::header::HeaderName::from_static("strict-transport-security"),
                    value,
                );
            }
        }

        resp
    }

    async fn process_exception(
        &self,
        _request: &HttpRequest,
        _error: &DjangoError,
    ) -> Option<HttpResponse> {
        None
    }
}

// ── CommonMiddleware ────────────────────────────────────────────────────

/// Middleware that handles common request/response processing.
///
/// - Redirects to add/remove trailing slashes based on `append_slash`
/// - Blocks requests from disallowed user agents
///
/// This mirrors Django's `CommonMiddleware`.
#[derive(Debug, Clone)]
pub struct CommonMiddleware {
    /// Whether to redirect URLs without a trailing slash to the version with one.
    pub append_slash: bool,
    /// User agent substrings that should be blocked.
    pub disallowed_user_agents: Vec<String>,
}

impl Default for CommonMiddleware {
    fn default() -> Self {
        Self {
            append_slash: true,
            disallowed_user_agents: Vec::new(),
        }
    }
}

#[async_trait]
impl Middleware for CommonMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        // Check disallowed user agents
        if !self.disallowed_user_agents.is_empty() {
            if let Some(user_agent) = request
                .headers()
                .get(http::header::USER_AGENT)
                .and_then(|v| v.to_str().ok())
            {
                let ua_lower = user_agent.to_lowercase();
                for disallowed in &self.disallowed_user_agents {
                    if ua_lower.contains(&disallowed.to_lowercase()) {
                        return Some(HttpResponse::forbidden("Forbidden"));
                    }
                }
            }
        }

        // APPEND_SLASH: redirect if path doesn't end with /
        if self.append_slash && !request.path().ends_with('/') && !request.path().contains('.') {
            let new_path = format!("{}/", request.path());
            let redirect_url = if request.query_string().is_empty() {
                new_path
            } else {
                format!("{new_path}?{}", request.query_string())
            };
            return Some(
                django_rs_http::HttpResponsePermanentRedirect::new(&redirect_url),
            );
        }

        None
    }

    async fn process_response(
        &self,
        _request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
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

// ── GZipMiddleware ──────────────────────────────────────────────────────

/// Middleware that compresses response bodies larger than a threshold using gzip.
///
/// Only compresses responses when the client sends an `Accept-Encoding: gzip` header
/// and the response body exceeds `min_length` bytes (default 200).
///
/// This mirrors Django's `GZipMiddleware`.
#[derive(Debug, Clone)]
pub struct GZipMiddleware {
    /// Minimum response body size (in bytes) to trigger compression.
    pub min_length: usize,
}

impl Default for GZipMiddleware {
    fn default() -> Self {
        Self { min_length: 200 }
    }
}

#[async_trait]
impl Middleware for GZipMiddleware {
    async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
        None
    }

    async fn process_response(
        &self,
        request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        // Check if client accepts gzip
        let accepts_gzip = request
            .headers()
            .get(http::header::ACCEPT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("gzip"));

        if !accepts_gzip {
            return response;
        }

        // Check if response body is large enough
        let body_bytes = match response.content_bytes() {
            Some(bytes) if bytes.len() >= self.min_length => bytes,
            _ => return response,
        };

        // Compress the body
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        if encoder.write_all(&body_bytes).is_err() {
            return response;
        }
        let Ok(compressed) = encoder.finish() else {
            return response;
        };

        // Build compressed response
        let mut resp = HttpResponse::with_bytes(response.status(), compressed);
        resp.set_content_type(response.content_type());

        // Copy original headers
        for (name, value) in response.headers() {
            resp.headers_mut().insert(name.clone(), value.clone());
        }

        // Set encoding header
        resp.headers_mut().insert(
            http::header::CONTENT_ENCODING,
            http::header::HeaderValue::from_static("gzip"),
        );

        resp
    }

    async fn process_exception(
        &self,
        _request: &HttpRequest,
        _error: &DjangoError,
    ) -> Option<HttpResponse> {
        None
    }
}

// ── ConditionalGetMiddleware ────────────────────────────────────────────

/// Middleware that handles conditional GET requests using ETag and Last-Modified headers.
///
/// - If the response has an `ETag` header and the request has a matching
///   `If-None-Match`, returns 304 Not Modified.
/// - If the response has a `Last-Modified` header and the request has a
///   `If-Modified-Since` that is not before the last modification, returns 304.
///
/// This mirrors Django's `ConditionalGetMiddleware`.
#[derive(Debug, Clone, Default)]
pub struct ConditionalGetMiddleware;

#[async_trait]
impl Middleware for ConditionalGetMiddleware {
    async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
        None
    }

    async fn process_response(
        &self,
        request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        // Only handle GET and HEAD requests
        if request.method() != http::Method::GET && request.method() != http::Method::HEAD {
            return response;
        }

        // Only handle 200 OK responses
        if response.status() != http::StatusCode::OK {
            return response;
        }

        // Check ETag / If-None-Match
        if let (Some(etag), Some(if_none_match)) = (
            response
                .headers()
                .get(http::header::ETAG)
                .and_then(|v| v.to_str().ok()),
            request
                .headers()
                .get(http::header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok()),
        ) {
            if etag == if_none_match {
                return HttpResponse::new(http::StatusCode::NOT_MODIFIED, "");
            }
        }

        // Check Last-Modified / If-Modified-Since
        if let (Some(last_modified), Some(if_modified_since)) = (
            response
                .headers()
                .get(http::header::LAST_MODIFIED)
                .and_then(|v| v.to_str().ok()),
            request
                .headers()
                .get(http::header::IF_MODIFIED_SINCE)
                .and_then(|v| v.to_str().ok()),
        ) {
            // Simple string comparison - both should be in HTTP date format
            if last_modified == if_modified_since {
                return HttpResponse::new(http::StatusCode::NOT_MODIFIED, "");
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

// ── CorsMiddleware ──────────────────────────────────────────────────────

/// Middleware that adds CORS (Cross-Origin Resource Sharing) headers to responses.
///
/// Handles preflight OPTIONS requests and adds appropriate CORS headers based
/// on the configured allowed origins, methods, and headers.
///
/// This provides functionality similar to Django's `django-cors-headers` package.
#[derive(Debug, Clone)]
pub struct CorsMiddleware {
    /// Allowed origins. Use `["*"]` to allow all origins.
    pub allowed_origins: Vec<String>,
    /// Allowed HTTP methods for CORS requests.
    pub allowed_methods: Vec<String>,
    /// Allowed request headers.
    pub allowed_headers: Vec<String>,
    /// Whether to allow credentials (cookies, authorization headers).
    pub allow_credentials: bool,
    /// Max age for preflight cache (in seconds).
    pub max_age: u64,
}

impl Default for CorsMiddleware {
    fn default() -> Self {
        Self {
            allowed_origins: Vec::new(),
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "PATCH".to_string(),
                "DELETE".to_string(),
                "OPTIONS".to_string(),
            ],
            allowed_headers: vec![
                "Content-Type".to_string(),
                "Authorization".to_string(),
                "X-Requested-With".to_string(),
            ],
            allow_credentials: false,
            max_age: 86400,
        }
    }
}

impl CorsMiddleware {
    fn is_origin_allowed(&self, origin: &str) -> bool {
        if self.allowed_origins.iter().any(|o| o == "*") {
            return true;
        }
        self.allowed_origins.iter().any(|o| o == origin)
    }

    fn add_cors_headers(&self, response: &mut HttpResponse, origin: &str) {
        if let Ok(value) = http::header::HeaderValue::from_str(origin) {
            response.headers_mut().insert(
                http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
                value,
            );
        }

        if self.allow_credentials {
            response.headers_mut().insert(
                http::header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
                http::header::HeaderValue::from_static("true"),
            );
        }
    }
}

#[async_trait]
impl Middleware for CorsMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        // Handle preflight OPTIONS requests
        if request.method() == http::Method::OPTIONS {
            let origin = request
                .headers()
                .get(http::header::ORIGIN)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            if !origin.is_empty() && self.is_origin_allowed(&origin) {
                let mut response = HttpResponse::new(http::StatusCode::NO_CONTENT, "");
                self.add_cors_headers(&mut response, &origin);

                let methods = self.allowed_methods.join(", ");
                if let Ok(value) = http::header::HeaderValue::from_str(&methods) {
                    response.headers_mut().insert(
                        http::header::ACCESS_CONTROL_ALLOW_METHODS,
                        value,
                    );
                }

                let headers_str = self.allowed_headers.join(", ");
                if let Ok(value) = http::header::HeaderValue::from_str(&headers_str) {
                    response.headers_mut().insert(
                        http::header::ACCESS_CONTROL_ALLOW_HEADERS,
                        value,
                    );
                }

                let max_age = self.max_age.to_string();
                if let Ok(value) = http::header::HeaderValue::from_str(&max_age) {
                    response.headers_mut().insert(
                        http::header::ACCESS_CONTROL_MAX_AGE,
                        value,
                    );
                }

                return Some(response);
            }
        }

        None
    }

    async fn process_response(
        &self,
        request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        let origin = request
            .headers()
            .get(http::header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if origin.is_empty() || !self.is_origin_allowed(&origin) {
            return response;
        }

        let mut resp = response;
        self.add_cors_headers(&mut resp, &origin);
        resp
    }

    async fn process_exception(
        &self,
        _request: &HttpRequest,
        _error: &DjangoError,
    ) -> Option<HttpResponse> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SecurityMiddleware tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_security_middleware_default_headers() {
        let mw = SecurityMiddleware::default();
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;

        assert_eq!(
            result
                .headers()
                .get("x-content-type-options")
                .unwrap()
                .to_str()
                .unwrap(),
            "nosniff"
        );
        assert_eq!(
            result
                .headers()
                .get("x-frame-options")
                .unwrap()
                .to_str()
                .unwrap(),
            "DENY"
        );
        assert_eq!(
            result
                .headers()
                .get("x-xss-protection")
                .unwrap()
                .to_str()
                .unwrap(),
            "1; mode=block"
        );
    }

    #[tokio::test]
    async fn test_security_middleware_no_hsts_by_default() {
        let mw = SecurityMiddleware::default();
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;
        assert!(result.headers().get("strict-transport-security").is_none());
    }

    #[tokio::test]
    async fn test_security_middleware_hsts_enabled() {
        let mw = SecurityMiddleware {
            hsts_seconds: 31_536_000,
            ..Default::default()
        };
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;

        let hsts = result
            .headers()
            .get("strict-transport-security")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(hsts.contains("max-age=31536000"));
    }

    #[tokio::test]
    async fn test_security_middleware_hsts_with_subdomains() {
        let mw = SecurityMiddleware {
            hsts_seconds: 31_536_000,
            hsts_include_subdomains: true,
            ..Default::default()
        };
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;

        let hsts = result
            .headers()
            .get("strict-transport-security")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(hsts.contains("includeSubDomains"));
    }

    #[tokio::test]
    async fn test_security_middleware_hsts_with_preload() {
        let mw = SecurityMiddleware {
            hsts_seconds: 31_536_000,
            hsts_preload: true,
            ..Default::default()
        };
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;

        let hsts = result
            .headers()
            .get("strict-transport-security")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(hsts.contains("preload"));
    }

    #[tokio::test]
    async fn test_security_middleware_custom_x_frame_options() {
        let mw = SecurityMiddleware {
            x_frame_options: "SAMEORIGIN".to_string(),
            ..Default::default()
        };
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;

        assert_eq!(
            result
                .headers()
                .get("x-frame-options")
                .unwrap()
                .to_str()
                .unwrap(),
            "SAMEORIGIN"
        );
    }

    #[tokio::test]
    async fn test_security_middleware_does_not_block_requests() {
        let mw = SecurityMiddleware::default();
        let mut request = HttpRequest::builder().build();
        assert!(mw.process_request(&mut request).await.is_none());
    }

    // ── CommonMiddleware tests ──────────────────────────────────────

    #[tokio::test]
    async fn test_common_middleware_append_slash_redirect() {
        let mw = CommonMiddleware::default();
        let mut request = HttpRequest::builder().path("/articles").build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
        let response = result.unwrap();
        assert_eq!(response.status(), http::StatusCode::MOVED_PERMANENTLY);
        assert_eq!(
            response
                .headers()
                .get(http::header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "/articles/"
        );
    }

    #[tokio::test]
    async fn test_common_middleware_no_redirect_with_slash() {
        let mw = CommonMiddleware::default();
        let mut request = HttpRequest::builder().path("/articles/").build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_common_middleware_no_redirect_for_files() {
        let mw = CommonMiddleware::default();
        let mut request = HttpRequest::builder().path("/static/style.css").build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_common_middleware_append_slash_disabled() {
        let mw = CommonMiddleware {
            append_slash: false,
            ..Default::default()
        };
        let mut request = HttpRequest::builder().path("/articles").build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_common_middleware_disallowed_user_agent() {
        let mw = CommonMiddleware {
            disallowed_user_agents: vec!["badbot".to_string()],
            ..Default::default()
        };
        let mut request = HttpRequest::builder()
            .path("/articles/")
            .header("user-agent", "BadBot/1.0")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().status(), http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_common_middleware_allowed_user_agent() {
        let mw = CommonMiddleware {
            disallowed_user_agents: vec!["badbot".to_string()],
            ..Default::default()
        };
        let mut request = HttpRequest::builder()
            .path("/articles/")
            .header("user-agent", "Mozilla/5.0")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_common_middleware_append_slash_with_query_string() {
        let mw = CommonMiddleware::default();
        let mut request = HttpRequest::builder()
            .path("/articles")
            .query_string("page=1")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
        let response = result.unwrap();
        assert_eq!(
            response
                .headers()
                .get(http::header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "/articles/?page=1"
        );
    }

    // ── GZipMiddleware tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_gzip_middleware_compresses_large_response() {
        let mw = GZipMiddleware::default();
        let request = HttpRequest::builder()
            .header("accept-encoding", "gzip, deflate")
            .build();
        let body = "x".repeat(500);
        let response = HttpResponse::ok(&body);
        let result = mw.process_response(&request, response).await;

        assert_eq!(
            result
                .headers()
                .get(http::header::CONTENT_ENCODING)
                .unwrap()
                .to_str()
                .unwrap(),
            "gzip"
        );
        // Compressed size should be smaller
        let compressed_bytes = result.content_bytes().unwrap();
        assert!(compressed_bytes.len() < 500);
    }

    #[tokio::test]
    async fn test_gzip_middleware_skips_small_response() {
        let mw = GZipMiddleware::default();
        let request = HttpRequest::builder()
            .header("accept-encoding", "gzip")
            .build();
        let response = HttpResponse::ok("small");
        let result = mw.process_response(&request, response).await;

        assert!(result.headers().get(http::header::CONTENT_ENCODING).is_none());
    }

    #[tokio::test]
    async fn test_gzip_middleware_skips_without_accept_encoding() {
        let mw = GZipMiddleware::default();
        let request = HttpRequest::builder().build();
        let body = "x".repeat(500);
        let response = HttpResponse::ok(&body);
        let result = mw.process_response(&request, response).await;

        assert!(result.headers().get(http::header::CONTENT_ENCODING).is_none());
    }

    #[tokio::test]
    async fn test_gzip_middleware_custom_min_length() {
        let mw = GZipMiddleware { min_length: 10 };
        let request = HttpRequest::builder()
            .header("accept-encoding", "gzip")
            .build();
        let response = HttpResponse::ok("hello world! test data");
        let result = mw.process_response(&request, response).await;

        assert!(result.headers().get(http::header::CONTENT_ENCODING).is_some());
    }

    // ── ConditionalGetMiddleware tests ──────────────────────────────

    #[tokio::test]
    async fn test_conditional_get_etag_match() {
        let mw = ConditionalGetMiddleware;
        let request = HttpRequest::builder()
            .header("if-none-match", "\"abc123\"")
            .build();
        let response = HttpResponse::ok("test").set_header(
            http::header::ETAG,
            http::header::HeaderValue::from_static("\"abc123\""),
        );
        let result = mw.process_response(&request, response).await;
        assert_eq!(result.status(), http::StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn test_conditional_get_etag_no_match() {
        let mw = ConditionalGetMiddleware;
        let request = HttpRequest::builder()
            .header("if-none-match", "\"different\"")
            .build();
        let response = HttpResponse::ok("test").set_header(
            http::header::ETAG,
            http::header::HeaderValue::from_static("\"abc123\""),
        );
        let result = mw.process_response(&request, response).await;
        assert_eq!(result.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_conditional_get_last_modified_match() {
        let mw = ConditionalGetMiddleware;
        let request = HttpRequest::builder()
            .header("if-modified-since", "Wed, 21 Oct 2015 07:28:00 GMT")
            .build();
        let response = HttpResponse::ok("test").set_header(
            http::header::LAST_MODIFIED,
            http::header::HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
        );
        let result = mw.process_response(&request, response).await;
        assert_eq!(result.status(), http::StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn test_conditional_get_only_for_get_requests() {
        let mw = ConditionalGetMiddleware;
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .header("if-none-match", "\"abc123\"")
            .build();
        let response = HttpResponse::ok("test").set_header(
            http::header::ETAG,
            http::header::HeaderValue::from_static("\"abc123\""),
        );
        let result = mw.process_response(&request, response).await;
        assert_eq!(result.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_conditional_get_only_for_200_responses() {
        let mw = ConditionalGetMiddleware;
        let request = HttpRequest::builder()
            .header("if-none-match", "\"abc123\"")
            .build();
        let mut response = HttpResponse::ok("test");
        response.set_status(http::StatusCode::CREATED);
        let response = response.set_header(
            http::header::ETAG,
            http::header::HeaderValue::from_static("\"abc123\""),
        );
        let result = mw.process_response(&request, response).await;
        assert_eq!(result.status(), http::StatusCode::CREATED);
    }

    // ── CorsMiddleware tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_cors_middleware_preflight() {
        let mw = CorsMiddleware {
            allowed_origins: vec!["http://example.com".to_string()],
            ..Default::default()
        };
        let mut request = HttpRequest::builder()
            .method(http::Method::OPTIONS)
            .header("origin", "http://example.com")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
        let response = result.unwrap();
        assert_eq!(response.status(), http::StatusCode::NO_CONTENT);
        assert!(response
            .headers()
            .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_some());
        assert!(response
            .headers()
            .get(http::header::ACCESS_CONTROL_ALLOW_METHODS)
            .is_some());
    }

    #[tokio::test]
    async fn test_cors_middleware_preflight_disallowed_origin() {
        let mw = CorsMiddleware {
            allowed_origins: vec!["http://example.com".to_string()],
            ..Default::default()
        };
        let mut request = HttpRequest::builder()
            .method(http::Method::OPTIONS)
            .header("origin", "http://evil.com")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cors_middleware_adds_headers_to_response() {
        let mw = CorsMiddleware {
            allowed_origins: vec!["http://example.com".to_string()],
            ..Default::default()
        };
        let request = HttpRequest::builder()
            .header("origin", "http://example.com")
            .build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;

        assert_eq!(
            result
                .headers()
                .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap()
                .to_str()
                .unwrap(),
            "http://example.com"
        );
    }

    #[tokio::test]
    async fn test_cors_middleware_wildcard_origin() {
        let mw = CorsMiddleware {
            allowed_origins: vec!["*".to_string()],
            ..Default::default()
        };
        let request = HttpRequest::builder()
            .header("origin", "http://any-site.com")
            .build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;

        assert!(result
            .headers()
            .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_some());
    }

    #[tokio::test]
    async fn test_cors_middleware_no_origin_header() {
        let mw = CorsMiddleware {
            allowed_origins: vec!["*".to_string()],
            ..Default::default()
        };
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;

        assert!(result
            .headers()
            .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());
    }

    #[tokio::test]
    async fn test_cors_middleware_credentials() {
        let mw = CorsMiddleware {
            allowed_origins: vec!["http://example.com".to_string()],
            allow_credentials: true,
            ..Default::default()
        };
        let request = HttpRequest::builder()
            .header("origin", "http://example.com")
            .build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;

        assert_eq!(
            result
                .headers()
                .get(http::header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .unwrap()
                .to_str()
                .unwrap(),
            "true"
        );
    }
}
