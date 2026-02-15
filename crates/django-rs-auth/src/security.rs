//! Security middleware for django-rs.
//!
//! This module provides the [`SecurityMiddleware`] which validates the `Host` header
//! against allowed hosts, enforces SSL/TLS redirects, and sets security-related
//! HTTP response headers.
//!
//! ## Features
//!
//! - **Host header validation** against `ALLOWED_HOSTS`
//! - **HTTPS redirect** (configurable)
//! - **HSTS** (HTTP Strict Transport Security) headers
//! - **X-Content-Type-Options: nosniff**
//! - **Cross-Origin-Opener-Policy** headers
//! - **Referrer-Policy** headers

use async_trait::async_trait;
use django_rs_core::error::DjangoError;
use django_rs_http::{HttpRequest, HttpResponse, HttpResponseRedirect};
use django_rs_views::middleware::Middleware;

/// Security middleware that validates hosts and sets security headers.
///
/// Mirrors Django's `SecurityMiddleware`. Should typically be the first
/// middleware in the pipeline.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct SecurityMiddleware {
    /// Whether to redirect all HTTP requests to HTTPS.
    pub secure_ssl_redirect: bool,
    /// The number of seconds for the HSTS `max-age` directive. Zero disables HSTS.
    pub secure_hsts_seconds: u64,
    /// Whether to include subdomains in the HSTS directive.
    pub secure_hsts_include_subdomains: bool,
    /// Whether to include the `preload` directive in HSTS.
    pub secure_hsts_preload: bool,
    /// Whether to set `X-Content-Type-Options: nosniff`.
    pub secure_content_type_nosniff: bool,
    /// List of allowed hostnames. An empty list allows all hosts.
    pub allowed_hosts: Vec<String>,
    /// Value for the `Cross-Origin-Opener-Policy` header.
    pub secure_cross_origin_opener_policy: String,
    /// Value for the `Referrer-Policy` header.
    pub referrer_policy: String,
}

impl Default for SecurityMiddleware {
    fn default() -> Self {
        Self {
            secure_ssl_redirect: false,
            secure_hsts_seconds: 0,
            secure_hsts_include_subdomains: false,
            secure_hsts_preload: false,
            secure_content_type_nosniff: true,
            allowed_hosts: Vec::new(),
            secure_cross_origin_opener_policy: "same-origin".to_string(),
            referrer_policy: "same-origin".to_string(),
        }
    }
}

impl SecurityMiddleware {
    /// Creates a new `SecurityMiddleware` with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates that the request's `Host` header is in the allowed list.
    ///
    /// Returns `true` if the host is valid. An empty `allowed_hosts` list
    /// allows all hosts (useful in development).
    fn is_host_allowed(&self, request: &HttpRequest) -> bool {
        // Empty allowed_hosts means allow all (development mode)
        if self.allowed_hosts.is_empty() {
            return true;
        }

        let host = request.get_host();
        // Strip port number for matching
        let host_without_port = host.split(':').next().unwrap_or(host);

        self.allowed_hosts.iter().any(|allowed| {
            if allowed == "*" {
                return true;
            }
            // Wildcard subdomain: .example.com matches *.example.com and example.com
            allowed.strip_prefix('.').map_or_else(
                || host_without_port.eq_ignore_ascii_case(allowed),
                |suffix| host_without_port.ends_with(suffix) || host_without_port == suffix,
            )
        })
    }

    /// Builds the HSTS header value.
    fn hsts_header_value(&self) -> Option<String> {
        if self.secure_hsts_seconds == 0 {
            return None;
        }
        let mut value = format!("max-age={}", self.secure_hsts_seconds);
        if self.secure_hsts_include_subdomains {
            value.push_str("; includeSubDomains");
        }
        if self.secure_hsts_preload {
            value.push_str("; preload");
        }
        Some(value)
    }
}

#[async_trait]
impl Middleware for SecurityMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        // Validate Host header
        if !self.is_host_allowed(request) {
            return Some(HttpResponse::bad_request(format!(
                "Invalid HTTP_HOST header: '{}'. You may need to add '{}' to ALLOWED_HOSTS.",
                request.get_host(),
                request.get_host()
            )));
        }

        // SSL redirect
        if self.secure_ssl_redirect && !request.is_secure() {
            let host = request.get_host();
            let path = request.get_full_path();
            let redirect_url = format!("https://{host}{path}");
            return Some(HttpResponseRedirect::new(&redirect_url));
        }

        None
    }

    async fn process_response(
        &self,
        request: &HttpRequest,
        mut response: HttpResponse,
    ) -> HttpResponse {
        // HSTS header (only on HTTPS)
        if request.is_secure() {
            if let Some(hsts_value) = self.hsts_header_value() {
                if let Ok(value) = http::HeaderValue::from_str(&hsts_value) {
                    response.headers_mut().insert(
                        http::header::STRICT_TRANSPORT_SECURITY,
                        value,
                    );
                }
            }
        }

        // X-Content-Type-Options
        if self.secure_content_type_nosniff {
            response.headers_mut().insert(
                http::header::X_CONTENT_TYPE_OPTIONS,
                http::HeaderValue::from_static("nosniff"),
            );
        }

        // Cross-Origin-Opener-Policy
        if !self.secure_cross_origin_opener_policy.is_empty() {
            if let Ok(value) = http::HeaderValue::from_str(&self.secure_cross_origin_opener_policy)
            {
                response.headers_mut().insert(
                    http::header::HeaderName::from_static("cross-origin-opener-policy"),
                    value,
                );
            }
        }

        // Referrer-Policy
        if !self.referrer_policy.is_empty() {
            if let Ok(value) = http::HeaderValue::from_str(&self.referrer_policy) {
                response.headers_mut().insert(
                    http::header::REFERRER_POLICY,
                    value,
                );
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── SecurityMiddleware construction tests ────────────────────────

    #[test]
    fn test_security_middleware_default() {
        let mw = SecurityMiddleware::default();
        assert!(!mw.secure_ssl_redirect);
        assert_eq!(mw.secure_hsts_seconds, 0);
        assert!(!mw.secure_hsts_include_subdomains);
        assert!(!mw.secure_hsts_preload);
        assert!(mw.secure_content_type_nosniff);
        assert!(mw.allowed_hosts.is_empty());
        assert_eq!(mw.secure_cross_origin_opener_policy, "same-origin");
        assert_eq!(mw.referrer_policy, "same-origin");
    }

    // ── Host validation tests ───────────────────────────────────────

    #[test]
    fn test_host_allowed_empty_list() {
        let mw = SecurityMiddleware::default();
        let request = HttpRequest::builder()
            .meta("HTTP_HOST", "anything.example.com")
            .build();
        assert!(mw.is_host_allowed(&request));
    }

    #[test]
    fn test_host_allowed_exact_match() {
        let mw = SecurityMiddleware {
            allowed_hosts: vec!["example.com".to_string()],
            ..SecurityMiddleware::default()
        };
        let request = HttpRequest::builder()
            .meta("HTTP_HOST", "example.com")
            .build();
        assert!(mw.is_host_allowed(&request));
    }

    #[test]
    fn test_host_allowed_with_port() {
        let mw = SecurityMiddleware {
            allowed_hosts: vec!["example.com".to_string()],
            ..SecurityMiddleware::default()
        };
        let request = HttpRequest::builder()
            .meta("HTTP_HOST", "example.com:8080")
            .build();
        assert!(mw.is_host_allowed(&request));
    }

    #[test]
    fn test_host_not_allowed() {
        let mw = SecurityMiddleware {
            allowed_hosts: vec!["example.com".to_string()],
            ..SecurityMiddleware::default()
        };
        let request = HttpRequest::builder()
            .meta("HTTP_HOST", "evil.com")
            .build();
        assert!(!mw.is_host_allowed(&request));
    }

    #[test]
    fn test_host_wildcard() {
        let mw = SecurityMiddleware {
            allowed_hosts: vec!["*".to_string()],
            ..SecurityMiddleware::default()
        };
        let request = HttpRequest::builder()
            .meta("HTTP_HOST", "anything.example.com")
            .build();
        assert!(mw.is_host_allowed(&request));
    }

    #[test]
    fn test_host_subdomain_wildcard() {
        let mw = SecurityMiddleware {
            allowed_hosts: vec![".example.com".to_string()],
            ..SecurityMiddleware::default()
        };
        let sub = HttpRequest::builder()
            .meta("HTTP_HOST", "sub.example.com")
            .build();
        assert!(mw.is_host_allowed(&sub));

        let base = HttpRequest::builder()
            .meta("HTTP_HOST", "example.com")
            .build();
        assert!(mw.is_host_allowed(&base));

        let other = HttpRequest::builder()
            .meta("HTTP_HOST", "other.com")
            .build();
        assert!(!mw.is_host_allowed(&other));
    }

    #[test]
    fn test_host_case_insensitive() {
        let mw = SecurityMiddleware {
            allowed_hosts: vec!["Example.COM".to_string()],
            ..SecurityMiddleware::default()
        };
        let request = HttpRequest::builder()
            .meta("HTTP_HOST", "example.com")
            .build();
        assert!(mw.is_host_allowed(&request));
    }

    // ── HSTS header tests ───────────────────────────────────────────

    #[test]
    fn test_hsts_disabled() {
        let mw = SecurityMiddleware::default();
        assert!(mw.hsts_header_value().is_none());
    }

    #[test]
    fn test_hsts_basic() {
        let mw = SecurityMiddleware {
            secure_hsts_seconds: 31_536_000,
            ..SecurityMiddleware::default()
        };
        assert_eq!(mw.hsts_header_value().unwrap(), "max-age=31536000");
    }

    #[test]
    fn test_hsts_with_subdomains() {
        let mw = SecurityMiddleware {
            secure_hsts_seconds: 31_536_000,
            secure_hsts_include_subdomains: true,
            ..SecurityMiddleware::default()
        };
        assert_eq!(
            mw.hsts_header_value().unwrap(),
            "max-age=31536000; includeSubDomains"
        );
    }

    #[test]
    fn test_hsts_with_preload() {
        let mw = SecurityMiddleware {
            secure_hsts_seconds: 31_536_000,
            secure_hsts_include_subdomains: true,
            secure_hsts_preload: true,
            ..SecurityMiddleware::default()
        };
        assert_eq!(
            mw.hsts_header_value().unwrap(),
            "max-age=31536000; includeSubDomains; preload"
        );
    }

    // ── Middleware process_request tests ─────────────────────────────

    #[tokio::test]
    async fn test_process_request_allows_valid_host() {
        let mw = SecurityMiddleware {
            allowed_hosts: vec!["example.com".to_string()],
            ..SecurityMiddleware::default()
        };
        let mut request = HttpRequest::builder()
            .meta("HTTP_HOST", "example.com")
            .build();
        assert!(mw.process_request(&mut request).await.is_none());
    }

    #[tokio::test]
    async fn test_process_request_blocks_invalid_host() {
        let mw = SecurityMiddleware {
            allowed_hosts: vec!["example.com".to_string()],
            ..SecurityMiddleware::default()
        };
        let mut request = HttpRequest::builder()
            .meta("HTTP_HOST", "evil.com")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().status(), http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_process_request_ssl_redirect() {
        let mw = SecurityMiddleware {
            secure_ssl_redirect: true,
            ..SecurityMiddleware::default()
        };
        let mut request = HttpRequest::builder()
            .meta("HTTP_HOST", "example.com")
            .path("/path/")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
        let response = result.unwrap();
        assert_eq!(response.status(), http::StatusCode::FOUND);
        let location = response
            .headers()
            .get(http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(location, "https://example.com/path/");
    }

    #[tokio::test]
    async fn test_process_request_no_ssl_redirect_on_https() {
        let mw = SecurityMiddleware {
            secure_ssl_redirect: true,
            ..SecurityMiddleware::default()
        };
        let mut request = HttpRequest::builder()
            .scheme("https")
            .meta("HTTP_HOST", "example.com")
            .build();
        assert!(mw.process_request(&mut request).await.is_none());
    }

    // ── Middleware process_response tests ────────────────────────────

    #[tokio::test]
    async fn test_response_content_type_nosniff() {
        let mw = SecurityMiddleware::default();
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let response = mw.process_response(&request, response).await;
        assert_eq!(
            response
                .headers()
                .get(http::header::X_CONTENT_TYPE_OPTIONS)
                .unwrap()
                .to_str()
                .unwrap(),
            "nosniff"
        );
    }

    #[tokio::test]
    async fn test_response_no_nosniff_when_disabled() {
        let mw = SecurityMiddleware {
            secure_content_type_nosniff: false,
            ..SecurityMiddleware::default()
        };
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let response = mw.process_response(&request, response).await;
        assert!(response
            .headers()
            .get(http::header::X_CONTENT_TYPE_OPTIONS)
            .is_none());
    }

    #[tokio::test]
    async fn test_response_cross_origin_opener_policy() {
        let mw = SecurityMiddleware::default();
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let response = mw.process_response(&request, response).await;
        assert_eq!(
            response
                .headers()
                .get("cross-origin-opener-policy")
                .unwrap()
                .to_str()
                .unwrap(),
            "same-origin"
        );
    }

    #[tokio::test]
    async fn test_response_referrer_policy() {
        let mw = SecurityMiddleware::default();
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let response = mw.process_response(&request, response).await;
        assert_eq!(
            response
                .headers()
                .get(http::header::REFERRER_POLICY)
                .unwrap()
                .to_str()
                .unwrap(),
            "same-origin"
        );
    }

    #[tokio::test]
    async fn test_response_hsts_on_https() {
        let mw = SecurityMiddleware {
            secure_hsts_seconds: 31_536_000,
            ..SecurityMiddleware::default()
        };
        let request = HttpRequest::builder().scheme("https").build();
        let response = HttpResponse::ok("test");
        let response = mw.process_response(&request, response).await;
        assert_eq!(
            response
                .headers()
                .get(http::header::STRICT_TRANSPORT_SECURITY)
                .unwrap()
                .to_str()
                .unwrap(),
            "max-age=31536000"
        );
    }

    #[tokio::test]
    async fn test_response_no_hsts_on_http() {
        let mw = SecurityMiddleware {
            secure_hsts_seconds: 31_536_000,
            ..SecurityMiddleware::default()
        };
        let request = HttpRequest::builder().build(); // HTTP by default
        let response = HttpResponse::ok("test");
        let response = mw.process_response(&request, response).await;
        assert!(response
            .headers()
            .get(http::header::STRICT_TRANSPORT_SECURITY)
            .is_none());
    }

    #[tokio::test]
    async fn test_ssl_redirect_with_query_string() {
        let mw = SecurityMiddleware {
            secure_ssl_redirect: true,
            ..SecurityMiddleware::default()
        };
        let mut request = HttpRequest::builder()
            .meta("HTTP_HOST", "example.com")
            .path("/search/")
            .query_string("q=test")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
        let location = result
            .unwrap()
            .headers()
            .get(http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(location, "https://example.com/search/?q=test");
    }
}
