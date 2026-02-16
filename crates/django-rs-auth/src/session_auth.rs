//! Auth-session integration for django-rs.
//!
//! This module connects the authentication system to the session framework,
//! providing functions to:
//!
//! - Store authentication state in request META (which maps to session data)
//! - Clear authentication state on logout
//! - Load a user from session data by querying an auth backend
//!
//! ## Session Keys
//!
//! Authentication state is stored using three session keys:
//! - `_auth_user_id` - The authenticated user's username/identifier
//! - `_auth_user_backend` - The backend class that authenticated the user
//! - `_auth_user_hash` - A fragment of the password hash for invalidation
//!
//! ## META Integration
//!
//! The session middleware serializes session data into request META entries.
//! This module reads/writes session data through META to avoid direct
//! session coupling, following the same pattern established in Wave 7.

use django_rs_http::HttpRequest;
use django_rs_views::session::SessionData;

use crate::backends::AuthBackend;
use crate::user::AbstractUser;

/// Session key for storing the authenticated user's ID.
const SESSION_USER_KEY: &str = "_auth_user_id";
/// Session key for the authentication backend class path.
const SESSION_BACKEND_KEY: &str = "_auth_user_backend";
/// Session key for the password hash fragment (to detect password changes).
const SESSION_HASH_KEY: &str = "_auth_user_hash";
/// META key indicating whether the current user is authenticated.
const META_USER_AUTHENTICATED: &str = "USER_AUTHENTICATED";

/// Computes the session authentication hash from a user's password hash.
///
/// Uses the first 40 characters of the password hash as a fingerprint.
/// This captures enough of the hash (including salt and algorithm-specific
/// parameters) to reliably detect when a password has been changed.
fn session_auth_hash(password_hash: &str) -> String {
    let end = std::cmp::min(password_hash.len(), 40);
    password_hash[..end].to_string()
}

/// Stores user authentication state in the request META.
///
/// After calling this, the session middleware will persist the auth data
/// when it processes the response. The following META entries are set:
///
/// - `SESSION_DATA` is updated with auth keys serialized as JSON
/// - `SESSION_MODIFIED` is set to `"true"` to trigger persistence
/// - `USER_AUTHENTICATED` is set to `"true"` for downstream middleware/views
///
/// This mirrors Django's `django.contrib.auth.login()`.
pub fn login_to_session(request: &mut HttpRequest, user: &AbstractUser) {
    login_to_session_with_backend(request, user, "django_rs.auth.backends.ModelBackend");
}

/// Stores user authentication state with a specific backend name.
///
/// This variant allows specifying which backend authenticated the user,
/// which is useful when multiple backends are configured.
pub fn login_to_session_with_backend(
    request: &mut HttpRequest,
    user: &AbstractUser,
    backend: &str,
) {
    let hash_fragment = session_auth_hash(&user.base.password);

    // Build the session data by merging auth keys into existing session data
    let meta = request.meta_mut();

    // Parse existing session data
    let session_data_str = meta
        .get("SESSION_DATA")
        .cloned()
        .unwrap_or_else(|| "{}".to_string());
    let mut data: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&session_data_str).unwrap_or_default();

    // Set auth keys
    data.insert(
        SESSION_USER_KEY.to_string(),
        serde_json::Value::String(user.username.clone()),
    );
    data.insert(
        SESSION_BACKEND_KEY.to_string(),
        serde_json::Value::String(backend.to_string()),
    );
    data.insert(
        SESSION_HASH_KEY.to_string(),
        serde_json::Value::String(hash_fragment),
    );

    // Serialize back and mark as modified
    let updated_json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    meta.insert("SESSION_DATA".to_string(), updated_json);
    meta.insert("SESSION_MODIFIED".to_string(), "true".to_string());
    meta.insert(META_USER_AUTHENTICATED.to_string(), "true".to_string());
}

/// Clears authentication state from the request META.
///
/// Removes all auth-related keys from the session data and marks the
/// session as modified. Sets `USER_AUTHENTICATED` to `"false"`.
///
/// This mirrors Django's `django.contrib.auth.logout()`.
pub fn logout_from_session(request: &mut HttpRequest) {
    let meta = request.meta_mut();

    // Parse existing session data
    let session_data_str = meta
        .get("SESSION_DATA")
        .cloned()
        .unwrap_or_else(|| "{}".to_string());
    let mut data: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&session_data_str).unwrap_or_default();

    // Remove auth keys
    data.remove(SESSION_USER_KEY);
    data.remove(SESSION_BACKEND_KEY);
    data.remove(SESSION_HASH_KEY);

    // Serialize back and mark as modified
    let updated_json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    meta.insert("SESSION_DATA".to_string(), updated_json);
    meta.insert("SESSION_MODIFIED".to_string(), "true".to_string());
    meta.insert(META_USER_AUTHENTICATED.to_string(), "false".to_string());
}

/// Checks whether the current request has an authenticated user.
///
/// Reads the `USER_AUTHENTICATED` META key set by [`login_to_session`].
pub fn is_authenticated(request: &HttpRequest) -> bool {
    request
        .meta()
        .get(META_USER_AUTHENTICATED)
        .is_some_and(|v| v == "true")
}

/// Retrieves the authenticated user's ID from the session data in META.
///
/// Returns `None` if no user is logged in or if the session data is missing.
pub fn get_user_id_from_meta(request: &HttpRequest) -> Option<String> {
    let session_data_str = request.meta().get("SESSION_DATA")?;
    let data: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(session_data_str).ok()?;
    data.get(SESSION_USER_KEY)
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Retrieves the authentication backend name from the session data in META.
pub fn get_backend_from_meta(request: &HttpRequest) -> Option<String> {
    let session_data_str = request.meta().get("SESSION_DATA")?;
    let data: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(session_data_str).ok()?;
    data.get(SESSION_BACKEND_KEY)
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Retrieves the session auth hash from the session data in META.
#[cfg(test)]
fn get_session_hash_from_meta(request: &HttpRequest) -> Option<String> {
    let session_data_str = request.meta().get("SESSION_DATA")?;
    let data: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(session_data_str).ok()?;
    data.get(SESSION_HASH_KEY)
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Loads the authenticated user from session data by querying an auth backend.
///
/// This function:
/// 1. Reads `_auth_user_id` from the session data
/// 2. Queries the backend for the user with that ID
/// 3. Verifies the session auth hash matches the current password hash
/// 4. Returns the user if all checks pass, `None` otherwise
///
/// This is called by the authentication middleware on each request to
/// populate the request with the current user.
pub async fn get_user_from_session(
    session_data: &SessionData,
    backend: &dyn AuthBackend,
) -> Option<AbstractUser> {
    // Read user ID from session
    let user_id = session_data
        .get(SESSION_USER_KEY)
        .and_then(|v| v.as_str())?;

    // Read stored auth hash
    let stored_hash = session_data
        .get(SESSION_HASH_KEY)
        .and_then(|v| v.as_str())?;

    // Query backend for the user
    let user = backend.get_user(user_id).await.ok()??;

    // Verify the session auth hash matches current password
    let current_hash = session_auth_hash(&user.base.password);
    if constant_time_eq(stored_hash.as_bytes(), current_hash.as_bytes()) {
        Some(user)
    } else {
        // Password has changed since session was created
        None
    }
}

/// Loads the authenticated user from request META by querying an auth backend.
///
/// Convenience function that reads session data from META and delegates
/// to [`get_user_from_session`].
pub async fn get_user_from_request(
    request: &HttpRequest,
    backend: &dyn AuthBackend,
) -> Option<AbstractUser> {
    let session_data_str = request.meta().get("SESSION_DATA")?;
    let data: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(session_data_str).ok()?;

    // Build a temporary SessionData from the META
    let session_key = request
        .meta()
        .get("SESSION_KEY")
        .cloned()
        .unwrap_or_default();
    let mut session = SessionData::new(session_key);
    for (k, v) in data {
        session.set(&k, v);
    }

    get_user_from_session(&session, backend).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user::AbstractUser;

    async fn create_test_user(username: &str, password: &str) -> AbstractUser {
        let mut user = AbstractUser::new(username);
        user.set_password(password).await.unwrap();
        user
    }

    fn make_request() -> HttpRequest {
        HttpRequest::builder()
            .meta("SESSION_KEY", "test-session-key")
            .meta("SESSION_DATA", "{}")
            .meta("SESSION_MODIFIED", "false")
            .meta("SESSION_IS_NEW", "true")
            .build()
    }

    fn make_request_with_session(session_json: &str) -> HttpRequest {
        HttpRequest::builder()
            .meta("SESSION_KEY", "test-session-key")
            .meta("SESSION_DATA", session_json)
            .meta("SESSION_MODIFIED", "false")
            .meta("SESSION_IS_NEW", "false")
            .build()
    }

    // ── login_to_session tests ──────────────────────────────────────

    #[tokio::test]
    async fn test_login_to_session_sets_user_authenticated() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request();

        login_to_session(&mut request, &user);

        assert_eq!(
            request.meta().get(META_USER_AUTHENTICATED),
            Some(&"true".to_string())
        );
    }

    #[tokio::test]
    async fn test_login_to_session_stores_user_id() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request();

        login_to_session(&mut request, &user);

        let user_id = get_user_id_from_meta(&request);
        assert_eq!(user_id.as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn test_login_to_session_stores_backend() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request();

        login_to_session(&mut request, &user);

        let backend = get_backend_from_meta(&request);
        assert!(backend.unwrap().contains("ModelBackend"));
    }

    #[tokio::test]
    async fn test_login_to_session_stores_hash() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request();

        login_to_session(&mut request, &user);

        let hash = get_session_hash_from_meta(&request);
        assert!(hash.is_some());
        let hash = hash.unwrap();
        assert!(!hash.is_empty());
        // Hash fragment should match the first 10 chars of the password hash
        let expected = session_auth_hash(&user.base.password);
        assert_eq!(hash, expected);
    }

    #[tokio::test]
    async fn test_login_to_session_marks_modified() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request();

        login_to_session(&mut request, &user);

        assert_eq!(
            request.meta().get("SESSION_MODIFIED"),
            Some(&"true".to_string())
        );
    }

    #[tokio::test]
    async fn test_login_to_session_with_custom_backend() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request();

        login_to_session_with_backend(&mut request, &user, "myapp.backends.LDAPBackend");

        let backend = get_backend_from_meta(&request);
        assert_eq!(backend.as_deref(), Some("myapp.backends.LDAPBackend"));
    }

    #[tokio::test]
    async fn test_login_preserves_existing_session_data() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request_with_session(r#"{"theme":"dark","lang":"en"}"#);

        login_to_session(&mut request, &user);

        // Auth data should be present
        let user_id = get_user_id_from_meta(&request);
        assert_eq!(user_id.as_deref(), Some("alice"));

        // Existing data should be preserved
        let session_data_str = request.meta().get("SESSION_DATA").unwrap();
        let data: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(session_data_str).unwrap();
        assert_eq!(data.get("theme").and_then(|v| v.as_str()), Some("dark"));
        assert_eq!(data.get("lang").and_then(|v| v.as_str()), Some("en"));
    }

    // ── logout_from_session tests ───────────────────────────────────

    #[tokio::test]
    async fn test_logout_clears_user_authenticated() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request();

        login_to_session(&mut request, &user);
        assert!(is_authenticated(&request));

        logout_from_session(&mut request);
        assert!(!is_authenticated(&request));
    }

    #[tokio::test]
    async fn test_logout_removes_user_id() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request();

        login_to_session(&mut request, &user);
        assert!(get_user_id_from_meta(&request).is_some());

        logout_from_session(&mut request);
        assert!(get_user_id_from_meta(&request).is_none());
    }

    #[tokio::test]
    async fn test_logout_removes_backend() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request();

        login_to_session(&mut request, &user);
        assert!(get_backend_from_meta(&request).is_some());

        logout_from_session(&mut request);
        assert!(get_backend_from_meta(&request).is_none());
    }

    #[tokio::test]
    async fn test_logout_removes_hash() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request();

        login_to_session(&mut request, &user);
        assert!(get_session_hash_from_meta(&request).is_some());

        logout_from_session(&mut request);
        assert!(get_session_hash_from_meta(&request).is_none());
    }

    #[tokio::test]
    async fn test_logout_marks_modified() {
        let mut request = make_request();
        logout_from_session(&mut request);

        assert_eq!(
            request.meta().get("SESSION_MODIFIED"),
            Some(&"true".to_string())
        );
    }

    #[tokio::test]
    async fn test_logout_preserves_non_auth_session_data() {
        let user = create_test_user("alice", "pass123").await;
        let mut request = make_request_with_session(r#"{"theme":"dark"}"#);

        login_to_session(&mut request, &user);
        logout_from_session(&mut request);

        // Auth data should be removed
        assert!(get_user_id_from_meta(&request).is_none());

        // Non-auth data should be preserved
        let session_data_str = request.meta().get("SESSION_DATA").unwrap();
        let data: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(session_data_str).unwrap();
        assert_eq!(data.get("theme").and_then(|v| v.as_str()), Some("dark"));
    }

    // ── get_user_from_session tests ─────────────────────────────────

    #[tokio::test]
    async fn test_get_user_from_session_valid() {
        let user = create_test_user("alice", "pass123").await;
        let backend = crate::backends::ModelBackend::new();
        backend.add_user(user.clone()).await;

        let mut session = SessionData::new("test-session".to_string());
        crate::backends::login(&mut session, &user);

        let loaded = get_user_from_session(&session, &backend).await;
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().username, "alice");
    }

    #[tokio::test]
    async fn test_get_user_from_session_no_user_id() {
        let backend = crate::backends::ModelBackend::new();
        let session = SessionData::new("test-session".to_string());

        let loaded = get_user_from_session(&session, &backend).await;
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_get_user_from_session_user_not_in_backend() {
        let user = create_test_user("alice", "pass123").await;
        let backend = crate::backends::ModelBackend::new();
        // Note: user not added to backend

        let mut session = SessionData::new("test-session".to_string());
        crate::backends::login(&mut session, &user);

        let loaded = get_user_from_session(&session, &backend).await;
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_get_user_from_session_password_changed() {
        let user = create_test_user("alice", "pass123").await;
        let backend = crate::backends::ModelBackend::new();

        // Create session with original password hash
        let mut session = SessionData::new("test-session".to_string());
        crate::backends::login(&mut session, &user);

        // Add user with a different password to backend
        let changed_user = create_test_user("alice", "newpassword").await;
        backend.add_user(changed_user).await;

        // Session should be invalidated because hash fragment differs
        let loaded = get_user_from_session(&session, &backend).await;
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_get_user_from_session_same_password() {
        // User in session and backend have the same password hash
        let user = create_test_user("alice", "pass123").await;
        let backend = crate::backends::ModelBackend::new();
        backend.add_user(user.clone()).await;

        let mut session = SessionData::new("test-session".to_string());
        crate::backends::login(&mut session, &user);

        let loaded = get_user_from_session(&session, &backend).await;
        assert!(loaded.is_some());
    }

    // ── get_user_from_request tests ─────────────────────────────────

    #[tokio::test]
    async fn test_get_user_from_request_logged_in() {
        let user = create_test_user("alice", "pass123").await;
        let backend = crate::backends::ModelBackend::new();
        backend.add_user(user.clone()).await;

        let mut request = make_request();
        login_to_session(&mut request, &user);

        let loaded = get_user_from_request(&request, &backend).await;
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().username, "alice");
    }

    #[tokio::test]
    async fn test_get_user_from_request_not_logged_in() {
        let backend = crate::backends::ModelBackend::new();
        let request = make_request();

        let loaded = get_user_from_request(&request, &backend).await;
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_get_user_from_request_after_logout() {
        let user = create_test_user("alice", "pass123").await;
        let backend = crate::backends::ModelBackend::new();
        backend.add_user(user.clone()).await;

        let mut request = make_request();
        login_to_session(&mut request, &user);
        logout_from_session(&mut request);

        let loaded = get_user_from_request(&request, &backend).await;
        assert!(loaded.is_none());
    }

    // ── is_authenticated tests ──────────────────────────────────────

    #[test]
    fn test_is_authenticated_true() {
        let request = HttpRequest::builder()
            .meta(META_USER_AUTHENTICATED, "true")
            .build();
        assert!(is_authenticated(&request));
    }

    #[test]
    fn test_is_authenticated_false() {
        let request = HttpRequest::builder()
            .meta(META_USER_AUTHENTICATED, "false")
            .build();
        assert!(!is_authenticated(&request));
    }

    #[test]
    fn test_is_authenticated_missing() {
        let request = HttpRequest::builder().build();
        assert!(!is_authenticated(&request));
    }

    // ── session_auth_hash tests ─────────────────────────────────────

    #[test]
    fn test_session_auth_hash_long_password() {
        let hash = session_auth_hash("$argon2id$v=19$m=19456,t=2,p=1$abc$def");
        assert_eq!(hash.len(), 38); // Full hash is 38 chars, so all taken
        assert_eq!(hash, "$argon2id$v=19$m=19456,t=2,p=1$abc$def");
    }

    #[test]
    fn test_session_auth_hash_very_long_password() {
        let long_hash = "$argon2id$v=19$m=19456,t=2,p=1$abcdefghijklmnop$qrstuvwxyz1234567890";
        let hash = session_auth_hash(long_hash);
        assert_eq!(hash.len(), 40);
        assert_eq!(hash, &long_hash[..40]);
    }

    #[test]
    fn test_session_auth_hash_short_password() {
        let hash = session_auth_hash("short");
        assert_eq!(hash, "short");
    }

    #[test]
    fn test_session_auth_hash_empty() {
        let hash = session_auth_hash("");
        assert_eq!(hash, "");
    }

    // ── constant_time_eq tests ──────────────────────────────────────

    #[test]
    fn test_constant_time_eq_same() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn test_constant_time_eq_different() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn test_constant_time_eq_different_lengths() {
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    // ── login/logout cycle tests ────────────────────────────────────

    #[tokio::test]
    async fn test_login_logout_login_different_user() {
        let user1 = create_test_user("alice", "pass123").await;
        let user2 = create_test_user("bob", "pass456").await;
        let mut request = make_request();

        // Login as alice
        login_to_session(&mut request, &user1);
        assert_eq!(get_user_id_from_meta(&request).as_deref(), Some("alice"));

        // Logout
        logout_from_session(&mut request);
        assert!(get_user_id_from_meta(&request).is_none());

        // Login as bob
        login_to_session(&mut request, &user2);
        assert_eq!(get_user_id_from_meta(&request).as_deref(), Some("bob"));
    }

    #[tokio::test]
    async fn test_login_replaces_previous_user() {
        let user1 = create_test_user("alice", "pass123").await;
        let user2 = create_test_user("bob", "pass456").await;
        let mut request = make_request();

        login_to_session(&mut request, &user1);
        assert_eq!(get_user_id_from_meta(&request).as_deref(), Some("alice"));

        // Login as bob without explicit logout
        login_to_session(&mut request, &user2);
        assert_eq!(get_user_id_from_meta(&request).as_deref(), Some("bob"));
    }
}
