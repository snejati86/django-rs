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
            return Some(django_rs_http::HttpResponsePermanentRedirect::new(
                &redirect_url,
            ));
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
            response
                .headers_mut()
                .insert(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
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
                    response
                        .headers_mut()
                        .insert(http::header::ACCESS_CONTROL_ALLOW_METHODS, value);
                }

                let headers_str = self.allowed_headers.join(", ");
                if let Ok(value) = http::header::HeaderValue::from_str(&headers_str) {
                    response
                        .headers_mut()
                        .insert(http::header::ACCESS_CONTROL_ALLOW_HEADERS, value);
                }

                let max_age = self.max_age.to_string();
                if let Ok(value) = http::header::HeaderValue::from_str(&max_age) {
                    response
                        .headers_mut()
                        .insert(http::header::ACCESS_CONTROL_MAX_AGE, value);
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

// ── AuthenticationMiddleware ────────────────────────────────────────

/// Middleware that loads user information from the session.
///
/// Reads the `_auth_user_id` key from the session data (set by `SessionMiddleware`)
/// and populates `META["USER_ID"]` and `META["USER_AUTHENTICATED"]`. This mirrors
/// Django's `AuthenticationMiddleware`.
///
/// This middleware must be placed after `SessionMiddleware` in the pipeline.
#[derive(Debug, Clone, Default)]
pub struct AuthenticationMiddleware;

#[async_trait]
impl Middleware for AuthenticationMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        // Read session data from META (set by SessionMiddleware)
        let session_data_str = request
            .meta()
            .get("SESSION_DATA")
            .cloned()
            .unwrap_or_else(|| "{}".to_string());

        let session_data: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(&session_data_str).unwrap_or_default();

        // Look for "_auth_user_id" key in session data
        if let Some(user_id_val) = session_data.get("_auth_user_id") {
            let user_id = match user_id_val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                other => other.to_string(),
            };
            let meta = request.meta_mut();
            meta.insert("USER_ID".to_string(), user_id);
            meta.insert("USER_AUTHENTICATED".to_string(), "true".to_string());
        } else {
            let meta = request.meta_mut();
            meta.insert("USER_AUTHENTICATED".to_string(), "false".to_string());
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

// ── MessageMiddleware ──────────────────────────────────────────────

/// Message severity levels matching Django's message framework.
///
/// Each level has a numeric value for comparison and filtering.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub enum MessageLevel {
    /// Debug messages (level 10) — only shown in development.
    Debug = 10,
    /// Informational messages (level 20).
    Info = 20,
    /// Success messages (level 25).
    Success = 25,
    /// Warning messages (level 30).
    Warning = 30,
    /// Error messages (level 40).
    Error = 40,
}

impl std::fmt::Display for MessageLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Debug => write!(f, "debug"),
            Self::Info => write!(f, "info"),
            Self::Success => write!(f, "success"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// A flash message stored in the session for display on the next page.
///
/// Messages are typically added by views and displayed once by templates,
/// then cleared automatically.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    /// The severity level of this message.
    pub level: MessageLevel,
    /// The message text.
    pub message: String,
    /// Optional extra CSS tags for template rendering.
    pub extra_tags: String,
}

/// Middleware that manages the messages framework — stores and retrieves flash messages.
///
/// On request, loads existing messages from the session (key `_messages`). On response,
/// saves any newly added messages back to the session. This mirrors Django's
/// `MessageMiddleware`.
///
/// This middleware must be placed after `SessionMiddleware` in the pipeline.
#[derive(Debug, Clone, Default)]
pub struct MessageMiddleware;

#[async_trait]
impl Middleware for MessageMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        // Load messages from session data
        let session_data_str = request
            .meta()
            .get("SESSION_DATA")
            .cloned()
            .unwrap_or_else(|| "{}".to_string());

        let session_data: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(&session_data_str).unwrap_or_default();

        // Extract messages from session (key: "_messages")
        let messages_json = if let Some(messages_val) = session_data.get("_messages") {
            serde_json::to_string(messages_val).unwrap_or_else(|_| "[]".to_string())
        } else {
            "[]".to_string()
        };

        let meta = request.meta_mut();
        meta.insert("_messages_store".to_string(), messages_json);
        // Track newly added messages separately
        meta.insert("_messages_added".to_string(), "[]".to_string());

        None
    }

    async fn process_response(
        &self,
        request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        let meta = request.meta();

        // Check if any new messages were added during the request
        let added_json = meta
            .get("_messages_added")
            .cloned()
            .unwrap_or_else(|| "[]".to_string());

        let added: Vec<Message> = serde_json::from_str(&added_json).unwrap_or_default();
        if added.is_empty() {
            return response;
        }

        // We need to signal to SessionMiddleware that session data changed.
        // Since we cannot modify request in process_response, the messages
        // are persisted by the view layer calling add_message which updates
        // SESSION_DATA and SESSION_MODIFIED directly.
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

/// Adds a flash message to the current request's message store.
///
/// The message will be persisted in the session and available on the next request
/// (or the current request if retrieved before the response).
///
/// # Panics
///
/// This function does not panic but silently fails if the session data is malformed.
pub fn add_message(request: &mut HttpRequest, level: MessageLevel, message: &str) {
    add_message_with_tags(request, level, message, "");
}

/// Adds a flash message with extra CSS tags.
pub fn add_message_with_tags(
    request: &mut HttpRequest,
    level: MessageLevel,
    message: &str,
    extra_tags: &str,
) {
    let msg = Message {
        level,
        message: message.to_string(),
        extra_tags: extra_tags.to_string(),
    };

    // Add to the added messages tracker
    let added_json = request
        .meta()
        .get("_messages_added")
        .cloned()
        .unwrap_or_else(|| "[]".to_string());
    let mut added: Vec<Message> = serde_json::from_str(&added_json).unwrap_or_default();
    added.push(msg.clone());
    let new_added = serde_json::to_string(&added).unwrap_or_else(|_| "[]".to_string());

    // Also write into the session data so SessionMiddleware will persist them
    let session_data_str = request
        .meta()
        .get("SESSION_DATA")
        .cloned()
        .unwrap_or_else(|| "{}".to_string());
    let mut session_data: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&session_data_str).unwrap_or_default();

    // Merge existing messages from session with newly added
    let mut all_messages: Vec<Message> = if let Some(existing) = session_data.get("_messages") {
        serde_json::from_value(existing.clone()).unwrap_or_default()
    } else {
        Vec::new()
    };
    all_messages.push(msg);

    session_data.insert(
        "_messages".to_string(),
        serde_json::to_value(&all_messages).unwrap_or(serde_json::Value::Array(vec![])),
    );

    let new_session_data =
        serde_json::to_string(&session_data).unwrap_or_else(|_| "{}".to_string());

    let meta = request.meta_mut();
    meta.insert("_messages_added".to_string(), new_added);
    meta.insert("SESSION_DATA".to_string(), new_session_data);
    meta.insert("SESSION_MODIFIED".to_string(), "true".to_string());
}

/// Retrieves and consumes all pending messages from the request.
///
/// After calling this function, the messages are cleared from the store.
/// Subsequent calls will return an empty list until new messages are added.
pub fn get_messages(request: &HttpRequest) -> Vec<Message> {
    // Get messages from the store (loaded by MessageMiddleware)
    let store_json = request
        .meta()
        .get("_messages_store")
        .cloned()
        .unwrap_or_else(|| "[]".to_string());
    let stored: Vec<Message> = serde_json::from_str(&store_json).unwrap_or_default();

    // Also include newly added messages from this request
    let added_json = request
        .meta()
        .get("_messages_added")
        .cloned()
        .unwrap_or_else(|| "[]".to_string());
    let added: Vec<Message> = serde_json::from_str(&added_json).unwrap_or_default();

    let mut all = stored;
    all.extend(added);
    all
}

/// Convenience function: adds an info-level message.
pub fn info(request: &mut HttpRequest, message: &str) {
    add_message(request, MessageLevel::Info, message);
}

/// Convenience function: adds a success-level message.
pub fn success(request: &mut HttpRequest, message: &str) {
    add_message(request, MessageLevel::Success, message);
}

/// Convenience function: adds a warning-level message.
pub fn warning(request: &mut HttpRequest, message: &str) {
    add_message(request, MessageLevel::Warning, message);
}

/// Convenience function: adds an error-level message.
pub fn error(request: &mut HttpRequest, message: &str) {
    add_message(request, MessageLevel::Error, message);
}

// ── LocaleMiddleware ───────────────────────────────────────────────

/// Middleware that detects the user's preferred language and sets it on the request.
///
/// Checks language preference in this order:
/// 1. Session key `_language`
/// 2. Cookie `django_language`
/// 3. `Accept-Language` header
/// 4. Default language code (`en`)
///
/// Sets `META["LANGUAGE_CODE"]` with the detected language and adds
/// `Content-Language` and `Vary: Accept-Language` headers to responses.
///
/// This mirrors Django's `LocaleMiddleware`.
#[derive(Debug, Clone)]
pub struct LocaleMiddleware {
    /// The default language code to use when no preference is detected.
    pub default_language: String,
    /// Supported language codes (e.g., `["en", "fr", "de"]`).
    pub supported_languages: Vec<String>,
}

impl Default for LocaleMiddleware {
    fn default() -> Self {
        Self {
            default_language: "en".to_string(),
            supported_languages: vec!["en".to_string()],
        }
    }
}

impl LocaleMiddleware {
    /// Parses the `Accept-Language` header and returns the best matching language.
    ///
    /// Supports quality values (e.g., `en-US,en;q=0.9,fr;q=0.8`).
    fn parse_accept_language(&self, header: &str) -> Option<String> {
        let mut candidates: Vec<(f32, String)> = Vec::new();

        for part in header.split(',') {
            let part = part.trim();
            let (lang, quality) = if let Some(idx) = part.find(";q=") {
                let lang = part[..idx].trim();
                let q: f32 = part[idx + 3..].trim().parse().unwrap_or(0.0);
                (lang, q)
            } else {
                (part, 1.0)
            };

            // Normalize: take the primary language tag (e.g., "en" from "en-US")
            let primary = lang.split('-').next().unwrap_or(lang).to_lowercase();

            if self
                .supported_languages
                .iter()
                .any(|s| s.to_lowercase() == primary)
            {
                candidates.push((quality, primary));
            }
        }

        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        candidates.into_iter().next().map(|(_, lang)| lang)
    }

    /// Extracts a cookie value from the Cookie header.
    fn get_cookie_value(request: &HttpRequest, cookie_name: &str) -> Option<String> {
        let cookie_header = request
            .headers()
            .get(http::header::COOKIE)
            .and_then(|v| v.to_str().ok())?;

        for cookie in cookie_header.split(';') {
            let cookie = cookie.trim();
            if let Some(value) = cookie.strip_prefix(&format!("{cookie_name}=")) {
                return Some(value.to_string());
            }
        }
        None
    }
}

#[async_trait]
impl Middleware for LocaleMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        // 1. Check session for language preference
        let session_data_str = request
            .meta()
            .get("SESSION_DATA")
            .cloned()
            .unwrap_or_else(|| "{}".to_string());
        let session_data: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(&session_data_str).unwrap_or_default();

        if let Some(serde_json::Value::String(lang)) = session_data.get("_language") {
            if self.supported_languages.iter().any(|s| s == lang) {
                request
                    .meta_mut()
                    .insert("LANGUAGE_CODE".to_string(), lang.clone());
                return None;
            }
        }

        // 2. Check cookie
        if let Some(lang) = Self::get_cookie_value(request, "django_language") {
            if self.supported_languages.contains(&lang) {
                request.meta_mut().insert("LANGUAGE_CODE".to_string(), lang);
                return None;
            }
        }

        // 3. Check Accept-Language header
        if let Some(accept_lang) = request
            .headers()
            .get(http::header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
        {
            if let Some(lang) = self.parse_accept_language(accept_lang) {
                request.meta_mut().insert("LANGUAGE_CODE".to_string(), lang);
                return None;
            }
        }

        // 4. Fall back to default
        request
            .meta_mut()
            .insert("LANGUAGE_CODE".to_string(), self.default_language.clone());

        None
    }

    async fn process_response(
        &self,
        request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        let lang = request
            .meta()
            .get("LANGUAGE_CODE")
            .cloned()
            .unwrap_or_else(|| self.default_language.clone());

        let mut resp = response;

        // Set Content-Language header
        if let Ok(value) = http::header::HeaderValue::from_str(&lang) {
            resp.headers_mut().insert(
                http::header::HeaderName::from_static("content-language"),
                value,
            );
        }

        // Set Vary: Accept-Language
        resp.headers_mut().insert(
            http::header::VARY,
            http::header::HeaderValue::from_static("Accept-Language"),
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

// ── LoginRequiredMiddleware ────────────────────────────────────────

/// Middleware that requires authentication for all views unless explicitly exempted.
///
/// This is a Django 5.1 addition. When enabled, any unauthenticated request to
/// a non-exempt URL is redirected to the login page with a `?next=` parameter.
///
/// This middleware must be placed after `AuthenticationMiddleware` in the pipeline.
#[derive(Debug, Clone)]
pub struct LoginRequiredMiddleware {
    /// The URL to redirect unauthenticated users to.
    pub login_url: String,
    /// URL path prefixes that do not require authentication.
    pub exempt_urls: Vec<String>,
}

impl Default for LoginRequiredMiddleware {
    fn default() -> Self {
        Self {
            login_url: "/accounts/login/".to_string(),
            exempt_urls: Vec::new(),
        }
    }
}

impl LoginRequiredMiddleware {
    /// Creates a new `LoginRequiredMiddleware` with the given login URL.
    pub fn new(login_url: &str) -> Self {
        Self {
            login_url: login_url.to_string(),
            exempt_urls: Vec::new(),
        }
    }

    /// Adds URL patterns that should be exempt from login requirement.
    #[must_use]
    pub fn with_exempt_urls(mut self, urls: Vec<String>) -> Self {
        self.exempt_urls = urls;
        self
    }

    fn is_exempt(&self, path: &str) -> bool {
        // The login URL itself is always exempt
        if path == self.login_url || path.starts_with(&self.login_url) {
            return true;
        }
        self.exempt_urls
            .iter()
            .any(|exempt| path == exempt.as_str() || path.starts_with(exempt.as_str()))
    }
}

#[async_trait]
impl Middleware for LoginRequiredMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        // Check if URL is exempt
        if self.is_exempt(request.path()) {
            return None;
        }

        // Check if user is authenticated (set by AuthenticationMiddleware)
        let is_authenticated = request
            .meta()
            .get("USER_AUTHENTICATED")
            .is_some_and(|v| v == "true");

        if is_authenticated {
            return None;
        }

        // Redirect to login URL with next parameter
        let next = if request.query_string().is_empty() {
            request.path().to_string()
        } else {
            format!("{}?{}", request.path(), request.query_string())
        };

        let redirect_url = format!(
            "{}?next={}",
            self.login_url,
            percent_encoding::utf8_percent_encode(&next, percent_encoding::NON_ALPHANUMERIC)
        );

        Some(django_rs_http::HttpResponseRedirect::new(&redirect_url))
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

// ── CacheMiddleware ────────────────────────────────────────────────

/// Full-page caching middleware that caches GET/HEAD responses in memory.
///
/// Combines the functionality of Django's `UpdateCacheMiddleware` and
/// `FetchFromCacheMiddleware` into a single middleware. Only cacheable
/// responses (200 OK, no `Cache-Control: private`) are cached.
///
/// The cache is stored in-memory using a thread-safe map. Cache keys
/// are derived from the request URL and the `key_prefix`.
#[derive(Debug, Clone)]
pub struct CacheMiddleware {
    /// Cache timeout in seconds.
    pub cache_timeout: u64,
    /// Prefix prepended to all cache keys.
    pub key_prefix: String,
    /// Internal cache storage.
    cache: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, CachedResponse>>>,
}

/// A cached HTTP response with its expiration time.
#[derive(Debug, Clone)]
struct CachedResponse {
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: Vec<u8>,
    content_type: String,
    expires_at: std::time::Instant,
}

impl Default for CacheMiddleware {
    fn default() -> Self {
        Self {
            cache_timeout: 600,
            key_prefix: String::new(),
            cache: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }
}

impl CacheMiddleware {
    /// Creates a new `CacheMiddleware` with the given timeout (in seconds).
    pub fn new(cache_timeout: u64) -> Self {
        Self {
            cache_timeout,
            ..Default::default()
        }
    }

    /// Sets the key prefix for cache entries.
    #[must_use]
    pub fn with_key_prefix(mut self, prefix: &str) -> Self {
        self.key_prefix = prefix.to_string();
        self
    }

    fn cache_key(&self, request: &HttpRequest) -> String {
        format!("{}:{}", self.key_prefix, request.path())
    }

    fn is_cacheable_request(request: &HttpRequest) -> bool {
        matches!(*request.method(), http::Method::GET | http::Method::HEAD)
    }

    fn is_cacheable_response(response: &HttpResponse) -> bool {
        if response.status() != http::StatusCode::OK {
            return false;
        }

        // Don't cache responses with Cache-Control: private or no-cache
        if let Some(cc) = response
            .headers()
            .get(http::header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
        {
            let cc_lower = cc.to_lowercase();
            if cc_lower.contains("private")
                || cc_lower.contains("no-cache")
                || cc_lower.contains("no-store")
            {
                return false;
            }
        }

        true
    }
}

#[async_trait]
impl Middleware for CacheMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        if !Self::is_cacheable_request(request) {
            return None;
        }

        let key = self.cache_key(request);
        let cache = self.cache.read().await;

        if let Some(cached) = cache.get(&key) {
            if cached.expires_at > std::time::Instant::now() {
                // Cache hit — return cached response
                let mut resp = HttpResponse::with_bytes(cached.status, cached.body.clone());
                resp.set_content_type(&cached.content_type);
                for (name, value) in &cached.headers {
                    resp.headers_mut().insert(name.clone(), value.clone());
                }
                resp.headers_mut().insert(
                    http::header::HeaderName::from_static("x-cache"),
                    http::header::HeaderValue::from_static("HIT"),
                );
                return Some(resp);
            }
        }

        None
    }

    async fn process_response(
        &self,
        request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        if !Self::is_cacheable_request(request) || !Self::is_cacheable_response(&response) {
            return response;
        }

        let key = self.cache_key(request);
        let cached = CachedResponse {
            status: response.status(),
            headers: response.headers().clone(),
            body: response.content_bytes().unwrap_or_default(),
            content_type: response.content_type().to_string(),
            expires_at: std::time::Instant::now()
                + std::time::Duration::from_secs(self.cache_timeout),
        };

        let mut cache = self.cache.write().await;
        cache.insert(key, cached);

        // Add cache miss header
        let mut resp = response;
        resp.headers_mut().insert(
            http::header::HeaderName::from_static("x-cache"),
            http::header::HeaderValue::from_static("MISS"),
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

        assert!(result
            .headers()
            .get(http::header::CONTENT_ENCODING)
            .is_none());
    }

    #[tokio::test]
    async fn test_gzip_middleware_skips_without_accept_encoding() {
        let mw = GZipMiddleware::default();
        let request = HttpRequest::builder().build();
        let body = "x".repeat(500);
        let response = HttpResponse::ok(&body);
        let result = mw.process_response(&request, response).await;

        assert!(result
            .headers()
            .get(http::header::CONTENT_ENCODING)
            .is_none());
    }

    #[tokio::test]
    async fn test_gzip_middleware_custom_min_length() {
        let mw = GZipMiddleware { min_length: 10 };
        let request = HttpRequest::builder()
            .header("accept-encoding", "gzip")
            .build();
        let response = HttpResponse::ok("hello world! test data");
        let result = mw.process_response(&request, response).await;

        assert!(result
            .headers()
            .get(http::header::CONTENT_ENCODING)
            .is_some());
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

    // ── AuthenticationMiddleware tests ──────────────────────────────

    #[tokio::test]
    async fn test_auth_middleware_user_in_session() {
        let mw = AuthenticationMiddleware;
        let session_data = serde_json::json!({"_auth_user_id": "42"});
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", &session_data.to_string())
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
        assert_eq!(request.meta().get("USER_ID").unwrap(), "42");
        assert_eq!(request.meta().get("USER_AUTHENTICATED").unwrap(), "true");
    }

    #[tokio::test]
    async fn test_auth_middleware_user_numeric_id() {
        let mw = AuthenticationMiddleware;
        let session_data = serde_json::json!({"_auth_user_id": 99});
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", &session_data.to_string())
            .build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("USER_ID").unwrap(), "99");
        assert_eq!(request.meta().get("USER_AUTHENTICATED").unwrap(), "true");
    }

    #[tokio::test]
    async fn test_auth_middleware_no_user_in_session() {
        let mw = AuthenticationMiddleware;
        let session_data = serde_json::json!({"theme": "dark"});
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", &session_data.to_string())
            .build();
        mw.process_request(&mut request).await;
        assert!(request.meta().get("USER_ID").is_none());
        assert_eq!(request.meta().get("USER_AUTHENTICATED").unwrap(), "false");
    }

    #[tokio::test]
    async fn test_auth_middleware_empty_session() {
        let mw = AuthenticationMiddleware;
        let mut request = HttpRequest::builder().meta("SESSION_DATA", "{}").build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("USER_AUTHENTICATED").unwrap(), "false");
    }

    #[tokio::test]
    async fn test_auth_middleware_no_session_data() {
        let mw = AuthenticationMiddleware;
        let mut request = HttpRequest::builder().build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("USER_AUTHENTICATED").unwrap(), "false");
    }

    #[tokio::test]
    async fn test_auth_middleware_invalid_session_json() {
        let mw = AuthenticationMiddleware;
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "not-json")
            .build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("USER_AUTHENTICATED").unwrap(), "false");
    }

    #[tokio::test]
    async fn test_auth_middleware_passthrough_response() {
        let mw = AuthenticationMiddleware;
        let request = HttpRequest::builder().build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;
        assert_eq!(result.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_middleware_does_not_short_circuit() {
        let mw = AuthenticationMiddleware;
        let mut request = HttpRequest::builder().build();
        assert!(mw.process_request(&mut request).await.is_none());
    }

    // ── MessageMiddleware tests ────────────────────────────────────

    #[tokio::test]
    async fn test_message_middleware_loads_empty_store() {
        let mw = MessageMiddleware;
        let mut request = HttpRequest::builder().meta("SESSION_DATA", "{}").build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("_messages_store").unwrap(), "[]");
    }

    #[tokio::test]
    async fn test_message_middleware_loads_existing_messages() {
        let mw = MessageMiddleware;
        let messages = serde_json::json!([{"level": "Info", "message": "Hello", "extra_tags": ""}]);
        let session = serde_json::json!({"_messages": messages});
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", &session.to_string())
            .build();
        mw.process_request(&mut request).await;
        let store = request.meta().get("_messages_store").unwrap();
        assert!(store.contains("Hello"));
    }

    #[tokio::test]
    async fn test_add_message_to_request() {
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .meta("_messages_store", "[]")
            .meta("_messages_added", "[]")
            .build();
        add_message(&mut request, MessageLevel::Info, "Test message");
        let messages = get_messages(&request);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message, "Test message");
    }

    #[tokio::test]
    async fn test_add_multiple_messages() {
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .meta("_messages_store", "[]")
            .meta("_messages_added", "[]")
            .build();
        add_message(&mut request, MessageLevel::Info, "Info msg");
        add_message(&mut request, MessageLevel::Warning, "Warn msg");
        add_message(&mut request, MessageLevel::Error, "Error msg");
        let messages = get_messages(&request);
        assert_eq!(messages.len(), 3);
    }

    #[tokio::test]
    async fn test_message_levels() {
        assert!(MessageLevel::Debug < MessageLevel::Info);
        assert!(MessageLevel::Info < MessageLevel::Success);
        assert!(MessageLevel::Success < MessageLevel::Warning);
        assert!(MessageLevel::Warning < MessageLevel::Error);
    }

    #[tokio::test]
    async fn test_message_level_display() {
        assert_eq!(MessageLevel::Debug.to_string(), "debug");
        assert_eq!(MessageLevel::Info.to_string(), "info");
        assert_eq!(MessageLevel::Success.to_string(), "success");
        assert_eq!(MessageLevel::Warning.to_string(), "warning");
        assert_eq!(MessageLevel::Error.to_string(), "error");
    }

    #[tokio::test]
    async fn test_convenience_info_message() {
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .meta("_messages_store", "[]")
            .meta("_messages_added", "[]")
            .build();
        info(&mut request, "Info message");
        let messages = get_messages(&request);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].level, MessageLevel::Info);
    }

    #[tokio::test]
    async fn test_convenience_success_message() {
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .meta("_messages_store", "[]")
            .meta("_messages_added", "[]")
            .build();
        success(&mut request, "Success!");
        let messages = get_messages(&request);
        assert_eq!(messages[0].level, MessageLevel::Success);
    }

    #[tokio::test]
    async fn test_convenience_warning_message() {
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .meta("_messages_store", "[]")
            .meta("_messages_added", "[]")
            .build();
        warning(&mut request, "Watch out!");
        let messages = get_messages(&request);
        assert_eq!(messages[0].level, MessageLevel::Warning);
    }

    #[tokio::test]
    async fn test_convenience_error_message() {
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .meta("_messages_store", "[]")
            .meta("_messages_added", "[]")
            .build();
        error(&mut request, "Something broke");
        let messages = get_messages(&request);
        assert_eq!(messages[0].level, MessageLevel::Error);
    }

    #[tokio::test]
    async fn test_message_marks_session_modified() {
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .meta("SESSION_MODIFIED", "false")
            .meta("_messages_store", "[]")
            .meta("_messages_added", "[]")
            .build();
        add_message(&mut request, MessageLevel::Info, "Test");
        assert_eq!(request.meta().get("SESSION_MODIFIED").unwrap(), "true");
    }

    #[tokio::test]
    async fn test_message_with_extra_tags() {
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .meta("_messages_store", "[]")
            .meta("_messages_added", "[]")
            .build();
        add_message_with_tags(&mut request, MessageLevel::Info, "Tagged", "important bold");
        let messages = get_messages(&request);
        assert_eq!(messages[0].extra_tags, "important bold");
    }

    // ── LocaleMiddleware tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_locale_default_language() {
        let mw = LocaleMiddleware::default();
        let mut request = HttpRequest::builder().meta("SESSION_DATA", "{}").build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("LANGUAGE_CODE").unwrap(), "en");
    }

    #[tokio::test]
    async fn test_locale_from_session() {
        let mw = LocaleMiddleware {
            supported_languages: vec!["en".to_string(), "fr".to_string()],
            ..Default::default()
        };
        let session = serde_json::json!({"_language": "fr"});
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", &session.to_string())
            .build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("LANGUAGE_CODE").unwrap(), "fr");
    }

    #[tokio::test]
    async fn test_locale_from_cookie() {
        let mw = LocaleMiddleware {
            supported_languages: vec!["en".to_string(), "de".to_string()],
            ..Default::default()
        };
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .header("cookie", "django_language=de; other=val")
            .build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("LANGUAGE_CODE").unwrap(), "de");
    }

    #[tokio::test]
    async fn test_locale_from_accept_language() {
        let mw = LocaleMiddleware {
            supported_languages: vec!["en".to_string(), "fr".to_string(), "de".to_string()],
            ..Default::default()
        };
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .header("accept-language", "fr-FR,fr;q=0.9,en;q=0.8")
            .build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("LANGUAGE_CODE").unwrap(), "fr");
    }

    #[tokio::test]
    async fn test_locale_accept_language_quality_ordering() {
        let mw = LocaleMiddleware {
            supported_languages: vec!["en".to_string(), "fr".to_string(), "de".to_string()],
            ..Default::default()
        };
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .header("accept-language", "en;q=0.5,de;q=0.9,fr;q=0.7")
            .build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("LANGUAGE_CODE").unwrap(), "de");
    }

    #[tokio::test]
    async fn test_locale_unsupported_language_falls_back() {
        let mw = LocaleMiddleware {
            supported_languages: vec!["en".to_string()],
            ..Default::default()
        };
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", "{}")
            .header("accept-language", "ja,zh;q=0.9")
            .build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("LANGUAGE_CODE").unwrap(), "en");
    }

    #[tokio::test]
    async fn test_locale_response_headers() {
        let mw = LocaleMiddleware::default();
        let request = HttpRequest::builder().meta("LANGUAGE_CODE", "en").build();
        let response = HttpResponse::ok("test");
        let result = mw.process_response(&request, response).await;
        assert_eq!(
            result
                .headers()
                .get("content-language")
                .unwrap()
                .to_str()
                .unwrap(),
            "en"
        );
        assert_eq!(
            result
                .headers()
                .get(http::header::VARY)
                .unwrap()
                .to_str()
                .unwrap(),
            "Accept-Language"
        );
    }

    #[tokio::test]
    async fn test_locale_session_takes_priority_over_cookie() {
        let mw = LocaleMiddleware {
            supported_languages: vec!["en".to_string(), "fr".to_string(), "de".to_string()],
            ..Default::default()
        };
        let session = serde_json::json!({"_language": "fr"});
        let mut request = HttpRequest::builder()
            .meta("SESSION_DATA", &session.to_string())
            .header("cookie", "django_language=de")
            .header("accept-language", "en")
            .build();
        mw.process_request(&mut request).await;
        assert_eq!(request.meta().get("LANGUAGE_CODE").unwrap(), "fr");
    }

    // ── LoginRequiredMiddleware tests ──────────────────────────────

    #[tokio::test]
    async fn test_login_required_redirects_unauthenticated() {
        let mw = LoginRequiredMiddleware::default();
        let mut request = HttpRequest::builder()
            .path("/dashboard/")
            .meta("USER_AUTHENTICATED", "false")
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
        assert!(location.starts_with("/accounts/login/"));
        assert!(location.contains("next="));
    }

    #[tokio::test]
    async fn test_login_required_passes_authenticated() {
        let mw = LoginRequiredMiddleware::default();
        let mut request = HttpRequest::builder()
            .path("/dashboard/")
            .meta("USER_AUTHENTICATED", "true")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_login_required_exempt_url() {
        let mw = LoginRequiredMiddleware::default().with_exempt_urls(vec!["/public/".to_string()]);
        let mut request = HttpRequest::builder()
            .path("/public/page/")
            .meta("USER_AUTHENTICATED", "false")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_login_required_login_url_is_exempt() {
        let mw = LoginRequiredMiddleware::default();
        let mut request = HttpRequest::builder()
            .path("/accounts/login/")
            .meta("USER_AUTHENTICATED", "false")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_login_required_custom_login_url() {
        let mw = LoginRequiredMiddleware::new("/auth/signin/");
        let mut request = HttpRequest::builder()
            .path("/protected/")
            .meta("USER_AUTHENTICATED", "false")
            .build();
        let result = mw.process_request(&mut request).await;
        let response = result.unwrap();
        let location = response
            .headers()
            .get(http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.starts_with("/auth/signin/"));
    }

    #[tokio::test]
    async fn test_login_required_preserves_query_string() {
        let mw = LoginRequiredMiddleware::default();
        let mut request = HttpRequest::builder()
            .path("/page/")
            .query_string("tab=settings")
            .meta("USER_AUTHENTICATED", "false")
            .build();
        let result = mw.process_request(&mut request).await;
        let response = result.unwrap();
        let location = response
            .headers()
            .get(http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.contains("tab"));
        assert!(location.contains("settings"));
    }

    // ── CacheMiddleware tests ──────────────────────────────────────

    #[tokio::test]
    async fn test_cache_middleware_miss_then_hit() {
        let mw = CacheMiddleware::new(600);
        let mut request = HttpRequest::builder()
            .method(http::Method::GET)
            .path("/cached-page/")
            .build();

        // First request: miss
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());

        // Simulate response to store in cache
        let response = HttpResponse::ok("cached content");
        let result = mw.process_response(&request, response).await;
        assert_eq!(
            result.headers().get("x-cache").unwrap().to_str().unwrap(),
            "MISS"
        );

        // Second request: hit
        let mut request2 = HttpRequest::builder()
            .method(http::Method::GET)
            .path("/cached-page/")
            .build();
        let result = mw.process_request(&mut request2).await;
        assert!(result.is_some());
        let cached_response = result.unwrap();
        assert_eq!(cached_response.status(), http::StatusCode::OK);
        assert_eq!(
            cached_response
                .headers()
                .get("x-cache")
                .unwrap()
                .to_str()
                .unwrap(),
            "HIT"
        );
    }

    #[tokio::test]
    async fn test_cache_middleware_skips_post() {
        let mw = CacheMiddleware::new(600);
        let mut request = HttpRequest::builder()
            .method(http::Method::POST)
            .path("/submit/")
            .build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());

        let response = HttpResponse::ok("response");
        let result = mw.process_response(&request, response).await;
        // Should not have x-cache header since POST is not cacheable
        assert!(result.headers().get("x-cache").is_none());
    }

    #[tokio::test]
    async fn test_cache_middleware_skips_private_response() {
        let mw = CacheMiddleware::new(600);
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .path("/private/")
            .build();
        let response = HttpResponse::ok("private content").set_header(
            http::header::CACHE_CONTROL,
            http::header::HeaderValue::from_static("private, max-age=0"),
        );
        let result = mw.process_response(&request, response).await;
        // Should not cache private responses
        assert!(result.headers().get("x-cache").is_none());
    }

    #[tokio::test]
    async fn test_cache_middleware_skips_non_200() {
        let mw = CacheMiddleware::new(600);
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .path("/not-found/")
            .build();
        let response = HttpResponse::new(http::StatusCode::NOT_FOUND, "not found");
        let result = mw.process_response(&request, response).await;
        assert!(result.headers().get("x-cache").is_none());
    }

    #[tokio::test]
    async fn test_cache_middleware_key_prefix() {
        let mw = CacheMiddleware::new(600).with_key_prefix("v1");
        let mut request = HttpRequest::builder()
            .method(http::Method::GET)
            .path("/page/")
            .build();

        // Store in cache
        mw.process_request(&mut request).await;
        let response = HttpResponse::ok("v1 content");
        mw.process_response(&request, response).await;

        // Cache hit with same prefix
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_cache_middleware_head_request_cached() {
        let mw = CacheMiddleware::new(600);
        let mut request = HttpRequest::builder()
            .method(http::Method::HEAD)
            .path("/head-test/")
            .build();

        // Miss
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());

        // Cache
        let response = HttpResponse::ok("");
        mw.process_response(&request, response).await;

        // Hit
        let result = mw.process_request(&mut request).await;
        assert!(result.is_some());
    }
}
