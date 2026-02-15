//! HTTP response types.
//!
//! This module provides [`HttpResponse`] and convenience types for common
//! response patterns, mirroring Django's `django.http.HttpResponse` and its
//! subclasses (`JsonResponse`, `HttpResponseRedirect`, etc.).


use std::pin::Pin;

use axum::response::IntoResponse;
use bytes::Bytes;
use futures_core::Stream;
use http::{HeaderMap, HeaderValue, StatusCode};

use django_rs_core::DjangoError;

/// The body content of an HTTP response.
///
/// Supports plain bytes, text, and streaming bodies.
pub enum ResponseContent {
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// UTF-8 text.
    Text(String),
    /// A streaming body.
    Streaming(Pin<Box<dyn Stream<Item = Result<Bytes, DjangoError>> + Send>>),
}

impl std::fmt::Debug for ResponseContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytes(b) => f.debug_tuple("Bytes").field(&b.len()).finish(),
            Self::Text(t) => f
                .debug_tuple("Text")
                .field(&t.chars().take(100).collect::<String>())
                .finish(),
            Self::Streaming(_) => f.debug_tuple("Streaming").finish(),
        }
    }
}

/// An HTTP response, modeled after Django's `HttpResponse`.
///
/// Supports setting status codes, headers, content type, charset, and body content.
/// All response types can be converted to an Axum response via [`IntoResponse`].
///
/// # Examples
///
/// ```
/// use django_rs_http::HttpResponse;
///
/// let response = HttpResponse::ok("Hello, World!");
/// assert_eq!(response.status(), http::StatusCode::OK);
/// ```
pub struct HttpResponse {
    status: StatusCode,
    headers: HeaderMap,
    content: ResponseContent,
    charset: String,
    content_type: String,
}

impl std::fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("content_type", &self.content_type)
            .field("charset", &self.charset)
            .field("content", &self.content)
            .finish_non_exhaustive()
    }
}

impl HttpResponse {
    /// Creates a new `HttpResponse` with the given status code and text body.
    pub fn new(status: StatusCode, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            content: ResponseContent::Text(body.into()),
            charset: "utf-8".to_string(),
            content_type: "text/html".to_string(),
        }
    }

    /// Creates a new `HttpResponse` with the given status code and byte body.
    pub fn with_bytes(status: StatusCode, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            content: ResponseContent::Bytes(body),
            charset: "utf-8".to_string(),
            content_type: "application/octet-stream".to_string(),
        }
    }

    /// Creates a 200 OK response with the given body.
    pub fn ok(body: impl Into<String>) -> Self {
        Self::new(StatusCode::OK, body)
    }

    /// Creates a 404 Not Found response.
    pub fn not_found(body: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, body)
    }

    /// Creates a 403 Forbidden response.
    pub fn forbidden(body: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, body)
    }

    /// Creates a 400 Bad Request response.
    pub fn bad_request(body: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, body)
    }

    /// Creates a 500 Internal Server Error response.
    pub fn server_error(body: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, body)
    }

    /// Creates a 405 Method Not Allowed response with the list of permitted methods.
    pub fn not_allowed(permitted_methods: &[&str]) -> Self {
        let body = format!("Method Not Allowed. Permitted: {}", permitted_methods.join(", "));
        let mut response = Self::new(StatusCode::METHOD_NOT_ALLOWED, body);
        if let Ok(value) = HeaderValue::from_str(&permitted_methods.join(", ")) {
            response.headers.insert(http::header::ALLOW, value);
        }
        response
    }

    /// Returns the status code.
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Sets the status code.
    pub fn set_status(&mut self, status: StatusCode) {
        self.status = status;
    }

    /// Returns a reference to the headers.
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns a mutable reference to the headers.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    /// Adds a header to the response.
    #[must_use]
    pub fn set_header(
        mut self,
        name: http::header::HeaderName,
        value: HeaderValue,
    ) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// Returns the charset.
    pub fn charset(&self) -> &str {
        &self.charset
    }

    /// Sets the charset.
    pub fn set_charset(&mut self, charset: impl Into<String>) {
        self.charset = charset.into();
    }

    /// Returns the content type.
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    /// Sets the content type.
    pub fn set_content_type(&mut self, content_type: impl Into<String>) {
        self.content_type = content_type.into();
    }

    /// Returns the response body as a reference to the content enum.
    pub const fn content(&self) -> &ResponseContent {
        &self.content
    }

    /// Returns the body as bytes, if available (not streaming).
    pub fn content_bytes(&self) -> Option<Vec<u8>> {
        match &self.content {
            ResponseContent::Bytes(b) => Some(b.clone()),
            ResponseContent::Text(t) => Some(t.as_bytes().to_vec()),
            ResponseContent::Streaming(_) => None,
        }
    }

    /// Returns the full content type header value including charset.
    fn full_content_type(&self) -> String {
        if self.content_type.starts_with("text/") || self.content_type.contains("json") {
            format!("{}; charset={}", self.content_type, self.charset)
        } else {
            self.content_type.clone()
        }
    }
}

impl IntoResponse for HttpResponse {
    fn into_response(self) -> axum::response::Response {
        let mut builder = axum::response::Response::builder().status(self.status);

        // Set content type
        if let Ok(ct) = HeaderValue::from_str(&self.full_content_type()) {
            builder = builder.header(http::header::CONTENT_TYPE, ct);
        }

        // Copy custom headers
        // We'll add them after building
        let response = match self.content {
            ResponseContent::Text(text) => builder
                .body(axum::body::Body::from(text))
                .unwrap_or_else(|_| {
                    axum::response::Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(axum::body::Body::from("Internal Server Error"))
                        .expect("fallback response should always be valid")
                }),
            ResponseContent::Bytes(bytes) => builder
                .body(axum::body::Body::from(bytes))
                .unwrap_or_else(|_| {
                    axum::response::Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(axum::body::Body::from("Internal Server Error"))
                        .expect("fallback response should always be valid")
                }),
            ResponseContent::Streaming(stream) => {
                let body = axum::body::Body::from_stream(stream);
                builder.body(body).unwrap_or_else(|_| {
                    axum::response::Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(axum::body::Body::from("Internal Server Error"))
                        .expect("fallback response should always be valid")
                })
            }
        };

        let (mut parts, body) = response.into_parts();
        // Merge custom headers
        for (key, value) in &self.headers {
            parts.headers.insert(key, value.clone());
        }
        axum::response::Response::from_parts(parts, body)
    }
}

/// A JSON response, equivalent to Django's `JsonResponse`.
///
/// Serializes the given data as JSON and sets the content type to `application/json`.
pub struct JsonResponse;

impl JsonResponse {
    /// Creates a new JSON response from a serializable value.
    ///
    /// # Errors
    ///
    /// Returns an error response if serialization fails.
    pub fn new<T: serde::Serialize>(data: &T) -> HttpResponse {
        match serde_json::to_string(data) {
            Ok(json) => {
                let mut response = HttpResponse::new(StatusCode::OK, json);
                response.set_content_type("application/json");
                response
            }
            Err(e) => HttpResponse::server_error(format!("JSON serialization error: {e}")),
        }
    }

    /// Creates a new JSON response with a custom status code.
    ///
    /// # Errors
    ///
    /// Returns an error response if serialization fails.
    pub fn with_status<T: serde::Serialize>(status: StatusCode, data: &T) -> HttpResponse {
        match serde_json::to_string(data) {
            Ok(json) => {
                let mut response = HttpResponse::new(status, json);
                response.set_content_type("application/json");
                response
            }
            Err(e) => HttpResponse::server_error(format!("JSON serialization error: {e}")),
        }
    }
}

/// An HTTP redirect response (302 Found).
///
/// Equivalent to Django's `HttpResponseRedirect`.
pub struct HttpResponseRedirect;

impl HttpResponseRedirect {
    /// Creates a 302 Found redirect to the given URL.
    pub fn new(url: &str) -> HttpResponse {
        let mut response = HttpResponse::new(StatusCode::FOUND, "");
        if let Ok(value) = HeaderValue::from_str(url) {
            response.headers.insert(http::header::LOCATION, value);
        }
        response
    }
}

/// An HTTP permanent redirect response (301 Moved Permanently).
///
/// Equivalent to Django's `HttpResponsePermanentRedirect`.
pub struct HttpResponsePermanentRedirect;

impl HttpResponsePermanentRedirect {
    /// Creates a 301 Moved Permanently redirect to the given URL.
    pub fn new(url: &str) -> HttpResponse {
        let mut response = HttpResponse::new(StatusCode::MOVED_PERMANENTLY, "");
        if let Ok(value) = HeaderValue::from_str(url) {
            response.headers.insert(http::header::LOCATION, value);
        }
        response
    }
}

/// A 404 Not Found response.
///
/// Equivalent to Django's `HttpResponseNotFound`.
pub struct HttpResponseNotFound;

impl HttpResponseNotFound {
    /// Creates a 404 Not Found response with the given body.
    pub fn new(body: impl Into<String>) -> HttpResponse {
        HttpResponse::not_found(body)
    }
}

/// A 403 Forbidden response.
///
/// Equivalent to Django's `HttpResponseForbidden`.
pub struct HttpResponseForbidden;

impl HttpResponseForbidden {
    /// Creates a 403 Forbidden response with the given body.
    pub fn new(body: impl Into<String>) -> HttpResponse {
        HttpResponse::forbidden(body)
    }
}

/// A 500 Internal Server Error response.
///
/// Equivalent to Django's `HttpResponseServerError`.
pub struct HttpResponseServerError;

impl HttpResponseServerError {
    /// Creates a 500 Internal Server Error response with the given body.
    pub fn new(body: impl Into<String>) -> HttpResponse {
        HttpResponse::server_error(body)
    }
}

/// A 405 Method Not Allowed response.
///
/// Equivalent to Django's `HttpResponseNotAllowed`.
pub struct HttpResponseNotAllowed;

impl HttpResponseNotAllowed {
    /// Creates a 405 Method Not Allowed response with the list of permitted methods.
    pub fn new(permitted_methods: &[&str]) -> HttpResponse {
        HttpResponse::not_allowed(permitted_methods)
    }
}

/// A file download response that streams the file content.
///
/// Equivalent to Django's `FileResponse`.
pub struct FileResponse;

impl FileResponse {
    /// Creates a response that streams the contents of a file.
    ///
    /// The file is read asynchronously. The content type is inferred from the
    /// file extension when possible.
    pub fn new(path: &std::path::Path) -> HttpResponse {
        // Determine content type from extension
        let content_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map_or("application/octet-stream", mime_from_extension);

        let path_owned = path.to_path_buf();

        // For simplicity, we read the file synchronously in the constructor.
        // In production, you'd use tokio::fs::File with streaming.
        match std::fs::read(&path_owned) {
            Ok(data) => {
                let mut response = HttpResponse::with_bytes(StatusCode::OK, data);
                response.set_content_type(content_type);

                // Set Content-Disposition for download
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if let Ok(value) =
                        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
                    {
                        response
                            .headers
                            .insert(http::header::CONTENT_DISPOSITION, value);
                    }
                }

                response
            }
            Err(e) => HttpResponse::server_error(format!("Failed to read file: {e}")),
        }
    }
}

/// A streaming HTTP response.
///
/// Equivalent to Django's `StreamingHttpResponse`.
pub struct StreamingHttpResponse;

impl StreamingHttpResponse {
    /// Creates a streaming response from an async stream.
    pub fn new(
        stream: Pin<Box<dyn Stream<Item = Result<Bytes, DjangoError>> + Send>>,
    ) -> HttpResponse {
        HttpResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            content: ResponseContent::Streaming(stream),
            charset: "utf-8".to_string(),
            content_type: "application/octet-stream".to_string(),
        }
    }

    /// Creates a streaming response with a custom content type.
    pub fn with_content_type(
        content_type: &str,
        stream: Pin<Box<dyn Stream<Item = Result<Bytes, DjangoError>> + Send>>,
    ) -> HttpResponse {
        let mut response = Self::new(stream);
        response.set_content_type(content_type);
        response
    }
}

/// Infers a MIME type from a file extension.
fn mime_from_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_response_ok() {
        let resp = HttpResponse::ok("Hello");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.content_type(), "text/html");
        assert_eq!(resp.charset(), "utf-8");
        assert_eq!(resp.content_bytes().unwrap(), b"Hello");
    }

    #[test]
    fn test_http_response_not_found() {
        let resp = HttpResponse::not_found("Not Found");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_http_response_forbidden() {
        let resp = HttpResponse::forbidden("Forbidden");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_http_response_bad_request() {
        let resp = HttpResponse::bad_request("Bad");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_http_response_server_error() {
        let resp = HttpResponse::server_error("Error");
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_http_response_not_allowed() {
        let resp = HttpResponse::not_allowed(&["GET", "POST"]);
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(resp
            .headers()
            .get(http::header::ALLOW)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("GET"));
    }

    #[test]
    fn test_http_response_set_header() {
        let resp = HttpResponse::ok("test").set_header(
            http::header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        );
        assert_eq!(
            resp.headers()
                .get(http::header::CACHE_CONTROL)
                .unwrap()
                .to_str()
                .unwrap(),
            "no-cache"
        );
    }

    #[test]
    fn test_http_response_set_content_type() {
        let mut resp = HttpResponse::ok("test");
        resp.set_content_type("text/plain");
        assert_eq!(resp.content_type(), "text/plain");
    }

    #[test]
    fn test_http_response_set_charset() {
        let mut resp = HttpResponse::ok("test");
        resp.set_charset("iso-8859-1");
        assert_eq!(resp.charset(), "iso-8859-1");
    }

    #[test]
    fn test_http_response_set_status() {
        let mut resp = HttpResponse::ok("test");
        resp.set_status(StatusCode::CREATED);
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[test]
    fn test_http_response_with_bytes() {
        let resp = HttpResponse::with_bytes(StatusCode::OK, vec![1, 2, 3]);
        assert_eq!(resp.content_bytes().unwrap(), vec![1, 2, 3]);
        assert_eq!(resp.content_type(), "application/octet-stream");
    }

    #[test]
    fn test_json_response() {
        let data = serde_json::json!({"key": "value"});
        let resp = JsonResponse::new(&data);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.content_type(), "application/json");
        let body = String::from_utf8(resp.content_bytes().unwrap()).unwrap();
        assert!(body.contains("\"key\""));
        assert!(body.contains("\"value\""));
    }

    #[test]
    fn test_json_response_with_status() {
        let data = serde_json::json!({"created": true});
        let resp = JsonResponse::with_status(StatusCode::CREATED, &data);
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.content_type(), "application/json");
    }

    #[test]
    fn test_redirect_response() {
        let resp = HttpResponseRedirect::new("/new-location/");
        assert_eq!(resp.status(), StatusCode::FOUND);
        assert_eq!(
            resp.headers()
                .get(http::header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "/new-location/"
        );
    }

    #[test]
    fn test_permanent_redirect_response() {
        let resp = HttpResponsePermanentRedirect::new("/permanent/");
        assert_eq!(resp.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(
            resp.headers()
                .get(http::header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "/permanent/"
        );
    }

    #[test]
    fn test_not_found_response() {
        let resp = HttpResponseNotFound::new("Page not found");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_forbidden_response() {
        let resp = HttpResponseForbidden::new("Access denied");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_server_error_response() {
        let resp = HttpResponseServerError::new("Something went wrong");
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_not_allowed_response() {
        let resp = HttpResponseNotAllowed::new(&["GET", "HEAD"]);
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
        let allow = resp
            .headers()
            .get(http::header::ALLOW)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(allow.contains("GET"));
        assert!(allow.contains("HEAD"));
    }

    #[test]
    fn test_response_content_debug() {
        let text = ResponseContent::Text("hello".to_string());
        let debug = format!("{text:?}");
        assert!(debug.contains("hello"));

        let bytes = ResponseContent::Bytes(vec![1, 2, 3]);
        let debug = format!("{bytes:?}");
        assert!(debug.contains('3'));
    }

    #[test]
    fn test_http_response_debug() {
        let resp = HttpResponse::ok("test");
        let debug = format!("{resp:?}");
        assert!(debug.contains("200"));
    }

    #[test]
    fn test_full_content_type_text() {
        let resp = HttpResponse::ok("test");
        assert_eq!(resp.full_content_type(), "text/html; charset=utf-8");
    }

    #[test]
    fn test_full_content_type_binary() {
        let resp = HttpResponse::with_bytes(StatusCode::OK, vec![]);
        assert_eq!(resp.full_content_type(), "application/octet-stream");
    }

    #[test]
    fn test_full_content_type_json() {
        let data = serde_json::json!({"k": "v"});
        let resp = JsonResponse::new(&data);
        assert_eq!(
            resp.full_content_type(),
            "application/json; charset=utf-8"
        );
    }

    #[test]
    fn test_into_response() {
        let resp = HttpResponse::ok("Hello, World!");
        let axum_resp = resp.into_response();
        assert_eq!(axum_resp.status(), StatusCode::OK);
        let ct = axum_resp
            .headers()
            .get(http::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("text/html"));
        assert!(ct.contains("utf-8"));
    }

    #[test]
    fn test_into_response_with_custom_header() {
        let resp = HttpResponse::ok("test").set_header(
            http::header::HeaderName::from_static("x-custom"),
            HeaderValue::from_static("custom-value"),
        );
        let axum_resp = resp.into_response();
        assert_eq!(
            axum_resp
                .headers()
                .get("x-custom")
                .unwrap()
                .to_str()
                .unwrap(),
            "custom-value"
        );
    }

    #[test]
    fn test_into_response_redirect() {
        let resp = HttpResponseRedirect::new("/other/");
        let axum_resp = resp.into_response();
        assert_eq!(axum_resp.status(), StatusCode::FOUND);
        assert_eq!(
            axum_resp
                .headers()
                .get(http::header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "/other/"
        );
    }

    #[test]
    fn test_mime_from_extension() {
        assert_eq!(mime_from_extension("html"), "text/html");
        assert_eq!(mime_from_extension("json"), "application/json");
        assert_eq!(mime_from_extension("png"), "image/png");
        assert_eq!(mime_from_extension("pdf"), "application/pdf");
        assert_eq!(mime_from_extension("unknown"), "application/octet-stream");
        assert_eq!(mime_from_extension("CSS"), "text/css");
        assert_eq!(mime_from_extension("JS"), "application/javascript");
    }

    #[test]
    fn test_content_bytes_text() {
        let resp = HttpResponse::ok("hello");
        assert_eq!(resp.content_bytes().unwrap(), b"hello");
    }

    #[test]
    fn test_content_bytes_binary() {
        let resp = HttpResponse::with_bytes(StatusCode::OK, vec![0xFF, 0xFE]);
        assert_eq!(resp.content_bytes().unwrap(), vec![0xFF, 0xFE]);
    }

    #[test]
    fn test_headers_mut() {
        let mut resp = HttpResponse::ok("test");
        resp.headers_mut()
            .insert(http::header::ETAG, HeaderValue::from_static("\"abc\""));
        assert_eq!(
            resp.headers()
                .get(http::header::ETAG)
                .unwrap()
                .to_str()
                .unwrap(),
            "\"abc\""
        );
    }

    #[test]
    fn test_response_new_custom_status() {
        let resp = HttpResponse::new(StatusCode::CREATED, "Created");
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.content_bytes().unwrap(), b"Created");
    }
}
