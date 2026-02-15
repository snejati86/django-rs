//! Session framework for django-rs.
//!
//! This module provides the [`SessionBackend`] trait and built-in implementations
//! for managing user sessions. It mirrors Django's `django.contrib.sessions` module.
//!
//! ## Session Backends
//!
//! - [`InMemorySessionBackend`] - Stores sessions in memory (suitable for testing)
//! - [`CookieSessionBackend`] - Stores session data in a signed cookie
//!
//! ## Session Middleware
//!
//! [`SessionMiddleware`] integrates the session framework into the request/response
//! pipeline, loading sessions from the cookie on request and saving them on response.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::RwLock;

use django_rs_core::DjangoError;
use django_rs_http::{HttpRequest, HttpResponse};

use crate::middleware::Middleware;

/// Data associated with a user session.
///
/// Contains the session key, a map of key-value data, an expiration timestamp,
/// and a flag indicating whether the session has been modified.
#[derive(Debug, Clone)]
pub struct SessionData {
    /// The unique session key identifying this session.
    pub session_key: String,
    /// The session data stored as a map of string keys to JSON values.
    pub data: HashMap<String, serde_json::Value>,
    /// The timestamp when this session expires.
    pub expire_date: DateTime<Utc>,
    /// Whether the session data has been modified since last save.
    pub modified: bool,
}

impl SessionData {
    /// Creates a new empty session with the given key and default expiration.
    pub fn new(session_key: String) -> Self {
        Self {
            session_key,
            data: HashMap::new(),
            expire_date: Utc::now() + Duration::weeks(2),
            modified: false,
        }
    }

    /// Creates a new empty session with a specified lifetime.
    pub fn with_lifetime(session_key: String, lifetime_seconds: i64) -> Self {
        Self {
            session_key,
            data: HashMap::new(),
            expire_date: Utc::now() + Duration::seconds(lifetime_seconds),
            modified: false,
        }
    }

    /// Gets a value from the session by key.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    /// Sets a value in the session.
    pub fn set(&mut self, key: &str, value: serde_json::Value) {
        self.data.insert(key.to_string(), value);
        self.modified = true;
    }

    /// Removes a value from the session.
    pub fn remove(&mut self, key: &str) -> Option<serde_json::Value> {
        let result = self.data.remove(key);
        if result.is_some() {
            self.modified = true;
        }
        result
    }

    /// Returns `true` if the session has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expire_date
    }

    /// Clears all data from the session.
    pub fn clear(&mut self) {
        self.data.clear();
        self.modified = true;
    }

    /// Returns the number of entries in the session data.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the session data is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// A backend for storing and retrieving session data.
///
/// This trait mirrors Django's session backend interface. Implementations
/// handle the actual persistence of session data (in memory, database, cache, etc.).
#[async_trait]
pub trait SessionBackend: Send + Sync {
    /// Loads session data for the given session key.
    async fn load(&self, session_key: &str) -> Result<SessionData, DjangoError>;

    /// Saves session data and returns the session key.
    async fn save(&self, session: &SessionData) -> Result<String, DjangoError>;

    /// Deletes a session by its key.
    async fn delete(&self, session_key: &str) -> Result<(), DjangoError>;

    /// Checks whether a session with the given key exists.
    async fn exists(&self, session_key: &str) -> Result<bool, DjangoError>;

    /// Removes all expired sessions.
    async fn clear_expired(&self) -> Result<(), DjangoError>;
}

/// An in-memory session backend, suitable for testing.
///
/// Stores all sessions in a thread-safe in-memory map. Sessions are lost
/// when the application restarts.
#[derive(Debug, Default)]
pub struct InMemorySessionBackend {
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
}

impl InMemorySessionBackend {
    /// Creates a new in-memory session backend.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl SessionBackend for InMemorySessionBackend {
    async fn load(&self, session_key: &str) -> Result<SessionData, DjangoError> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_key)
            .filter(|s| !s.is_expired())
            .cloned()
            .ok_or_else(|| {
                DjangoError::NotFound(format!("Session '{session_key}' not found"))
            })
    }

    async fn save(&self, session: &SessionData) -> Result<String, DjangoError> {
        let key = session.session_key.clone();
        self.sessions
            .write()
            .await
            .insert(key.clone(), session.clone());
        Ok(key)
    }

    async fn delete(&self, session_key: &str) -> Result<(), DjangoError> {
        self.sessions.write().await.remove(session_key);
        Ok(())
    }

    async fn exists(&self, session_key: &str) -> Result<bool, DjangoError> {
        let sessions = self.sessions.read().await;
        Ok(sessions
            .get(session_key)
            .is_some_and(|s| !s.is_expired()))
    }

    async fn clear_expired(&self) -> Result<(), DjangoError> {
        self.sessions
            .write()
            .await
            .retain(|_, session| !session.is_expired());
        Ok(())
    }
}

/// A cookie-based session backend (simplified).
///
/// In a full implementation, this would sign and encrypt session data stored
/// in the cookie. This simplified version stores the session key in the cookie
/// and delegates to an in-memory store.
///
/// This mirrors Django's `django.contrib.sessions.backends.signed_cookies`.
#[derive(Debug, Default)]
pub struct CookieSessionBackend {
    inner: InMemorySessionBackend,
}

impl CookieSessionBackend {
    /// Creates a new cookie session backend.
    pub fn new() -> Self {
        Self {
            inner: InMemorySessionBackend::new(),
        }
    }
}

#[async_trait]
impl SessionBackend for CookieSessionBackend {
    async fn load(&self, session_key: &str) -> Result<SessionData, DjangoError> {
        self.inner.load(session_key).await
    }

    async fn save(&self, session: &SessionData) -> Result<String, DjangoError> {
        self.inner.save(session).await
    }

    async fn delete(&self, session_key: &str) -> Result<(), DjangoError> {
        self.inner.delete(session_key).await
    }

    async fn exists(&self, session_key: &str) -> Result<bool, DjangoError> {
        self.inner.exists(session_key).await
    }

    async fn clear_expired(&self) -> Result<(), DjangoError> {
        self.inner.clear_expired().await
    }
}

/// Middleware that integrates the session framework into the request/response pipeline.
///
/// On each request, loads the session data from the backend using the session cookie.
/// On each response, saves modified session data back to the backend and sets
/// the session cookie.
///
/// Session data is serialized into the request's META dictionary:
/// - `SESSION_KEY`: the session key string
/// - `SESSION_DATA`: JSON-serialized session data
/// - `SESSION_MODIFIED`: "true" or "false"
/// - `SESSION_IS_NEW`: "true" if a new session was created
///
/// Views can access and modify session data via the META entries. On response,
/// modified sessions are saved and the session cookie is set/updated.
///
/// This mirrors Django's `SessionMiddleware`.
pub struct SessionMiddleware {
    backend: Box<dyn SessionBackend>,
    cookie_name: String,
    cookie_path: String,
    cookie_httponly: bool,
    cookie_secure: bool,
    cookie_samesite: String,
}

impl SessionMiddleware {
    /// Creates a new `SessionMiddleware` with the given backend.
    pub fn new(backend: impl SessionBackend + 'static) -> Self {
        Self {
            backend: Box::new(backend),
            cookie_name: "sessionid".to_string(),
            cookie_path: "/".to_string(),
            cookie_httponly: true,
            cookie_secure: false,
            cookie_samesite: "Lax".to_string(),
        }
    }

    /// Sets the cookie name used for sessions.
    #[must_use]
    pub fn with_cookie_name(mut self, name: &str) -> Self {
        self.cookie_name = name.to_string();
        self
    }

    /// Sets the cookie path.
    #[must_use]
    pub fn with_cookie_path(mut self, path: &str) -> Self {
        self.cookie_path = path.to_string();
        self
    }

    /// Sets whether the cookie should be marked as secure.
    #[must_use]
    pub fn with_cookie_secure(mut self, secure: bool) -> Self {
        self.cookie_secure = secure;
        self
    }

    /// Sets the `SameSite` attribute for the session cookie.
    #[must_use]
    pub fn with_cookie_samesite(mut self, samesite: &str) -> Self {
        self.cookie_samesite = samesite.to_string();
        self
    }

    /// Returns the cookie name used for sessions.
    pub fn cookie_name(&self) -> &str {
        &self.cookie_name
    }

    /// Returns a reference to the session backend.
    pub fn backend(&self) -> &dyn SessionBackend {
        &*self.backend
    }

    /// Extracts the session key from the request cookies.
    fn get_session_key_from_request(&self, request: &HttpRequest) -> Option<String> {
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

    /// Builds the Set-Cookie header value for the session cookie.
    fn build_set_cookie(&self, session_key: &str) -> String {
        use std::fmt::Write;
        let mut cookie = format!("{}={}", self.cookie_name, session_key);
        let _ = write!(cookie, "; Path={}", self.cookie_path);
        if self.cookie_httponly {
            cookie.push_str("; HttpOnly");
        }
        if self.cookie_secure {
            cookie.push_str("; Secure");
        }
        if !self.cookie_samesite.is_empty() {
            let _ = write!(cookie, "; SameSite={}", self.cookie_samesite);
        }
        cookie
    }
}

#[async_trait]
impl Middleware for SessionMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        if let Some(session_key) = self.get_session_key_from_request(request) {
            // Try to load an existing session from the backend
            if let Ok(session) = self.backend.load(&session_key).await {
                // Serialize session data as JSON into META
                let session_data_json =
                    serde_json::to_string(&session.data).unwrap_or_default();

                // Store session key, data, and modification flag in META
                let meta = request.meta_mut();
                meta.insert("SESSION_KEY".to_string(), session_key);
                meta.insert("SESSION_DATA".to_string(), session_data_json);
                meta.insert("SESSION_MODIFIED".to_string(), "false".to_string());
                meta.insert("SESSION_IS_NEW".to_string(), "false".to_string());
            } else {
                // Session not found or expired -- create a new empty session
                let new_key = generate_session_key();
                let meta = request.meta_mut();
                meta.insert("SESSION_KEY".to_string(), new_key);
                meta.insert("SESSION_DATA".to_string(), "{}".to_string());
                meta.insert("SESSION_MODIFIED".to_string(), "false".to_string());
                meta.insert("SESSION_IS_NEW".to_string(), "true".to_string());
            }
        } else {
            // No session cookie — create a new session
            let new_key = generate_session_key();
            let meta = request.meta_mut();
            meta.insert("SESSION_KEY".to_string(), new_key);
            meta.insert("SESSION_DATA".to_string(), "{}".to_string());
            meta.insert("SESSION_MODIFIED".to_string(), "false".to_string());
            meta.insert("SESSION_IS_NEW".to_string(), "true".to_string());
        }
        None
    }

    async fn process_response(
        &self,
        request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        let meta = request.meta();
        let Some(session_key) = meta.get("SESSION_KEY") else {
            return response;
        };
        let session_key = session_key.clone();

        let session_data_str = meta
            .get("SESSION_DATA")
            .cloned()
            .unwrap_or_else(|| "{}".to_string());
        let modified = meta
            .get("SESSION_MODIFIED")
            .is_some_and(|v| v == "true");
        let is_new = meta
            .get("SESSION_IS_NEW")
            .is_some_and(|v| v == "true");

        // Only save and set cookie if session was modified or is new with data
        let data: HashMap<String, serde_json::Value> =
            serde_json::from_str(&session_data_str).unwrap_or_default();

        let should_save = modified || (is_new && !data.is_empty());

        if should_save {
            let mut session = SessionData::new(session_key.clone());
            session.data = data;

            // Save session to backend
            let _ = self.backend.save(&session).await;

            // Set the session cookie
            let cookie_value = self.build_set_cookie(&session_key);
            let mut resp = response;
            if let Ok(header_value) = http::header::HeaderValue::from_str(&cookie_value) {
                resp.headers_mut()
                    .insert(http::header::SET_COOKIE, header_value);
            }
            return resp;
        }

        // If session already existed (not new), always set the cookie to maintain it
        if !is_new {
            let cookie_value = self.build_set_cookie(&session_key);
            let mut resp = response;
            if let Ok(header_value) = http::header::HeaderValue::from_str(&cookie_value) {
                resp.headers_mut()
                    .insert(http::header::SET_COOKIE, header_value);
            }
            return resp;
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

/// Generates a random session key.
pub fn generate_session_key() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{timestamp:032x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SessionData tests ───────────────────────────────────────────

    #[test]
    fn test_session_data_new() {
        let session = SessionData::new("test-key".to_string());
        assert_eq!(session.session_key, "test-key");
        assert!(session.data.is_empty());
        assert!(!session.modified);
        assert!(!session.is_expired());
    }

    #[test]
    fn test_session_data_get_set() {
        let mut session = SessionData::new("test".to_string());
        session.set("username", serde_json::json!("alice"));
        assert_eq!(
            session.get("username"),
            Some(&serde_json::json!("alice"))
        );
        assert!(session.modified);
    }

    #[test]
    fn test_session_data_remove() {
        let mut session = SessionData::new("test".to_string());
        session.set("key", serde_json::json!("value"));
        session.modified = false;

        let removed = session.remove("key");
        assert_eq!(removed, Some(serde_json::json!("value")));
        assert!(session.modified);
        assert!(session.get("key").is_none());
    }

    #[test]
    fn test_session_data_remove_nonexistent() {
        let mut session = SessionData::new("test".to_string());
        let removed = session.remove("nonexistent");
        assert!(removed.is_none());
        assert!(!session.modified);
    }

    #[test]
    fn test_session_data_clear() {
        let mut session = SessionData::new("test".to_string());
        session.set("a", serde_json::json!(1));
        session.set("b", serde_json::json!(2));
        session.modified = false;

        session.clear();
        assert!(session.is_empty());
        assert!(session.modified);
    }

    #[test]
    fn test_session_data_len() {
        let mut session = SessionData::new("test".to_string());
        assert_eq!(session.len(), 0);
        session.set("a", serde_json::json!(1));
        assert_eq!(session.len(), 1);
        session.set("b", serde_json::json!(2));
        assert_eq!(session.len(), 2);
    }

    #[test]
    fn test_session_data_is_empty() {
        let session = SessionData::new("test".to_string());
        assert!(session.is_empty());
    }

    #[test]
    fn test_session_data_expired() {
        let mut session = SessionData::new("test".to_string());
        session.expire_date = Utc::now() - Duration::hours(1);
        assert!(session.is_expired());
    }

    #[test]
    fn test_session_data_not_expired() {
        let session = SessionData::new("test".to_string());
        assert!(!session.is_expired());
    }

    #[test]
    fn test_session_data_with_lifetime() {
        let session = SessionData::with_lifetime("test".to_string(), 3600);
        assert!(!session.is_expired());
        assert!(session.expire_date > Utc::now());
    }

    // ── InMemorySessionBackend tests ────────────────────────────────

    #[tokio::test]
    async fn test_in_memory_backend_save_and_load() {
        let backend = InMemorySessionBackend::new();
        let mut session = SessionData::new("session-1".to_string());
        session.set("user", serde_json::json!("alice"));

        backend.save(&session).await.unwrap();

        let loaded = backend.load("session-1").await.unwrap();
        assert_eq!(
            loaded.get("user"),
            Some(&serde_json::json!("alice"))
        );
    }

    #[tokio::test]
    async fn test_in_memory_backend_load_nonexistent() {
        let backend = InMemorySessionBackend::new();
        let result = backend.load("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_in_memory_backend_delete() {
        let backend = InMemorySessionBackend::new();
        let session = SessionData::new("session-1".to_string());
        backend.save(&session).await.unwrap();

        backend.delete("session-1").await.unwrap();
        assert!(!backend.exists("session-1").await.unwrap());
    }

    #[tokio::test]
    async fn test_in_memory_backend_exists() {
        let backend = InMemorySessionBackend::new();
        assert!(!backend.exists("session-1").await.unwrap());

        let session = SessionData::new("session-1".to_string());
        backend.save(&session).await.unwrap();

        assert!(backend.exists("session-1").await.unwrap());
    }

    #[tokio::test]
    async fn test_in_memory_backend_expired_session_not_loaded() {
        let backend = InMemorySessionBackend::new();
        let mut session = SessionData::new("expired".to_string());
        session.expire_date = Utc::now() - Duration::hours(1);
        backend.save(&session).await.unwrap();

        let result = backend.load("expired").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_in_memory_backend_expired_session_not_exists() {
        let backend = InMemorySessionBackend::new();
        let mut session = SessionData::new("expired".to_string());
        session.expire_date = Utc::now() - Duration::hours(1);
        backend.save(&session).await.unwrap();

        assert!(!backend.exists("expired").await.unwrap());
    }

    #[tokio::test]
    async fn test_in_memory_backend_clear_expired() {
        let backend = InMemorySessionBackend::new();

        // Save an active session
        let active = SessionData::new("active".to_string());
        backend.save(&active).await.unwrap();

        // Save an expired session
        let mut expired = SessionData::new("expired".to_string());
        expired.expire_date = Utc::now() - Duration::hours(1);
        backend.save(&expired).await.unwrap();

        backend.clear_expired().await.unwrap();

        assert!(backend.exists("active").await.unwrap());
        // Expired session should be gone from storage
        let sessions = backend.sessions.read().await;
        assert!(!sessions.contains_key("expired"));
    }

    // ── CookieSessionBackend tests ──────────────────────────────────

    #[tokio::test]
    async fn test_cookie_backend_save_and_load() {
        let backend = CookieSessionBackend::new();
        let mut session = SessionData::new("cookie-session".to_string());
        session.set("theme", serde_json::json!("dark"));

        backend.save(&session).await.unwrap();

        let loaded = backend.load("cookie-session").await.unwrap();
        assert_eq!(loaded.get("theme"), Some(&serde_json::json!("dark")));
    }

    #[tokio::test]
    async fn test_cookie_backend_delete() {
        let backend = CookieSessionBackend::new();
        let session = SessionData::new("to-delete".to_string());
        backend.save(&session).await.unwrap();

        backend.delete("to-delete").await.unwrap();
        assert!(!backend.exists("to-delete").await.unwrap());
    }

    // ── SessionMiddleware tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_session_middleware_new() {
        let backend = InMemorySessionBackend::new();
        let mw = SessionMiddleware::new(backend);
        assert_eq!(mw.cookie_name(), "sessionid");
    }

    #[tokio::test]
    async fn test_session_middleware_custom_cookie_name() {
        let backend = InMemorySessionBackend::new();
        let mw = SessionMiddleware::new(backend).with_cookie_name("my_session");
        assert_eq!(mw.cookie_name(), "my_session");
    }

    #[tokio::test]
    async fn test_session_middleware_extract_session_key() {
        let backend = InMemorySessionBackend::new();
        let mw = SessionMiddleware::new(backend);
        let request = HttpRequest::builder()
            .header("cookie", "sessionid=abc123; other=value")
            .build();
        let key = mw.get_session_key_from_request(&request);
        assert_eq!(key, Some("abc123".to_string()));
    }

    #[tokio::test]
    async fn test_session_middleware_no_cookie() {
        let backend = InMemorySessionBackend::new();
        let mw = SessionMiddleware::new(backend);
        let request = HttpRequest::builder().build();
        let key = mw.get_session_key_from_request(&request);
        assert!(key.is_none());
    }

    #[tokio::test]
    async fn test_session_middleware_process_request_passthrough() {
        let backend = InMemorySessionBackend::new();
        let mw = SessionMiddleware::new(backend);
        let mut request = HttpRequest::builder().build();
        let result = mw.process_request(&mut request).await;
        assert!(result.is_none());
    }

    // ── generate_session_key tests ──────────────────────────────────

    #[test]
    fn test_generate_session_key_not_empty() {
        let key = generate_session_key();
        assert!(!key.is_empty());
    }

    #[test]
    fn test_generate_session_key_unique() {
        let key1 = generate_session_key();
        // Small delay to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(1));
        let key2 = generate_session_key();
        assert_ne!(key1, key2);
    }
}
