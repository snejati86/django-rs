//! Authentication backends for django-rs.
//!
//! This module provides the [`AuthBackend`] trait and built-in implementations
//! for authenticating users against different sources. It also provides
//! convenience functions for login/logout session management.
//!
//! ## Built-in Backends
//!
//! - [`ModelBackend`] - Authenticates against the User model in the database
//! - [`RemoteUserBackend`] - Trusts the `REMOTE_USER` header (for proxy auth)
//!
//! ## Authentication Flow
//!
//! 1. Call [`authenticate`] with credentials and a list of backends
//! 2. Each backend is tried in order until one returns a user
//! 3. On success, call [`login`] to establish a session
//! 4. Call [`logout`] to destroy the session

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use django_rs_core::error::DjangoError;
use django_rs_views::session::SessionData;
use tokio::sync::RwLock;

use crate::user::AbstractUser;

/// Session key for storing the authenticated user's ID.
const SESSION_USER_KEY: &str = "_auth_user_id";
/// Session key for the authentication backend class path.
const SESSION_BACKEND_KEY: &str = "_auth_user_backend";
/// Session key for the password hash (to detect password changes).
const SESSION_HASH_KEY: &str = "_auth_user_hash";

/// Credentials for authentication.
///
/// Supports authentication by username, email, or both, combined with a password.
#[derive(Debug, Clone)]
pub struct Credentials {
    /// The username to authenticate with.
    pub username: Option<String>,
    /// The email to authenticate with.
    pub email: Option<String>,
    /// The password to verify.
    pub password: String,
}

impl Credentials {
    /// Creates credentials with username and password.
    pub fn with_username(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: Some(username.into()),
            email: None,
            password: password.into(),
        }
    }

    /// Creates credentials with email and password.
    pub fn with_email(email: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: None,
            email: Some(email.into()),
            password: password.into(),
        }
    }
}

/// Trait for authentication backends.
///
/// Implementations provide pluggable authentication strategies. Each backend
/// must be `Send + Sync` for safe concurrent use across async tasks.
#[async_trait]
pub trait AuthBackend: Send + Sync {
    /// Attempts to authenticate a user with the given credentials.
    ///
    /// Returns `Ok(Some(user))` on success, `Ok(None)` if the credentials
    /// don't match (but are not erroneous), or `Err` on backend failure.
    async fn authenticate(
        &self,
        credentials: &Credentials,
    ) -> Result<Option<AbstractUser>, DjangoError>;

    /// Retrieves a user by their unique identifier (e.g., username).
    async fn get_user(&self, user_id: &str) -> Result<Option<AbstractUser>, DjangoError>;

    /// Checks if a user has a specific permission.
    fn has_perm(&self, user: &AbstractUser, perm: &str) -> bool;

    /// Returns all permissions for a user.
    fn get_all_permissions(&self, user: &AbstractUser) -> HashSet<String>;
}

/// Model-based authentication backend.
///
/// Authenticates users against an in-memory user store. In a full implementation,
/// this would query the database. Mirrors Django's `ModelBackend`.
#[derive(Debug, Default)]
pub struct ModelBackend {
    /// In-memory user store for testing and simple use cases.
    users: Arc<RwLock<Vec<AbstractUser>>>,
}

impl ModelBackend {
    /// Creates a new `ModelBackend` with an empty user store.
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Adds a user to the backend's store.
    pub async fn add_user(&self, user: AbstractUser) {
        self.users.write().await.push(user);
    }
}

#[async_trait]
#[allow(clippy::significant_drop_tightening)]
impl AuthBackend for ModelBackend {
    async fn authenticate(
        &self,
        credentials: &Credentials,
    ) -> Result<Option<AbstractUser>, DjangoError> {
        let users = self.users.read().await;

        for user in users.iter() {
            // Match by username or email
            let matches = match (&credentials.username, &credentials.email) {
                (Some(username), _) => user.username == *username,
                (_, Some(email)) => user.email == *email,
                (None, None) => false,
            };

            if matches && user.base.is_active {
                // Verify password
                if user.check_password(&credentials.password).await? {
                    return Ok(Some(user.clone()));
                }
            }
        }

        Ok(None)
    }

    async fn get_user(&self, user_id: &str) -> Result<Option<AbstractUser>, DjangoError> {
        let users = self.users.read().await;
        Ok(users.iter().find(|u| u.username == user_id).cloned())
    }

    fn has_perm(&self, user: &AbstractUser, perm: &str) -> bool {
        crate::permissions::has_perm(user, perm)
    }

    fn get_all_permissions(&self, user: &AbstractUser) -> HashSet<String> {
        crate::permissions::get_all_permissions(user)
    }
}

/// Remote user authentication backend.
///
/// Trusts the `REMOTE_USER` header for authentication. Used when authentication
/// is handled by a reverse proxy (e.g., Apache, Nginx with `auth_basic`).
///
/// WARNING: Only use this backend when you trust the proxy to authenticate users.
#[derive(Debug, Default)]
pub struct RemoteUserBackend {
    /// Known users (in a real implementation, this would create users on first login).
    users: Arc<RwLock<Vec<AbstractUser>>>,
}

impl RemoteUserBackend {
    /// Creates a new `RemoteUserBackend`.
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Adds a user to the backend's store.
    pub async fn add_user(&self, user: AbstractUser) {
        self.users.write().await.push(user);
    }

    /// Authenticates a user by remote username (no password check).
    #[allow(clippy::significant_drop_tightening)]
    pub async fn authenticate_remote(
        &self,
        remote_user: &str,
    ) -> Result<Option<AbstractUser>, DjangoError> {
        let users = self.users.read().await;
        if let Some(user) = users.iter().find(|u| u.username == remote_user) {
            if user.base.is_active {
                return Ok(Some(user.clone()));
            }
        }

        // In a real implementation, auto-create the user here
        Ok(None)
    }
}

#[async_trait]
impl AuthBackend for RemoteUserBackend {
    async fn authenticate(
        &self,
        credentials: &Credentials,
    ) -> Result<Option<AbstractUser>, DjangoError> {
        // RemoteUserBackend uses the username as the remote user identity
        if let Some(username) = &credentials.username {
            return self.authenticate_remote(username).await;
        }
        Ok(None)
    }

    async fn get_user(&self, user_id: &str) -> Result<Option<AbstractUser>, DjangoError> {
        let users = self.users.read().await;
        Ok(users.iter().find(|u| u.username == user_id).cloned())
    }

    fn has_perm(&self, user: &AbstractUser, perm: &str) -> bool {
        crate::permissions::has_perm(user, perm)
    }

    fn get_all_permissions(&self, user: &AbstractUser) -> HashSet<String> {
        crate::permissions::get_all_permissions(user)
    }
}

/// Authenticates a user against a list of backends.
///
/// Tries each backend in order. Returns the first successfully authenticated user,
/// or `None` if no backend accepts the credentials.
pub async fn authenticate(
    credentials: &Credentials,
    backends: &[Box<dyn AuthBackend>],
) -> Result<Option<AbstractUser>, DjangoError> {
    for backend in backends {
        if let Some(user) = backend.authenticate(credentials).await? {
            return Ok(Some(user));
        }
    }
    Ok(None)
}

/// Logs a user into the session.
///
/// Stores the user's identity in the session data so subsequent requests
/// can identify the authenticated user.
pub fn login(session: &mut SessionData, user: &AbstractUser) {
    session.set(
        SESSION_USER_KEY,
        serde_json::Value::String(user.username.clone()),
    );
    session.set(
        SESSION_BACKEND_KEY,
        serde_json::Value::String("django_rs.auth.backends.ModelBackend".to_string()),
    );
    // Store a portion of the password hash to detect password changes.
    // Uses first 40 chars to capture algorithm + salt for reliable detection.
    let end = std::cmp::min(user.base.password.len(), 40);
    let hash_fragment = &user.base.password[..end];
    session.set(
        SESSION_HASH_KEY,
        serde_json::Value::String(hash_fragment.to_string()),
    );
}

/// Logs a user out by clearing auth data from the session.
///
/// Removes all authentication-related keys from the session.
pub fn logout(session: &mut SessionData) {
    session.remove(SESSION_USER_KEY);
    session.remove(SESSION_BACKEND_KEY);
    session.remove(SESSION_HASH_KEY);
}

/// Returns the user ID stored in the session, if any.
pub fn get_session_user_id(session: &SessionData) -> Option<String> {
    session
        .get(SESSION_USER_KEY)
        .and_then(|v| v.as_str())
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_user(username: &str, password: &str) -> AbstractUser {
        let mut user = AbstractUser::new(username);
        user.set_password(password).await.unwrap();
        user
    }

    // ── Credentials tests ───────────────────────────────────────────

    #[test]
    fn test_credentials_with_username() {
        let creds = Credentials::with_username("alice", "password123");
        assert_eq!(creds.username.as_deref(), Some("alice"));
        assert!(creds.email.is_none());
        assert_eq!(creds.password, "password123");
    }

    #[test]
    fn test_credentials_with_email() {
        let creds = Credentials::with_email("alice@example.com", "password123");
        assert!(creds.username.is_none());
        assert_eq!(creds.email.as_deref(), Some("alice@example.com"));
    }

    // ── ModelBackend tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_model_backend_authenticate_success() {
        let backend = ModelBackend::new();
        let user = create_test_user("alice", "password123").await;
        backend.add_user(user).await;

        let creds = Credentials::with_username("alice", "password123");
        let result = backend.authenticate(&creds).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().username, "alice");
    }

    #[tokio::test]
    async fn test_model_backend_authenticate_wrong_password() {
        let backend = ModelBackend::new();
        let user = create_test_user("alice", "password123").await;
        backend.add_user(user).await;

        let creds = Credentials::with_username("alice", "wrongpassword");
        let result = backend.authenticate(&creds).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_model_backend_authenticate_unknown_user() {
        let backend = ModelBackend::new();
        let creds = Credentials::with_username("unknown", "password");
        let result = backend.authenticate(&creds).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_model_backend_authenticate_inactive_user() {
        let backend = ModelBackend::new();
        let mut user = create_test_user("alice", "password123").await;
        user.base.is_active = false;
        backend.add_user(user).await;

        let creds = Credentials::with_username("alice", "password123");
        let result = backend.authenticate(&creds).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_model_backend_authenticate_by_email() {
        let backend = ModelBackend::new();
        let mut user = create_test_user("alice", "password123").await;
        user.email = "alice@example.com".to_string();
        backend.add_user(user).await;

        let creds = Credentials::with_email("alice@example.com", "password123");
        let result = backend.authenticate(&creds).await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_model_backend_get_user() {
        let backend = ModelBackend::new();
        let user = create_test_user("alice", "password123").await;
        backend.add_user(user).await;

        let result = backend.get_user("alice").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().username, "alice");
    }

    #[tokio::test]
    async fn test_model_backend_get_user_not_found() {
        let backend = ModelBackend::new();
        let result = backend.get_user("unknown").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_model_backend_has_perm() {
        let backend = ModelBackend::new();
        let mut user = AbstractUser::new("alice");
        user.user_permissions = vec!["blog.add_post".to_string()];
        assert!(backend.has_perm(&user, "blog.add_post"));
        assert!(!backend.has_perm(&user, "blog.delete_post"));
    }

    #[tokio::test]
    async fn test_model_backend_get_all_permissions() {
        let backend = ModelBackend::new();
        let mut user = AbstractUser::new("alice");
        user.user_permissions = vec!["blog.add_post".to_string(), "blog.change_post".to_string()];
        let perms = backend.get_all_permissions(&user);
        assert_eq!(perms.len(), 2);
    }

    // ── RemoteUserBackend tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_remote_backend_authenticate() {
        let backend = RemoteUserBackend::new();
        let user = AbstractUser::new("proxy_user");
        backend.add_user(user).await;

        let result = backend.authenticate_remote("proxy_user").await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_remote_backend_authenticate_unknown() {
        let backend = RemoteUserBackend::new();
        let result = backend.authenticate_remote("unknown").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_remote_backend_authenticate_via_trait() {
        let backend = RemoteUserBackend::new();
        let user = AbstractUser::new("proxy_user");
        backend.add_user(user).await;

        let creds = Credentials::with_username("proxy_user", "");
        let result = backend.authenticate(&creds).await.unwrap();
        assert!(result.is_some());
    }

    // ── authenticate function tests ─────────────────────────────────

    #[tokio::test]
    async fn test_authenticate_first_backend() {
        let backend = ModelBackend::new();
        let user = create_test_user("alice", "pass123").await;
        backend.add_user(user).await;

        let backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(backend)];
        let creds = Credentials::with_username("alice", "pass123");
        let result = authenticate(&creds, &backends).await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_authenticate_second_backend() {
        let first = ModelBackend::new();
        let second = ModelBackend::new();
        let user = create_test_user("bob", "pass456").await;
        second.add_user(user).await;

        let backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(first), Box::new(second)];
        let creds = Credentials::with_username("bob", "pass456");
        let result = authenticate(&creds, &backends).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().username, "bob");
    }

    #[tokio::test]
    async fn test_authenticate_no_backends() {
        let backends: Vec<Box<dyn AuthBackend>> = vec![];
        let creds = Credentials::with_username("alice", "pass");
        let result = authenticate(&creds, &backends).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_authenticate_all_fail() {
        let backend = ModelBackend::new();
        let backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(backend)];
        let creds = Credentials::with_username("nonexistent", "pass");
        let result = authenticate(&creds, &backends).await.unwrap();
        assert!(result.is_none());
    }

    // ── login / logout tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_login_stores_user_in_session() {
        let user = create_test_user("alice", "pass123").await;
        let mut session = SessionData::new("test-session".to_string());

        login(&mut session, &user);

        let user_id = get_session_user_id(&session);
        assert_eq!(user_id.as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn test_login_stores_backend() {
        let user = create_test_user("alice", "pass123").await;
        let mut session = SessionData::new("test-session".to_string());

        login(&mut session, &user);

        let backend = session.get(SESSION_BACKEND_KEY).unwrap();
        assert!(backend.as_str().unwrap().contains("ModelBackend"));
    }

    #[tokio::test]
    async fn test_logout_clears_session() {
        let user = create_test_user("alice", "pass123").await;
        let mut session = SessionData::new("test-session".to_string());

        login(&mut session, &user);
        assert!(get_session_user_id(&session).is_some());

        logout(&mut session);
        assert!(get_session_user_id(&session).is_none());
        assert!(session.get(SESSION_BACKEND_KEY).is_none());
        assert!(session.get(SESSION_HASH_KEY).is_none());
    }

    #[tokio::test]
    async fn test_login_then_logout_then_login() {
        let user1 = create_test_user("alice", "pass123").await;
        let user2 = create_test_user("bob", "pass456").await;
        let mut session = SessionData::new("test-session".to_string());

        login(&mut session, &user1);
        assert_eq!(get_session_user_id(&session).as_deref(), Some("alice"));

        logout(&mut session);
        assert!(get_session_user_id(&session).is_none());

        login(&mut session, &user2);
        assert_eq!(get_session_user_id(&session).as_deref(), Some("bob"));
    }

    #[test]
    fn test_get_session_user_id_empty_session() {
        let session = SessionData::new("test-session".to_string());
        assert!(get_session_user_id(&session).is_none());
    }
}
