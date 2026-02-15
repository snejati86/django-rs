//! HTTP test client for django-rs.
//!
//! This module provides [`TestClient`] for making simulated HTTP requests
//! against an Axum application, and [`TestResponse`] for inspecting the results.
//! It mirrors Django's `django.test.Client`.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use django_rs_test::client::TestClient;
//! use axum::Router;
//! use axum::routing::get;
//!
//! async fn example() {
//!     let app = Router::new().route("/hello", get(|| async { "Hello, World!" }));
//!     let mut client = TestClient::new(app);
//!
//!     let response = client.get("/hello").await;
//!     assert_eq!(response.status_code(), 200);
//!     assert_eq!(response.text(), "Hello, World!");
//! }
//! ```

use std::collections::HashMap;

use axum::Router;
use bytes::Bytes;
use http::{HeaderMap, Method, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use django_rs_core::DjangoError;
use django_rs_views::SessionData;

/// A test client for making simulated HTTP requests against an Axum application.
///
/// Maintains cookies across requests and provides convenience methods for
/// common HTTP methods. All methods are async.
pub struct TestClient {
    app: Router,
    cookies: HashMap<String, String>,
    session: SessionData,
}

impl TestClient {
    /// Creates a new test client wrapping the given Axum router.
    pub fn new(app: Router) -> Self {
        Self {
            app,
            cookies: HashMap::new(),
            session: SessionData::new("test-session".to_string()),
        }
    }

    /// Sends a GET request to the given path.
    pub async fn get(&mut self, path: &str) -> TestResponse {
        self.request(Method::GET, path, None).await
    }

    /// Sends a POST request with form data.
    pub async fn post(&mut self, path: &str, data: &HashMap<String, String>) -> TestResponse {
        let body = Self::encode_form_data(data);
        self.request_with_body(
            Method::POST,
            path,
            body.into_bytes(),
            "application/x-www-form-urlencoded",
        )
        .await
    }

    /// Sends a PUT request with form data.
    pub async fn put(&mut self, path: &str, data: &HashMap<String, String>) -> TestResponse {
        let body = Self::encode_form_data(data);
        self.request_with_body(
            Method::PUT,
            path,
            body.into_bytes(),
            "application/x-www-form-urlencoded",
        )
        .await
    }

    /// Sends a PATCH request with form data.
    pub async fn patch(&mut self, path: &str, data: &HashMap<String, String>) -> TestResponse {
        let body = Self::encode_form_data(data);
        self.request_with_body(
            Method::PATCH,
            path,
            body.into_bytes(),
            "application/x-www-form-urlencoded",
        )
        .await
    }

    /// Sends a DELETE request to the given path.
    pub async fn delete(&mut self, path: &str) -> TestResponse {
        self.request(Method::DELETE, path, None).await
    }

    /// Sends a HEAD request to the given path.
    pub async fn head(&mut self, path: &str) -> TestResponse {
        self.request(Method::HEAD, path, None).await
    }

    /// Sends an OPTIONS request to the given path.
    pub async fn options(&mut self, path: &str) -> TestResponse {
        self.request(Method::OPTIONS, path, None).await
    }

    /// Sets a cookie that will be included in subsequent requests.
    pub fn set_cookie(&mut self, name: &str, value: &str) {
        self.cookies.insert(name.to_string(), value.to_string());
    }

    /// Clears all cookies from the client.
    pub fn clear_cookies(&mut self) {
        self.cookies.clear();
    }

    /// Returns a reference to the session data.
    pub const fn session(&self) -> &SessionData {
        &self.session
    }

    /// Returns a mutable reference to the session data.
    pub fn session_mut(&mut self) -> &mut SessionData {
        &mut self.session
    }

    /// Encodes form data as a URL-encoded string.
    fn encode_form_data(data: &HashMap<String, String>) -> String {
        data.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// Builds the Cookie header from the current cookie jar.
    fn cookie_header(&self) -> Option<String> {
        if self.cookies.is_empty() {
            return None;
        }

        Some(
            self.cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    /// Sends a request with no body or an optional content type header.
    async fn request(
        &mut self,
        method: Method,
        path: &str,
        content_type: Option<&str>,
    ) -> TestResponse {
        let mut builder = Request::builder()
            .method(method)
            .uri(path);

        if let Some(ct) = content_type {
            builder = builder.header("content-type", ct);
        }

        if let Some(cookie) = self.cookie_header() {
            builder = builder.header("cookie", cookie);
        }

        let req = builder
            .body(axum::body::Body::empty())
            .expect("request builder should not fail");

        self.send(req).await
    }

    /// Sends a request with a body and content type.
    async fn request_with_body(
        &mut self,
        method: Method,
        path: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> TestResponse {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", content_type);

        if let Some(cookie) = self.cookie_header() {
            builder = builder.header("cookie", cookie);
        }

        let req = builder
            .body(axum::body::Body::from(body))
            .expect("request builder should not fail");

        self.send(req).await
    }

    /// Sends the request through the Axum router and builds a `TestResponse`.
    async fn send(&mut self, req: Request<axum::body::Body>) -> TestResponse {
        let response = self
            .app
            .clone()
            .oneshot(req)
            .await
            .expect("router should not error");

        let status = response.status();
        let headers = response.headers().clone();

        // Extract Set-Cookie headers and update cookie jar
        let mut response_cookies = HashMap::new();
        for value in headers.get_all(http::header::SET_COOKIE) {
            if let Ok(cookie_str) = value.to_str() {
                // Parse simple "name=value" from Set-Cookie header
                if let Some(pair) = cookie_str.split(';').next() {
                    if let Some((name, val)) = pair.split_once('=') {
                        let name = name.trim().to_string();
                        let val = val.trim().to_string();
                        self.cookies.insert(name.clone(), val.clone());
                        response_cookies.insert(name, val);
                    }
                }
            }
        }

        // Collect body
        let body_bytes = response
            .into_body()
            .collect()
            .await.map_or_else(|_| Bytes::new(), http_body_util::Collected::to_bytes);

        TestResponse {
            status,
            headers,
            body: body_bytes.to_vec(),
            cookies: response_cookies,
        }
    }
}

/// The response from a test request.
///
/// Provides methods for inspecting the status code, headers, body, and cookies.
#[derive(Debug)]
pub struct TestResponse {
    /// The HTTP status code.
    pub status: StatusCode,
    /// The response headers.
    pub headers: HeaderMap,
    /// The response body as raw bytes.
    pub body: Vec<u8>,
    /// Cookies set by the response.
    pub cookies: HashMap<String, String>,
}

impl TestResponse {
    /// Returns the response body as a UTF-8 string.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// Deserializes the response body as JSON.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, DjangoError> {
        serde_json::from_slice(&self.body)
            .map_err(|e| DjangoError::SerializationError(e.to_string()))
    }

    /// Returns the numeric status code.
    pub fn status_code(&self) -> u16 {
        self.status.as_u16()
    }

    /// Returns the value of a header by name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
    }

    /// Returns `true` if the response has the specified header.
    pub fn has_header(&self, name: &str) -> bool {
        self.headers.contains_key(name)
    }

    /// Returns `true` if the response body contains the given text.
    pub fn contains(&self, text: &str) -> bool {
        self.text().contains(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::{delete, get, head, options, patch, post, put};

    fn test_app() -> Router {
        Router::new()
            .route("/hello", get(|| async { "Hello, World!" }))
            .route("/json", get(|| async {
                axum::Json(serde_json::json!({"key": "value"}))
            }))
            .route(
                "/echo",
                post(|body: String| async move { body }),
            )
            .route(
                "/put",
                put(|body: String| async move { body }),
            )
            .route(
                "/patch",
                patch(|body: String| async move { body }),
            )
            .route(
                "/delete",
                delete(|| async { "deleted" }),
            )
            .route(
                "/head",
                head(|| async { "" }),
            )
            .route(
                "/options",
                options(|| async { "GET, POST" }),
            )
            .route(
                "/cookie",
                get(|headers: http::HeaderMap| async move {
                    headers
                        .get("cookie")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("no cookies")
                        .to_string()
                }),
            )
            .route(
                "/set-cookie",
                get(|| async {
                    (
                        [(http::header::SET_COOKIE, "session=abc123; Path=/")],
                        "cookie set",
                    )
                }),
            )
            .route(
                "/status/201",
                get(|| async { (StatusCode::CREATED, "created") }),
            )
            .route(
                "/status/404",
                get(|| async { (StatusCode::NOT_FOUND, "not found") }),
            )
            .route(
                "/headers",
                get(|| async {
                    (
                        [("x-custom", "custom-value"), ("x-another", "another-value")],
                        "headers",
                    )
                }),
            )
    }

    // ── GET tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_simple() {
        let mut client = TestClient::new(test_app());
        let response = client.get("/hello").await;
        assert_eq!(response.status_code(), 200);
        assert_eq!(response.text(), "Hello, World!");
    }

    #[tokio::test]
    async fn test_get_json() {
        let mut client = TestClient::new(test_app());
        let response = client.get("/json").await;
        assert_eq!(response.status_code(), 200);

        let json: serde_json::Value = response.json().unwrap();
        assert_eq!(json["key"], "value");
    }

    #[tokio::test]
    async fn test_get_status_codes() {
        let mut client = TestClient::new(test_app());

        let response = client.get("/status/201").await;
        assert_eq!(response.status_code(), 201);

        let response = client.get("/status/404").await;
        assert_eq!(response.status_code(), 404);
    }

    // ── POST tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_post_form_data() {
        let mut client = TestClient::new(test_app());
        let mut data = HashMap::new();
        data.insert("name".to_string(), "test".to_string());
        data.insert("value".to_string(), "123".to_string());

        let response = client.post("/echo", &data).await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(text.contains("name=test"));
        assert!(text.contains("value=123"));
    }

    // ── PUT tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_put() {
        let mut client = TestClient::new(test_app());
        let mut data = HashMap::new();
        data.insert("field".to_string(), "updated".to_string());

        let response = client.put("/put", &data).await;
        assert_eq!(response.status_code(), 200);
        assert!(response.text().contains("field=updated"));
    }

    // ── PATCH tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_patch() {
        let mut client = TestClient::new(test_app());
        let mut data = HashMap::new();
        data.insert("partial".to_string(), "update".to_string());

        let response = client.patch("/patch", &data).await;
        assert_eq!(response.status_code(), 200);
        assert!(response.text().contains("partial=update"));
    }

    // ── DELETE tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete() {
        let mut client = TestClient::new(test_app());
        let response = client.delete("/delete").await;
        assert_eq!(response.status_code(), 200);
        assert_eq!(response.text(), "deleted");
    }

    // ── HEAD tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_head() {
        let mut client = TestClient::new(test_app());
        let response = client.head("/head").await;
        assert_eq!(response.status_code(), 200);
    }

    // ── OPTIONS tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_options() {
        let mut client = TestClient::new(test_app());
        let response = client.options("/options").await;
        assert_eq!(response.status_code(), 200);
    }

    // ── Cookie tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_set_cookie_manually() {
        let mut client = TestClient::new(test_app());
        client.set_cookie("csrftoken", "abc123");

        let response = client.get("/cookie").await;
        assert!(response.text().contains("csrftoken=abc123"));
    }

    #[tokio::test]
    async fn test_cookies_from_response() {
        let mut client = TestClient::new(test_app());
        let response = client.get("/set-cookie").await;
        assert_eq!(response.cookies.get("session"), Some(&"abc123".to_string()));

        // The cookie should be sent in subsequent requests
        let response = client.get("/cookie").await;
        assert!(response.text().contains("session=abc123"));
    }

    #[tokio::test]
    async fn test_clear_cookies() {
        let mut client = TestClient::new(test_app());
        client.set_cookie("token", "xyz");
        client.clear_cookies();

        let response = client.get("/cookie").await;
        assert_eq!(response.text(), "no cookies");
    }

    // ── Header tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_response_headers() {
        let mut client = TestClient::new(test_app());
        let response = client.get("/headers").await;

        assert!(response.has_header("x-custom"));
        assert_eq!(response.header("x-custom"), Some("custom-value"));
        assert_eq!(response.header("x-another"), Some("another-value"));
        assert!(!response.has_header("x-nonexistent"));
        assert!(response.header("x-nonexistent").is_none());
    }

    // ── TestResponse tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_response_contains() {
        let mut client = TestClient::new(test_app());
        let response = client.get("/hello").await;
        assert!(response.contains("Hello"));
        assert!(response.contains("World"));
        assert!(!response.contains("Goodbye"));
    }

    #[tokio::test]
    async fn test_response_json_parse_error() {
        let mut client = TestClient::new(test_app());
        let response = client.get("/hello").await;
        let result: Result<serde_json::Value, _> = response.json();
        assert!(result.is_err());
    }

    // ── Session tests ─────────────────────────────────────────────────

    #[test]
    fn test_client_session() {
        let mut client = TestClient::new(Router::new());
        assert_eq!(client.session().session_key, "test-session");

        client.session_mut().set("key", serde_json::json!("value"));
        assert_eq!(
            client.session().get("key"),
            Some(&serde_json::json!("value"))
        );
    }

    // ── 404 for non-existent routes ───────────────────────────────────

    #[tokio::test]
    async fn test_404_for_missing_route() {
        let mut client = TestClient::new(test_app());
        let response = client.get("/nonexistent").await;
        assert_eq!(response.status_code(), 404);
    }
}
