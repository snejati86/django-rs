//! Request factory for building [`HttpRequest`] objects in tests.
//!
//! [`RequestFactory`] builds HTTP requests directly, bypassing routing and
//! middleware. This is useful when testing individual view functions in isolation.
//!
//! ## Example
//!
//! ```rust,no_run
//! use django_rs_test::request_factory::RequestFactory;
//!
//! let factory = RequestFactory::new();
//! let request = factory.get("/articles/");
//! assert_eq!(request.method(), &http::Method::GET);
//! assert_eq!(request.path(), "/articles/");
//! ```

use std::collections::HashMap;

use django_rs_auth::user::AbstractUser;
use django_rs_http::HttpRequest;
use http::Method;

/// A factory for building [`HttpRequest`] objects without routing or middleware.
///
/// Mirrors Django's `RequestFactory`. Requests are constructed directly and can
/// have users, session data, and custom headers attached via META entries.
pub struct RequestFactory {
    /// Default headers applied to every request.
    default_headers: HashMap<String, String>,
    /// Default META entries applied to every request.
    default_meta: HashMap<String, String>,
}

impl Default for RequestFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestFactory {
    /// Creates a new request factory with default settings.
    pub fn new() -> Self {
        Self {
            default_headers: HashMap::new(),
            default_meta: HashMap::new(),
        }
    }

    /// Adds a default header that will be included in all requests.
    #[must_use]
    pub fn with_default_header(mut self, name: &str, value: &str) -> Self {
        self.default_headers
            .insert(name.to_string(), value.to_string());
        self
    }

    /// Adds a default META entry that will be included in all requests.
    #[must_use]
    pub fn with_default_meta(mut self, key: &str, value: &str) -> Self {
        self.default_meta
            .insert(key.to_string(), value.to_string());
        self
    }

    /// Builds a GET request to the given path.
    pub fn get(&self, path: &str) -> HttpRequest {
        self.build_request(Method::GET, path, None, None)
    }

    /// Builds a POST request with a form-encoded body.
    pub fn post(&self, path: &str, body: &HashMap<String, String>) -> HttpRequest {
        let encoded = encode_form_data(body);
        self.build_request(
            Method::POST,
            path,
            Some(encoded.into_bytes()),
            Some("application/x-www-form-urlencoded"),
        )
    }

    /// Builds a POST request with a JSON body.
    pub fn post_json(&self, path: &str, json: &serde_json::Value) -> HttpRequest {
        let body = serde_json::to_vec(json).unwrap_or_default();
        self.build_request(Method::POST, path, Some(body), Some("application/json"))
    }

    /// Builds a PUT request with a form-encoded body.
    pub fn put(&self, path: &str, body: &HashMap<String, String>) -> HttpRequest {
        let encoded = encode_form_data(body);
        self.build_request(
            Method::PUT,
            path,
            Some(encoded.into_bytes()),
            Some("application/x-www-form-urlencoded"),
        )
    }

    /// Builds a PATCH request with a form-encoded body.
    pub fn patch(&self, path: &str, body: &HashMap<String, String>) -> HttpRequest {
        let encoded = encode_form_data(body);
        self.build_request(
            Method::PATCH,
            path,
            Some(encoded.into_bytes()),
            Some("application/x-www-form-urlencoded"),
        )
    }

    /// Builds a DELETE request to the given path.
    pub fn delete(&self, path: &str) -> HttpRequest {
        self.build_request(Method::DELETE, path, None, None)
    }

    /// Builds a HEAD request to the given path.
    pub fn head(&self, path: &str) -> HttpRequest {
        self.build_request(Method::HEAD, path, None, None)
    }

    /// Builds an OPTIONS request to the given path.
    pub fn options(&self, path: &str) -> HttpRequest {
        self.build_request(Method::OPTIONS, path, None, None)
    }

    /// Attaches user information to the request via META entries.
    ///
    /// Sets `USER_USERNAME`, `USER_EMAIL`, `USER_IS_AUTHENTICATED`,
    /// `USER_IS_STAFF`, and `USER_IS_SUPERUSER` in the request META.
    pub fn with_user(request: &mut HttpRequest, user: &AbstractUser) {
        let meta = request.meta_mut();
        meta.insert("USER_USERNAME".to_string(), user.username.clone());
        meta.insert("USER_EMAIL".to_string(), user.email.clone());
        meta.insert("USER_IS_AUTHENTICATED".to_string(), "true".to_string());
        meta.insert(
            "USER_IS_STAFF".to_string(),
            user.is_staff.to_string(),
        );
        meta.insert(
            "USER_IS_SUPERUSER".to_string(),
            user.is_superuser.to_string(),
        );
    }

    /// Attaches session data to the request via META entries.
    ///
    /// Serializes the session data as JSON into `SESSION_DATA` and marks it as
    /// `SESSION_MODIFIED=false`, `SESSION_IS_NEW=true`.
    pub fn with_session(request: &mut HttpRequest, data: &HashMap<String, serde_json::Value>) {
        let session_json = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
        let meta = request.meta_mut();
        meta.insert("SESSION_KEY".to_string(), "test-session".to_string());
        meta.insert("SESSION_DATA".to_string(), session_json);
        meta.insert("SESSION_MODIFIED".to_string(), "false".to_string());
        meta.insert("SESSION_IS_NEW".to_string(), "true".to_string());
    }

    /// Builds an [`HttpRequest`] with the given method, path, optional body, and
    /// optional content type.
    fn build_request(
        &self,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> HttpRequest {
        let mut builder = HttpRequest::builder()
            .method(method)
            .path(path)
            .meta("SERVER_NAME", "testserver")
            .meta("HTTP_HOST", "testserver");

        // Apply default headers
        for (name, value) in &self.default_headers {
            builder = builder.header(name, value);
        }

        // Apply default META
        for (key, value) in &self.default_meta {
            builder = builder.meta(key, value);
        }

        if let Some(ct) = content_type {
            builder = builder.content_type(ct);
        }

        if let Some(body_bytes) = body {
            builder = builder.body(body_bytes);
        }

        builder.build()
    }
}

/// URL-encodes form data as `key=value&key=value`.
fn encode_form_data(data: &HashMap<String, String>) -> String {
    data.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_get() {
        let factory = RequestFactory::new();
        let req = factory.get("/articles/");
        assert_eq!(req.method(), &Method::GET);
        assert_eq!(req.path(), "/articles/");
    }

    #[test]
    fn test_factory_default() {
        let factory = RequestFactory::default();
        let req = factory.get("/");
        assert_eq!(req.path(), "/");
    }

    #[test]
    fn test_factory_post_form() {
        let factory = RequestFactory::new();
        let mut data = HashMap::new();
        data.insert("name".to_string(), "alice".to_string());

        let req = factory.post("/submit/", &data);
        assert_eq!(req.method(), &Method::POST);
        assert_eq!(
            req.content_type(),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(req.post().get("name"), Some("alice"));
    }

    #[test]
    fn test_factory_post_json() {
        let factory = RequestFactory::new();
        let json = serde_json::json!({"key": "value"});

        let req = factory.post_json("/api/", &json);
        assert_eq!(req.method(), &Method::POST);
        assert_eq!(req.content_type(), Some("application/json"));
        assert!(!req.body().is_empty());
    }

    #[test]
    fn test_factory_put() {
        let factory = RequestFactory::new();
        let mut data = HashMap::new();
        data.insert("field".to_string(), "updated".to_string());

        let req = factory.put("/update/", &data);
        assert_eq!(req.method(), &Method::PUT);
    }

    #[test]
    fn test_factory_patch() {
        let factory = RequestFactory::new();
        let mut data = HashMap::new();
        data.insert("partial".to_string(), "update".to_string());

        let req = factory.patch("/patch/", &data);
        assert_eq!(req.method(), &Method::PATCH);
    }

    #[test]
    fn test_factory_delete() {
        let factory = RequestFactory::new();
        let req = factory.delete("/items/1/");
        assert_eq!(req.method(), &Method::DELETE);
        assert_eq!(req.path(), "/items/1/");
    }

    #[test]
    fn test_factory_head() {
        let factory = RequestFactory::new();
        let req = factory.head("/check/");
        assert_eq!(req.method(), &Method::HEAD);
    }

    #[test]
    fn test_factory_options() {
        let factory = RequestFactory::new();
        let req = factory.options("/api/");
        assert_eq!(req.method(), &Method::OPTIONS);
    }

    #[test]
    fn test_factory_with_user() {
        let factory = RequestFactory::new();
        let mut req = factory.get("/profile/");

        let mut user = AbstractUser::new("testuser");
        user.email = "test@example.com".to_string();
        user.is_staff = true;
        user.is_superuser = false;

        RequestFactory::with_user(&mut req, &user);

        assert_eq!(req.meta().get("USER_USERNAME").unwrap(), "testuser");
        assert_eq!(req.meta().get("USER_EMAIL").unwrap(), "test@example.com");
        assert_eq!(req.meta().get("USER_IS_AUTHENTICATED").unwrap(), "true");
        assert_eq!(req.meta().get("USER_IS_STAFF").unwrap(), "true");
        assert_eq!(req.meta().get("USER_IS_SUPERUSER").unwrap(), "false");
    }

    #[test]
    fn test_factory_with_session() {
        let factory = RequestFactory::new();
        let mut req = factory.get("/dashboard/");

        let mut session_data = HashMap::new();
        session_data.insert("theme".to_string(), serde_json::json!("dark"));
        session_data.insert("language".to_string(), serde_json::json!("en"));

        RequestFactory::with_session(&mut req, &session_data);

        let meta = req.meta();
        assert_eq!(meta.get("SESSION_KEY").unwrap(), "test-session");
        assert!(meta.get("SESSION_DATA").unwrap().contains("dark"));
        assert_eq!(meta.get("SESSION_MODIFIED").unwrap(), "false");
        assert_eq!(meta.get("SESSION_IS_NEW").unwrap(), "true");
    }

    #[test]
    fn test_factory_default_headers() {
        let factory = RequestFactory::new()
            .with_default_header("accept", "application/json")
            .with_default_header("x-custom", "test-value");

        let req = factory.get("/api/");
        assert_eq!(
            req.headers()
                .get("accept")
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
        assert_eq!(
            req.headers()
                .get("x-custom")
                .and_then(|v| v.to_str().ok()),
            Some("test-value")
        );
    }

    #[test]
    fn test_factory_default_meta() {
        let factory = RequestFactory::new()
            .with_default_meta("REMOTE_ADDR", "192.168.1.1");

        let req = factory.get("/");
        assert_eq!(req.meta().get("REMOTE_ADDR").unwrap(), "192.168.1.1");
    }

    #[test]
    fn test_factory_server_name() {
        let factory = RequestFactory::new();
        let req = factory.get("/");
        assert_eq!(req.get_host(), "testserver");
    }
}
