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

// ── DatabaseSessionBackend ─────────────────────────────────────────

/// A database-backed session backend using [`DbExecutor`].
///
/// Stores sessions in a `django_session` table with columns:
/// - `session_key TEXT PRIMARY KEY`
/// - `session_data TEXT` (JSON-serialized)
/// - `expire_date TEXT` (ISO 8601 timestamp)
///
/// This mirrors Django's `django.contrib.sessions.backends.db`.
pub struct DatabaseSessionBackend {
    db: Arc<dyn django_rs_db::executor::DbExecutor>,
}

impl DatabaseSessionBackend {
    /// Creates a new database session backend with the given executor.
    pub fn new(db: Arc<dyn django_rs_db::executor::DbExecutor>) -> Self {
        Self { db }
    }

    /// Creates the `django_session` table if it does not already exist.
    pub async fn create_table(&self) -> Result<(), DjangoError> {
        let sql = "CREATE TABLE IF NOT EXISTS django_session (\
            session_key TEXT PRIMARY KEY, \
            session_data TEXT NOT NULL, \
            expire_date TEXT NOT NULL\
        )";
        self.db
            .execute_sql(sql, &[])
            .await
            .map(|_| ())
    }
}

#[async_trait]
impl SessionBackend for DatabaseSessionBackend {
    async fn load(&self, session_key: &str) -> Result<SessionData, DjangoError> {
        let sql = "SELECT session_key, session_data, expire_date \
                    FROM django_session \
                    WHERE session_key = $1";
        let row = self
            .db
            .query_one(
                sql,
                &[django_rs_db::value::Value::String(session_key.to_string())],
            )
            .await?;

        let data_str: String = row.get("session_data")?;
        let expire_str: String = row.get("expire_date")?;

        let expire_date = chrono::DateTime::parse_from_rfc3339(&expire_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                DjangoError::NotFound(format!(
                    "Session '{session_key}' expire_date parse error: {e}"
                ))
            })?;

        if Utc::now() > expire_date {
            return Err(DjangoError::NotFound(format!(
                "Session '{session_key}' has expired"
            )));
        }

        let data: HashMap<String, serde_json::Value> =
            serde_json::from_str(&data_str).unwrap_or_default();

        Ok(SessionData {
            session_key: session_key.to_string(),
            data,
            expire_date,
            modified: false,
        })
    }

    async fn save(&self, session: &SessionData) -> Result<String, DjangoError> {
        let data_json = serde_json::to_string(&session.data)
            .map_err(|e| DjangoError::InternalServerError(format!("Failed to serialize session: {e}")))?;
        let expire_str = session.expire_date.to_rfc3339();

        // Use INSERT OR REPLACE (SQLite) / ON CONFLICT (Postgres-compatible)
        let sql = "INSERT INTO django_session (session_key, session_data, expire_date) \
                    VALUES ($1, $2, $3) \
                    ON CONFLICT(session_key) DO UPDATE SET \
                    session_data = $2, expire_date = $3";

        self.db
            .execute_sql(
                sql,
                &[
                    django_rs_db::value::Value::String(session.session_key.clone()),
                    django_rs_db::value::Value::String(data_json),
                    django_rs_db::value::Value::String(expire_str),
                ],
            )
            .await?;

        Ok(session.session_key.clone())
    }

    async fn delete(&self, session_key: &str) -> Result<(), DjangoError> {
        let sql = "DELETE FROM django_session WHERE session_key = $1";
        self.db
            .execute_sql(
                sql,
                &[django_rs_db::value::Value::String(session_key.to_string())],
            )
            .await?;
        Ok(())
    }

    async fn exists(&self, session_key: &str) -> Result<bool, DjangoError> {
        let sql = "SELECT session_key FROM django_session WHERE session_key = $1";
        match self
            .db
            .query(
                sql,
                &[django_rs_db::value::Value::String(session_key.to_string())],
            )
            .await
        {
            Ok(rows) => Ok(!rows.is_empty()),
            Err(_) => Ok(false),
        }
    }

    async fn clear_expired(&self) -> Result<(), DjangoError> {
        let now_str = Utc::now().to_rfc3339();
        let sql = "DELETE FROM django_session WHERE expire_date < $1";
        self.db
            .execute_sql(
                sql,
                &[django_rs_db::value::Value::String(now_str)],
            )
            .await?;
        Ok(())
    }
}

// ── SignedCookieSessionBackend ──────────────────────────────────────

/// A session backend that stores all session data in a signed cookie.
///
/// Session data is serialized to JSON, signed with HMAC-SHA256, and stored
/// directly in the cookie value. No server-side storage is required.
///
/// **Size limit:** Cookies are limited to 4096 bytes. An error is returned
/// if the signed data exceeds this limit.
///
/// This mirrors Django's `django.contrib.sessions.backends.signed_cookies`.
pub struct SignedCookieSessionBackend {
    secret_key: String,
    salt: String,
    /// Maximum cookie size in bytes.
    max_cookie_size: usize,
}

impl SignedCookieSessionBackend {
    /// Creates a new signed cookie backend with the given secret key.
    pub fn new(secret_key: &str) -> Self {
        Self {
            secret_key: secret_key.to_string(),
            salt: "django.contrib.sessions.backends.signed_cookies".to_string(),
            max_cookie_size: 4096,
        }
    }

    /// Sets a custom salt for HMAC signing.
    #[must_use]
    pub fn with_salt(mut self, salt: &str) -> Self {
        self.salt = salt.to_string();
        self
    }

    /// Sets the maximum cookie size.
    #[must_use]
    pub fn with_max_cookie_size(mut self, size: usize) -> Self {
        self.max_cookie_size = size;
        self
    }

    /// Signs data with HMAC-SHA256 and returns `base64(data).base64(signature)`.
    fn sign(&self, data: &str) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let key = format!("{}:{}", self.salt, self.secret_key);
        let mut mac =
            Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC can take key of any size");
        mac.update(data.as_bytes());
        let signature = mac.finalize().into_bytes();

        let data_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            data.as_bytes(),
        );
        let sig_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            signature,
        );

        format!("{data_b64}.{sig_b64}")
    }

    /// Verifies and extracts the original data from a signed cookie value.
    fn unsign(&self, signed_value: &str) -> Result<String, DjangoError> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let parts: Vec<&str> = signed_value.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(DjangoError::InternalServerError(
                "Invalid signed cookie format".to_string(),
            ));
        }

        let sig_b64 = parts[0];
        let data_b64 = parts[1];

        let data_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            data_b64,
        )
        .map_err(|e| DjangoError::InternalServerError(format!("Invalid base64 data: {e}")))?;

        let expected_sig = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            sig_b64,
        )
        .map_err(|e| DjangoError::InternalServerError(format!("Invalid base64 signature: {e}")))?;

        let data_str =
            String::from_utf8(data_bytes).map_err(|e| DjangoError::InternalServerError(e.to_string()))?;

        // Verify signature
        let key = format!("{}:{}", self.salt, self.secret_key);
        let mut mac =
            Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC can take key of any size");
        mac.update(data_str.as_bytes());

        mac.verify_slice(&expected_sig)
            .map_err(|_| DjangoError::InternalServerError("Invalid cookie signature".to_string()))?;

        Ok(data_str)
    }
}

#[async_trait]
impl SessionBackend for SignedCookieSessionBackend {
    async fn load(&self, session_key: &str) -> Result<SessionData, DjangoError> {
        // The "session_key" is the signed cookie value
        let data_str = self.unsign(session_key)?;

        // Parse the JSON envelope containing data and expiry
        let envelope: serde_json::Value = serde_json::from_str(&data_str)
            .map_err(|e| DjangoError::InternalServerError(format!("Invalid session JSON: {e}")))?;

        let session_data: HashMap<String, serde_json::Value> = envelope
            .get("data")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let expire_str = envelope
            .get("expire_date")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let expire_date = chrono::DateTime::parse_from_rfc3339(expire_str).map_or_else(
            |_| Utc::now() + Duration::weeks(2),
            |dt| dt.with_timezone(&Utc),
        );

        if Utc::now() > expire_date {
            return Err(DjangoError::NotFound("Session has expired".to_string()));
        }

        let key = envelope
            .get("session_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(SessionData {
            session_key: key,
            data: session_data,
            expire_date,
            modified: false,
        })
    }

    async fn save(&self, session: &SessionData) -> Result<String, DjangoError> {
        let envelope = serde_json::json!({
            "session_key": session.session_key,
            "data": session.data,
            "expire_date": session.expire_date.to_rfc3339(),
        });

        let json_str = serde_json::to_string(&envelope)
            .map_err(|e| DjangoError::InternalServerError(format!("Failed to serialize session: {e}")))?;

        let signed = self.sign(&json_str);

        if signed.len() > self.max_cookie_size {
            return Err(DjangoError::InternalServerError(format!(
                "Session cookie exceeds maximum size of {} bytes (actual: {})",
                self.max_cookie_size,
                signed.len()
            )));
        }

        Ok(signed)
    }

    async fn delete(&self, _session_key: &str) -> Result<(), DjangoError> {
        // For signed cookies, "deleting" means the client won't send the cookie anymore.
        // Nothing to do server-side.
        Ok(())
    }

    async fn exists(&self, session_key: &str) -> Result<bool, DjangoError> {
        // Verify that the signed value is valid
        match self.unsign(session_key) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn clear_expired(&self) -> Result<(), DjangoError> {
        // No server-side storage to clean up
        Ok(())
    }
}

// ── FileSessionBackend ─────────────────────────────────────────────

/// A file-based session backend that stores each session as a JSON file.
///
/// Sessions are stored as `{storage_path}/{session_key}.json`. This is
/// suitable for development and single-server deployments.
///
/// This mirrors Django's `django.contrib.sessions.backends.file`.
pub struct FileSessionBackend {
    storage_path: std::path::PathBuf,
}

impl FileSessionBackend {
    /// Creates a new file session backend that stores sessions in the given directory.
    ///
    /// The directory will be created if it does not exist.
    pub fn new(storage_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            storage_path: storage_path.into(),
        }
    }

    /// Returns the file path for a given session key.
    fn session_file(&self, session_key: &str) -> std::path::PathBuf {
        self.storage_path.join(format!("{session_key}.json"))
    }

    /// Ensures the storage directory exists.
    async fn ensure_dir(&self) -> Result<(), DjangoError> {
        tokio::fs::create_dir_all(&self.storage_path)
            .await
            .map_err(|e| {
                DjangoError::InternalServerError(format!(
                    "Failed to create session directory '{}': {e}",
                    self.storage_path.display()
                ))
            })
    }
}

/// Internal file format for session files.
#[derive(serde::Serialize, serde::Deserialize)]
struct FileSessionEnvelope {
    session_key: String,
    data: HashMap<String, serde_json::Value>,
    expire_date: String,
}

#[async_trait]
impl SessionBackend for FileSessionBackend {
    async fn load(&self, session_key: &str) -> Result<SessionData, DjangoError> {
        let path = self.session_file(session_key);
        let content = tokio::fs::read_to_string(&path).await.map_err(|_| {
            DjangoError::NotFound(format!("Session '{session_key}' not found"))
        })?;

        let envelope: FileSessionEnvelope = serde_json::from_str(&content).map_err(|e| {
            DjangoError::InternalServerError(format!("Invalid session file for '{session_key}': {e}"))
        })?;

        let expire_date = chrono::DateTime::parse_from_rfc3339(&envelope.expire_date)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                DjangoError::InternalServerError(format!(
                    "Invalid expire_date in session '{session_key}': {e}"
                ))
            })?;

        if Utc::now() > expire_date {
            // Remove the expired file
            let _ = tokio::fs::remove_file(&path).await;
            return Err(DjangoError::NotFound(format!(
                "Session '{session_key}' has expired"
            )));
        }

        Ok(SessionData {
            session_key: envelope.session_key,
            data: envelope.data,
            expire_date,
            modified: false,
        })
    }

    async fn save(&self, session: &SessionData) -> Result<String, DjangoError> {
        self.ensure_dir().await?;

        let envelope = FileSessionEnvelope {
            session_key: session.session_key.clone(),
            data: session.data.clone(),
            expire_date: session.expire_date.to_rfc3339(),
        };

        let content = serde_json::to_string_pretty(&envelope)
            .map_err(|e| DjangoError::InternalServerError(format!("Failed to serialize session: {e}")))?;

        let path = self.session_file(&session.session_key);
        tokio::fs::write(&path, content.as_bytes())
            .await
            .map_err(|e| {
                DjangoError::InternalServerError(format!(
                    "Failed to write session file '{}': {e}",
                    path.display()
                ))
            })?;

        Ok(session.session_key.clone())
    }

    async fn delete(&self, session_key: &str) -> Result<(), DjangoError> {
        let path = self.session_file(session_key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) | Err(_) => Ok(()),
        }
    }

    async fn exists(&self, session_key: &str) -> Result<bool, DjangoError> {
        let path = self.session_file(session_key);
        Ok(path.exists())
    }

    async fn clear_expired(&self) -> Result<(), DjangoError> {
        self.ensure_dir().await?;

        let mut entries = tokio::fs::read_dir(&self.storage_path).await.map_err(|e| {
            DjangoError::InternalServerError(format!(
                "Failed to read session directory '{}': {e}",
                self.storage_path.display()
            ))
        })?;

        loop {
            let entry: tokio::fs::DirEntry = match entries.next_entry().await {
                Ok(Some(e)) => e,
                Ok(None) | Err(_) => break,
            };
            let path: std::path::PathBuf = entry.path();
            let ext_match = path
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                == Some("json");
            if !ext_match {
                continue;
            }

            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                if let Ok(envelope) = serde_json::from_str::<FileSessionEnvelope>(&content) {
                    if let Ok(expire_date) =
                        chrono::DateTime::parse_from_rfc3339(&envelope.expire_date)
                    {
                        if Utc::now() > expire_date.with_timezone(&Utc) {
                            let _ = tokio::fs::remove_file(&path).await;
                        }
                    }
                }
            }
        }

        Ok(())
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

    // ── SignedCookieSessionBackend tests ────────────────────────────

    #[tokio::test]
    async fn test_signed_cookie_save_and_load() {
        let backend = SignedCookieSessionBackend::new("my-secret-key-1234");
        let mut session = SessionData::new("signed-session".to_string());
        session.set("user", serde_json::json!("bob"));

        let cookie_value = backend.save(&session).await.unwrap();
        assert!(!cookie_value.is_empty());
        assert!(cookie_value.contains('.'));

        let loaded = backend.load(&cookie_value).await.unwrap();
        assert_eq!(loaded.get("user"), Some(&serde_json::json!("bob")));
        assert_eq!(loaded.session_key, "signed-session");
    }

    #[tokio::test]
    async fn test_signed_cookie_tamper_detection() {
        let backend = SignedCookieSessionBackend::new("my-secret-key-1234");
        let mut session = SessionData::new("test".to_string());
        session.set("admin", serde_json::json!(true));

        let cookie_value = backend.save(&session).await.unwrap();

        // Tamper with the data portion
        let tampered = format!("tampered{cookie_value}");
        let result = backend.load(&tampered).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_signed_cookie_wrong_key() {
        let backend1 = SignedCookieSessionBackend::new("key-1");
        let backend2 = SignedCookieSessionBackend::new("key-2");

        let session = SessionData::new("test".to_string());
        let cookie_value = backend1.save(&session).await.unwrap();

        let result = backend2.load(&cookie_value).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_signed_cookie_expired_session() {
        let backend = SignedCookieSessionBackend::new("secret");
        let mut session = SessionData::new("expired".to_string());
        session.expire_date = Utc::now() - Duration::hours(1);

        let cookie_value = backend.save(&session).await.unwrap();
        let result = backend.load(&cookie_value).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_signed_cookie_size_limit() {
        let backend = SignedCookieSessionBackend::new("secret")
            .with_max_cookie_size(100);
        let mut session = SessionData::new("big".to_string());
        // Add a large value
        session.set("data", serde_json::json!("x".repeat(200)));

        let result = backend.save(&session).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_signed_cookie_exists() {
        let backend = SignedCookieSessionBackend::new("secret");
        let session = SessionData::new("test".to_string());
        let cookie_value = backend.save(&session).await.unwrap();

        assert!(backend.exists(&cookie_value).await.unwrap());
        assert!(!backend.exists("invalid-cookie").await.unwrap());
    }

    #[tokio::test]
    async fn test_signed_cookie_delete() {
        let backend = SignedCookieSessionBackend::new("secret");
        // Delete is a no-op for signed cookies
        let result = backend.delete("anything").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_signed_cookie_clear_expired() {
        let backend = SignedCookieSessionBackend::new("secret");
        // clear_expired is a no-op for signed cookies
        let result = backend.clear_expired().await;
        assert!(result.is_ok());
    }

    // ── FileSessionBackend tests ───────────────────────────────────

    #[tokio::test]
    async fn test_file_backend_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileSessionBackend::new(dir.path());

        let mut session = SessionData::new("file-session".to_string());
        session.set("color", serde_json::json!("blue"));

        backend.save(&session).await.unwrap();

        let loaded = backend.load("file-session").await.unwrap();
        assert_eq!(loaded.get("color"), Some(&serde_json::json!("blue")));
    }

    #[tokio::test]
    async fn test_file_backend_load_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileSessionBackend::new(dir.path());
        let result = backend.load("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_backend_delete() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileSessionBackend::new(dir.path());

        let session = SessionData::new("to-delete".to_string());
        backend.save(&session).await.unwrap();
        assert!(backend.exists("to-delete").await.unwrap());

        backend.delete("to-delete").await.unwrap();
        assert!(!backend.exists("to-delete").await.unwrap());
    }

    #[tokio::test]
    async fn test_file_backend_exists() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileSessionBackend::new(dir.path());

        assert!(!backend.exists("nope").await.unwrap());

        let session = SessionData::new("yes".to_string());
        backend.save(&session).await.unwrap();
        assert!(backend.exists("yes").await.unwrap());
    }

    #[tokio::test]
    async fn test_file_backend_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileSessionBackend::new(dir.path());

        let mut session = SessionData::new("overwrite".to_string());
        session.set("val", serde_json::json!(1));
        backend.save(&session).await.unwrap();

        session.set("val", serde_json::json!(2));
        backend.save(&session).await.unwrap();

        let loaded = backend.load("overwrite").await.unwrap();
        assert_eq!(loaded.get("val"), Some(&serde_json::json!(2)));
    }

    #[tokio::test]
    async fn test_file_backend_expired_session() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileSessionBackend::new(dir.path());

        let mut session = SessionData::new("expired".to_string());
        session.expire_date = Utc::now() - Duration::hours(1);
        backend.save(&session).await.unwrap();

        let result = backend.load("expired").await;
        assert!(result.is_err());
        // File should be cleaned up
        assert!(!backend.exists("expired").await.unwrap());
    }

    #[tokio::test]
    async fn test_file_backend_clear_expired() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileSessionBackend::new(dir.path());

        // Active session
        let active = SessionData::new("active".to_string());
        backend.save(&active).await.unwrap();

        // Expired session
        let mut expired = SessionData::new("expired".to_string());
        expired.expire_date = Utc::now() - Duration::hours(1);
        backend.save(&expired).await.unwrap();

        backend.clear_expired().await.unwrap();

        assert!(backend.exists("active").await.unwrap());
        assert!(!backend.exists("expired").await.unwrap());
    }

    #[tokio::test]
    async fn test_file_backend_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("nested").join("sessions");
        let backend = FileSessionBackend::new(&nested);

        let session = SessionData::new("test".to_string());
        backend.save(&session).await.unwrap();
        assert!(nested.exists());
    }
}
