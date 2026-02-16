//! Integration tests for the Auth + Session + CSRF pipeline.
//!
//! Tests the interaction between authentication, session management, CSRF protection,
//! password hashing, and permission enforcement components working together.

use django_rs_auth::backends::{
    authenticate, login, logout, AuthBackend, Credentials, ModelBackend,
};
use django_rs_auth::csrf::{
    generate_csrf_token, mask_csrf_token, unmask_csrf_token, validate_csrf_token, CsrfMiddleware,
};
use django_rs_auth::hashers::{
    check_password, make_password, Argon2Hasher, BcryptHasher, PasswordHasher, Pbkdf2Hasher,
};
use django_rs_auth::permissions::{has_perm, has_perm_with_groups, Group, Permission};
use django_rs_auth::session_auth::{
    get_backend_from_meta, get_user_from_request, get_user_from_session, get_user_id_from_meta,
    is_authenticated, login_to_session, login_to_session_with_backend, logout_from_session,
};
use django_rs_auth::user::{AbstractUser, AnonymousUser};
use django_rs_http::HttpRequest;
use django_rs_views::middleware::Middleware;
use django_rs_views::session::{
    CookieSessionBackend, InMemorySessionBackend, SessionBackend, SessionData,
    SignedCookieSessionBackend,
};
use django_rs_views::views::function::{
    login_required, login_required_redirect, permission_required, ViewFunction,
};

// ── Helpers ──────────────────────────────────────────────────────────

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

fn make_view() -> ViewFunction {
    Box::new(|_req| Box::pin(async { django_rs_http::HttpResponse::ok("success") }))
}

// ═══════════════════════════════════════════════════════════════════════
// 1. LOGIN FLOW (~12 tests)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn login_valid_credentials_sets_session_data() {
    let user = create_test_user("alice", "Str0ngP@ss!").await;
    let mut request = make_request();

    login_to_session(&mut request, &user);

    assert!(is_authenticated(&request));
    assert_eq!(get_user_id_from_meta(&request).as_deref(), Some("alice"));
    assert!(get_backend_from_meta(&request).is_some());
}

#[tokio::test]
async fn login_invalid_password_fails_authentication() {
    let user = create_test_user("alice", "correct_password").await;
    let backend = ModelBackend::new();
    backend.add_user(user).await;

    let creds = Credentials::with_username("alice", "wrong_password");
    let backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(backend)];
    let result = authenticate(&creds, &backends).await.unwrap();
    assert!(
        result.is_none(),
        "Authentication should fail with wrong password"
    );
}

#[tokio::test]
async fn login_nonexistent_user_fails() {
    let backend = ModelBackend::new();

    let creds = Credentials::with_username("ghost", "password");
    let backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(backend)];
    let result = authenticate(&creds, &backends).await.unwrap();
    assert!(
        result.is_none(),
        "Authentication should fail for nonexistent user"
    );
}

#[tokio::test]
async fn login_session_contains_user_id_and_backend() {
    let user = create_test_user("bob", "B0bStr0ng!").await;
    let mut request = make_request();

    login_to_session(&mut request, &user);

    let user_id = get_user_id_from_meta(&request);
    assert_eq!(user_id.as_deref(), Some("bob"));

    let backend = get_backend_from_meta(&request);
    assert!(backend.is_some());
    assert!(backend.unwrap().contains("ModelBackend"));
}

#[tokio::test]
async fn login_inactive_user_fails_authentication() {
    let mut user = create_test_user("inactive_alice", "password").await;
    user.base.is_active = false;

    let backend = ModelBackend::new();
    backend.add_user(user).await;

    let creds = Credentials::with_username("inactive_alice", "password");
    let backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(backend)];
    let result = authenticate(&creds, &backends).await.unwrap();
    assert!(result.is_none(), "Inactive user should not authenticate");
}

#[tokio::test]
async fn login_generates_session_key_in_meta() {
    let user = create_test_user("charlie", "Ch@rlie123").await;
    let mut request = make_request();

    login_to_session(&mut request, &user);

    // SESSION_KEY should exist and not be empty
    let session_key = request.meta().get("SESSION_KEY");
    assert!(session_key.is_some());
    assert!(!session_key.unwrap().is_empty());
}

#[tokio::test]
async fn login_marks_session_modified() {
    let user = create_test_user("dave", "D@veP@ss!").await;
    let mut request = make_request();

    login_to_session(&mut request, &user);

    assert_eq!(
        request.meta().get("SESSION_MODIFIED"),
        Some(&"true".to_string())
    );
}

#[tokio::test]
async fn multiple_logins_replace_user_data() {
    let user1 = create_test_user("user_a", "P@ssw0rd1").await;
    let user2 = create_test_user("user_b", "P@ssw0rd2").await;
    let mut request = make_request();

    login_to_session(&mut request, &user1);
    assert_eq!(get_user_id_from_meta(&request).as_deref(), Some("user_a"));

    // Login as different user replaces user_id
    login_to_session(&mut request, &user2);
    assert_eq!(get_user_id_from_meta(&request).as_deref(), Some("user_b"));
}

#[tokio::test]
async fn login_with_custom_backend_stores_backend_name() {
    let user = create_test_user("eve", "Ev3P@ss!").await;
    let mut request = make_request();

    login_to_session_with_backend(&mut request, &user, "myapp.backends.LDAPBackend");

    let backend = get_backend_from_meta(&request);
    assert_eq!(backend.as_deref(), Some("myapp.backends.LDAPBackend"));
}

#[tokio::test]
async fn login_preserves_existing_session_data() {
    let user = create_test_user("frank", "Fr@nk!P@ss").await;
    let mut request = HttpRequest::builder()
        .meta("SESSION_KEY", "test-session-key")
        .meta("SESSION_DATA", r#"{"theme":"dark","locale":"en"}"#)
        .meta("SESSION_MODIFIED", "false")
        .meta("SESSION_IS_NEW", "false")
        .build();

    login_to_session(&mut request, &user);

    // Auth data present
    assert_eq!(get_user_id_from_meta(&request).as_deref(), Some("frank"));

    // Pre-existing data preserved
    let session_data_str = request.meta().get("SESSION_DATA").unwrap();
    let data: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(session_data_str).unwrap();
    assert_eq!(data.get("theme").and_then(|v| v.as_str()), Some("dark"));
    assert_eq!(data.get("locale").and_then(|v| v.as_str()), Some("en"));
}

#[tokio::test]
async fn login_via_authenticate_then_session() {
    let user = create_test_user("grace", "Gr@ce!123").await;
    let backend = ModelBackend::new();
    backend.add_user(user).await;

    // Authenticate first
    let creds = Credentials::with_username("grace", "Gr@ce!123");
    let backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(backend)];
    let authed_user = authenticate(&creds, &backends).await.unwrap().unwrap();

    // Then login to session
    let mut request = make_request();
    login_to_session(&mut request, &authed_user);

    assert!(is_authenticated(&request));
    assert_eq!(get_user_id_from_meta(&request).as_deref(), Some("grace"));
}

#[tokio::test]
async fn login_via_email_authentication() {
    let mut user = create_test_user("heidi", "H3idi!P@ss").await;
    user.email = "heidi@example.com".to_string();

    let backend = ModelBackend::new();
    backend.add_user(user).await;

    let creds = Credentials::with_email("heidi@example.com", "H3idi!P@ss");
    let backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(backend)];
    let result = authenticate(&creds, &backends).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().username, "heidi");
}

// ═══════════════════════════════════════════════════════════════════════
// 2. SESSION PERSISTENCE (~10 tests)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn session_data_persists_across_simulated_requests() {
    let user = create_test_user("ivan", "Iv@n!P@ss").await;
    let backend = ModelBackend::new();
    backend.add_user(user.clone()).await;

    // Request 1: Login
    let mut request1 = make_request();
    login_to_session(&mut request1, &user);

    // Extract session data from request1 (simulating middleware persistence)
    let session_data = request1.meta().get("SESSION_DATA").unwrap().clone();

    // Request 2: New request with persisted session data
    let request2 = HttpRequest::builder()
        .meta("SESSION_KEY", "same-session")
        .meta("SESSION_DATA", &session_data)
        .meta("USER_AUTHENTICATED", "true")
        .build();

    let loaded = get_user_from_request(&request2, &backend).await;
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().username, "ivan");
}

#[test]
fn session_set_and_get_value() {
    let mut session = SessionData::new("test-key".to_string());
    session.set("color", serde_json::json!("blue"));

    assert_eq!(session.get("color"), Some(&serde_json::json!("blue")));
    assert!(session.modified);
}

#[test]
fn session_delete_key() {
    let mut session = SessionData::new("test-key".to_string());
    session.set("temp", serde_json::json!(42));
    session.modified = false;

    let removed = session.remove("temp");
    assert_eq!(removed, Some(serde_json::json!(42)));
    assert!(session.get("temp").is_none());
    assert!(session.modified);
}

#[test]
fn session_flush_clears_all_data() {
    let mut session = SessionData::new("test-key".to_string());
    session.set("a", serde_json::json!(1));
    session.set("b", serde_json::json!(2));
    session.set("c", serde_json::json!(3));

    session.clear();
    assert!(session.is_empty());
    assert_eq!(session.len(), 0);
    assert!(session.modified);
}

#[test]
fn session_multiple_values() {
    let mut session = SessionData::new("test-key".to_string());
    session.set("name", serde_json::json!("Alice"));
    session.set("age", serde_json::json!(30));
    session.set("active", serde_json::json!(true));

    assert_eq!(session.len(), 3);
    assert_eq!(session.get("name"), Some(&serde_json::json!("Alice")));
    assert_eq!(session.get("age"), Some(&serde_json::json!(30)));
    assert_eq!(session.get("active"), Some(&serde_json::json!(true)));
}

#[tokio::test]
async fn in_memory_session_backend_stores_and_retrieves() {
    let backend = InMemorySessionBackend::new();
    let mut session = SessionData::new("inmem-1".to_string());
    session.set("user", serde_json::json!("alice"));
    session.set("theme", serde_json::json!("dark"));

    backend.save(&session).await.unwrap();

    let loaded = backend.load("inmem-1").await.unwrap();
    assert_eq!(loaded.get("user"), Some(&serde_json::json!("alice")));
    assert_eq!(loaded.get("theme"), Some(&serde_json::json!("dark")));
}

#[tokio::test]
async fn in_memory_session_backend_delete_removes_session() {
    let backend = InMemorySessionBackend::new();
    let session = SessionData::new("to-delete".to_string());
    backend.save(&session).await.unwrap();
    assert!(backend.exists("to-delete").await.unwrap());

    backend.delete("to-delete").await.unwrap();
    assert!(!backend.exists("to-delete").await.unwrap());
}

#[tokio::test]
async fn cookie_session_backend_stores_and_retrieves() {
    let backend = CookieSessionBackend::new();
    let mut session = SessionData::new("cookie-1".to_string());
    session.set("color", serde_json::json!("red"));

    backend.save(&session).await.unwrap();

    let loaded = backend.load("cookie-1").await.unwrap();
    assert_eq!(loaded.get("color"), Some(&serde_json::json!("red")));
}

#[tokio::test]
async fn signed_cookie_session_roundtrip() {
    let backend = SignedCookieSessionBackend::new("my-secret-key-for-testing");
    let mut session = SessionData::new("signed-1".to_string());
    session.set("admin", serde_json::json!(false));
    session.set("role", serde_json::json!("editor"));

    let cookie_value = backend.save(&session).await.unwrap();
    assert!(!cookie_value.is_empty());

    let loaded = backend.load(&cookie_value).await.unwrap();
    assert_eq!(loaded.get("admin"), Some(&serde_json::json!(false)));
    assert_eq!(loaded.get("role"), Some(&serde_json::json!("editor")));
}

#[tokio::test]
async fn signed_cookie_tampered_data_rejected() {
    let backend = SignedCookieSessionBackend::new("secret-key");
    let mut session = SessionData::new("test".to_string());
    session.set("admin", serde_json::json!(true));

    let cookie_value = backend.save(&session).await.unwrap();

    // Tamper with the value
    let tampered = format!("X{cookie_value}");
    let result = backend.load(&tampered).await;
    assert!(result.is_err(), "Tampered cookie should be rejected");
}

// ═══════════════════════════════════════════════════════════════════════
// 3. LOGOUT FLOW (~5 tests)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn logout_clears_session_auth_data() {
    let user = create_test_user("judy", "JudyP@ss!").await;
    let mut request = make_request();

    login_to_session(&mut request, &user);
    assert!(is_authenticated(&request));

    logout_from_session(&mut request);
    assert!(!is_authenticated(&request));
}

#[tokio::test]
async fn logout_removes_user_id_from_session() {
    let user = create_test_user("kevin", "K3vinP@ss!").await;
    let mut request = make_request();

    login_to_session(&mut request, &user);
    assert!(get_user_id_from_meta(&request).is_some());

    logout_from_session(&mut request);
    assert!(get_user_id_from_meta(&request).is_none());
}

#[tokio::test]
async fn logout_removes_backend_and_hash() {
    let user = create_test_user("laura", "L@uraP@ss!").await;
    let mut request = make_request();

    login_to_session(&mut request, &user);
    assert!(get_backend_from_meta(&request).is_some());

    logout_from_session(&mut request);
    assert!(get_backend_from_meta(&request).is_none());
}

#[test]
fn logout_with_no_active_session_is_safe() {
    let mut request = make_request();

    // Should not panic when no user is logged in
    logout_from_session(&mut request);
    assert!(!is_authenticated(&request));
}

#[tokio::test]
async fn logout_preserves_non_auth_session_data() {
    let user = create_test_user("mike", "M!keP@ss!").await;
    let mut request = HttpRequest::builder()
        .meta("SESSION_KEY", "test-session-key")
        .meta("SESSION_DATA", r#"{"cart":["item1","item2"]}"#)
        .meta("SESSION_MODIFIED", "false")
        .meta("SESSION_IS_NEW", "false")
        .build();

    login_to_session(&mut request, &user);
    logout_from_session(&mut request);

    // Auth data gone
    assert!(get_user_id_from_meta(&request).is_none());

    // Cart data preserved
    let session_data_str = request.meta().get("SESSION_DATA").unwrap();
    let data: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(session_data_str).unwrap();
    assert!(data.contains_key("cart"));
}

// ═══════════════════════════════════════════════════════════════════════
// 4. PERMISSION ENFORCEMENT (~10 tests)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn login_required_redirect_anonymous_user_gets_redirect() {
    let view = login_required_redirect("/accounts/login/", "next", make_view());
    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .path("/protected/page/")
        .build();

    let response = view(request).await;
    assert_eq!(response.status(), http::StatusCode::FOUND);

    let location = response
        .headers()
        .get(http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/accounts/login/"));
    assert!(location.contains("next=/protected/page/"));
}

#[tokio::test]
async fn login_required_redirect_authenticated_user_passes() {
    let view = login_required_redirect("/accounts/login/", "next", make_view());
    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .meta("USER_AUTHENTICATED", "true")
        .build();

    let response = view(request).await;
    assert_eq!(response.status(), http::StatusCode::OK);
}

#[tokio::test]
async fn permission_required_user_with_permission_passes() {
    let view = permission_required("blog.add_post", "/accounts/login/", make_view());
    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .meta("USER_AUTHENTICATED", "true")
        .meta("USER_PERMISSIONS", "blog.add_post,blog.change_post")
        .build();

    let response = view(request).await;
    assert_eq!(response.status(), http::StatusCode::OK);
}

#[tokio::test]
async fn permission_required_user_without_permission_gets_403() {
    let view = permission_required("blog.delete_post", "/accounts/login/", make_view());
    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .meta("USER_AUTHENTICATED", "true")
        .meta("USER_PERMISSIONS", "blog.add_post")
        .build();

    let response = view(request).await;
    assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn superuser_bypasses_all_permission_checks() {
    let view = permission_required("any.permission", "/accounts/login/", make_view());
    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .meta("USER_AUTHENTICATED", "true")
        .meta("USER_IS_SUPERUSER", "true")
        .build();

    let response = view(request).await;
    assert_eq!(response.status(), http::StatusCode::OK);
}

#[test]
fn superuser_has_perm_always_true() {
    let mut user = AbstractUser::new("admin");
    user.is_superuser = true;

    assert!(has_perm(&user, "any.perm"));
    assert!(has_perm(&user, "blog.delete_post"));
    assert!(has_perm(&user, "totally.made_up"));
}

#[test]
fn inactive_superuser_has_no_permissions() {
    let mut user = AbstractUser::new("admin");
    user.is_superuser = true;
    user.base.is_active = false;

    assert!(!has_perm(&user, "any.perm"));
}

#[test]
fn group_permissions_inherited_by_members() {
    let mut user = AbstractUser::new("editor");
    user.groups = vec!["editors".to_string()];

    let mut editors = Group::new("editors");
    editors.add_permission(Permission::new(
        "change_post",
        "Can change post",
        "blog.post",
    ));
    editors.add_permission(Permission::new("add_post", "Can add post", "blog.post"));

    assert!(has_perm_with_groups(
        &user,
        "blog.post.change_post",
        &[editors.clone()]
    ));
    assert!(has_perm_with_groups(
        &user,
        "blog.post.add_post",
        &[editors]
    ));
}

#[test]
fn non_member_does_not_inherit_group_permissions() {
    let user = AbstractUser::new("outsider");

    let mut editors = Group::new("editors");
    editors.add_permission(Permission::new(
        "change_post",
        "Can change post",
        "blog.post",
    ));

    assert!(!has_perm_with_groups(
        &user,
        "blog.post.change_post",
        &[editors]
    ));
}

#[tokio::test]
async fn permission_required_unauthenticated_redirects_to_login() {
    let view = permission_required("blog.add_post", "/accounts/login/", make_view());
    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .path("/blog/create/")
        .build();

    let response = view(request).await;
    assert_eq!(response.status(), http::StatusCode::FOUND);

    let location = response
        .headers()
        .get(http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/accounts/login/"));
    assert!(location.contains("next=/blog/create/"));
}

// ═══════════════════════════════════════════════════════════════════════
// 5. CSRF PROTECTION (~8 tests)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn csrf_generate_token_not_empty() {
    let token = generate_csrf_token();
    assert!(!token.is_empty());
    assert_eq!(token.len(), 64); // 32 bytes = 64 hex chars
}

#[test]
fn csrf_validate_correct_token_succeeds() {
    let token = generate_csrf_token();
    assert!(validate_csrf_token(&token, &token));
}

#[test]
fn csrf_validate_wrong_token_fails() {
    let token1 = generate_csrf_token();
    let token2 = generate_csrf_token();
    assert!(!validate_csrf_token(&token1, &token2));
}

#[test]
fn csrf_validate_empty_token_fails() {
    assert!(!validate_csrf_token("", ""));
    assert!(!validate_csrf_token("token", ""));
    assert!(!validate_csrf_token("", "token"));
}

#[test]
fn csrf_token_masking_produces_different_value() {
    let token = generate_csrf_token();
    let masked = mask_csrf_token(&token);

    assert_ne!(masked, token);
    assert_eq!(masked.len(), token.len() * 2); // mask + masked_token
}

#[test]
fn csrf_unmask_recovers_original() {
    let token = generate_csrf_token();
    let masked = mask_csrf_token(&token);
    let unmasked = unmask_csrf_token(&masked);

    assert_eq!(unmasked, token);
}

#[test]
fn csrf_validate_masked_token_against_cookie() {
    let token = generate_csrf_token();
    let masked = mask_csrf_token(&token);
    assert!(validate_csrf_token(&masked, &token));
}

#[tokio::test]
async fn csrf_middleware_blocks_post_without_token() {
    let mw = CsrfMiddleware::new();
    let mut request = HttpRequest::builder()
        .method(http::Method::POST)
        .path("/submit/")
        .build();

    let result = mw.process_request(&mut request).await;
    assert!(result.is_some());
    assert_eq!(result.unwrap().status(), http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn csrf_middleware_allows_get_requests() {
    let mw = CsrfMiddleware::new();
    let mut request = HttpRequest::builder()
        .method(http::Method::GET)
        .path("/page/")
        .build();

    let result = mw.process_request(&mut request).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn csrf_middleware_allows_post_with_valid_header_token() {
    let mw = CsrfMiddleware::new();
    let token = generate_csrf_token();
    let mut request = HttpRequest::builder()
        .method(http::Method::POST)
        .path("/submit/")
        .header("cookie", &format!("csrftoken={token}"))
        .header("x-csrftoken", &token)
        .build();

    let result = mw.process_request(&mut request).await;
    assert!(
        result.is_none(),
        "POST with valid CSRF token should be allowed"
    );
}

#[tokio::test]
async fn csrf_middleware_allows_post_with_masked_token() {
    let mw = CsrfMiddleware::new();
    let token = generate_csrf_token();
    let masked = mask_csrf_token(&token);
    let mut request = HttpRequest::builder()
        .method(http::Method::POST)
        .path("/submit/")
        .header("cookie", &format!("csrftoken={token}"))
        .header("x-csrftoken", &masked)
        .build();

    let result = mw.process_request(&mut request).await;
    assert!(
        result.is_none(),
        "POST with masked CSRF token should be allowed"
    );
}

#[tokio::test]
async fn csrf_middleware_blocks_post_with_wrong_token() {
    let mw = CsrfMiddleware::new();
    let cookie_token = generate_csrf_token();
    let wrong_token = generate_csrf_token();
    let mut request = HttpRequest::builder()
        .method(http::Method::POST)
        .path("/submit/")
        .header("cookie", &format!("csrftoken={cookie_token}"))
        .header("x-csrftoken", &wrong_token)
        .build();

    let result = mw.process_request(&mut request).await;
    assert!(result.is_some());
    assert_eq!(result.unwrap().status(), http::StatusCode::FORBIDDEN);
}

// ═══════════════════════════════════════════════════════════════════════
// 6. PASSWORD HASHING (~5 tests)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn hash_and_verify_with_pbkdf2() {
    let hasher = Pbkdf2Hasher { iterations: 1000 };
    let hash = hasher.hash("my_pbkdf2_password").await.unwrap();
    assert!(hash.starts_with("pbkdf2_sha256$"));
    assert!(hasher.verify("my_pbkdf2_password", &hash).await.unwrap());
}

#[tokio::test]
async fn hash_and_verify_with_argon2() {
    let hasher = Argon2Hasher;
    let hash = hasher.hash("my_argon2_password").await.unwrap();
    assert!(hash.starts_with("$argon2"));
    assert!(hasher.verify("my_argon2_password", &hash).await.unwrap());
}

#[tokio::test]
async fn hash_and_verify_with_bcrypt() {
    let hasher = BcryptHasher { cost: 4 }; // Low cost for test speed
    let hash = hasher.hash("my_bcrypt_password").await.unwrap();
    assert!(hash.starts_with("$2b$"));
    assert!(hasher.verify("my_bcrypt_password", &hash).await.unwrap());
}

#[tokio::test]
async fn wrong_password_verification_fails_all_hashers() {
    let argon2_hasher = Argon2Hasher;
    let bcrypt_hasher = BcryptHasher { cost: 4 };
    let pbkdf2_hasher = Pbkdf2Hasher { iterations: 1000 };

    let argon2_hash = argon2_hasher.hash("correct").await.unwrap();
    let bcrypt_hash = bcrypt_hasher.hash("correct").await.unwrap();
    let pbkdf2_hash = pbkdf2_hasher.hash("correct").await.unwrap();

    assert!(!argon2_hasher.verify("wrong", &argon2_hash).await.unwrap());
    assert!(!bcrypt_hasher.verify("wrong", &bcrypt_hash).await.unwrap());
    assert!(!pbkdf2_hasher.verify("wrong", &pbkdf2_hash).await.unwrap());
}

#[tokio::test]
async fn different_hashers_produce_different_hash_formats() {
    let argon2_hash = Argon2Hasher.hash("same_password").await.unwrap();
    let bcrypt_hash = BcryptHasher { cost: 4 }
        .hash("same_password")
        .await
        .unwrap();
    let pbkdf2_hash = Pbkdf2Hasher { iterations: 1000 }
        .hash("same_password")
        .await
        .unwrap();

    // All hashes should be different (different algorithms)
    assert_ne!(argon2_hash, bcrypt_hash);
    assert_ne!(bcrypt_hash, pbkdf2_hash);
    assert_ne!(argon2_hash, pbkdf2_hash);

    // Each should start with its algorithm identifier
    assert!(argon2_hash.starts_with("$argon2"));
    assert!(bcrypt_hash.starts_with("$2b$"));
    assert!(pbkdf2_hash.starts_with("pbkdf2_sha256$"));
}

#[tokio::test]
async fn check_password_auto_identifies_hasher() {
    // make_password uses Argon2 by default
    let hash = make_password("auto_detect_me").await.unwrap();
    assert!(check_password("auto_detect_me", &hash).await.unwrap());
    assert!(!check_password("wrong_password", &hash).await.unwrap());
}

// ═══════════════════════════════════════════════════════════════════════
// BONUS: Cross-cutting integration tests
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn full_login_session_persist_logout_flow() {
    // 1. Create user and backend
    let user = create_test_user("zara", "Z@raP@ss123!").await;
    let backend = ModelBackend::new();
    backend.add_user(user.clone()).await;

    // 2. Authenticate
    let creds = Credentials::with_username("zara", "Z@raP@ss123!");
    let _backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(ModelBackend::new())];
    // Need to use the backend that has the user
    let auth_backend = ModelBackend::new();
    auth_backend.add_user(user.clone()).await;
    let auth_result = authenticate(&creds, &[Box::new(auth_backend) as Box<dyn AuthBackend>])
        .await
        .unwrap();
    assert!(auth_result.is_some());

    // 3. Login to session
    let mut request = make_request();
    login_to_session(&mut request, &auth_result.unwrap());
    assert!(is_authenticated(&request));

    // 4. Simulate session persistence
    let session_data = request.meta().get("SESSION_DATA").unwrap().clone();
    let request2 = HttpRequest::builder()
        .meta("SESSION_KEY", "persisted-session")
        .meta("SESSION_DATA", &session_data)
        .meta("USER_AUTHENTICATED", "true")
        .build();
    let loaded = get_user_from_request(&request2, &backend).await;
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().username, "zara");

    // 5. Logout
    logout_from_session(&mut request);
    assert!(!is_authenticated(&request));
    assert!(get_user_id_from_meta(&request).is_none());
}

#[tokio::test]
async fn session_auth_hash_detects_password_change() {
    let user = create_test_user("mallory", "Original!P@ss").await;
    let backend = ModelBackend::new();

    // Login with original password hash in session
    let mut session = SessionData::new("test".to_string());
    login(&mut session, &user);

    // Store user with a changed password in the backend
    let changed_user = create_test_user("mallory", "Changed!P@ss").await;
    backend.add_user(changed_user).await;

    // Session should be invalidated because hash fragment differs
    let loaded = get_user_from_session(&session, &backend).await;
    assert!(
        loaded.is_none(),
        "Password change should invalidate session"
    );
}

#[tokio::test]
async fn anonymous_user_has_no_permissions() {
    let anon = AnonymousUser::new();
    assert!(!anon.is_authenticated());
    assert!(anon.is_anonymous());
    assert!(!anon.has_perm("any.perm"));
    assert!(!anon.has_perms(&["perm1", "perm2"]));
    assert!(!anon.has_module_perms("any_app"));
    assert_eq!(anon.get_username(), "");
}

#[tokio::test]
async fn login_required_decorator_blocks_then_allows() {
    let view = login_required(make_view());

    // Unauthenticated: 403
    let request1 = HttpRequest::builder().method(http::Method::GET).build();
    let response1 = view(request1).await;
    assert_eq!(response1.status(), http::StatusCode::FORBIDDEN);

    // Authenticated: 200
    let request2 = HttpRequest::builder()
        .method(http::Method::GET)
        .meta("USER_AUTHENTICATED", "true")
        .build();
    let response2 = view(request2).await;
    assert_eq!(response2.status(), http::StatusCode::OK);
}

#[tokio::test]
async fn csrf_token_via_cookie_roundtrip_in_middleware() {
    let mw = CsrfMiddleware::new();

    // Step 1: GET sets a CSRF cookie
    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .path("/form/")
        .build();
    let response = django_rs_http::HttpResponse::ok("form page");
    let response = mw.process_response(&request, response).await;

    // Extract token from Set-Cookie header
    let set_cookie = response
        .headers()
        .get(http::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("csrftoken="));

    // Parse the token value
    let token = set_cookie
        .split(';')
        .next()
        .unwrap()
        .strip_prefix("csrftoken=")
        .unwrap();

    // Step 2: POST with the extracted token should succeed
    let mut post_request = HttpRequest::builder()
        .method(http::Method::POST)
        .path("/form/")
        .header("cookie", &format!("csrftoken={token}"))
        .header("x-csrftoken", token)
        .build();

    let result = mw.process_request(&mut post_request).await;
    assert!(
        result.is_none(),
        "POST with cookie-sourced CSRF token should succeed"
    );
}

#[tokio::test]
async fn session_backend_login_logout_via_session_data() {
    let user = create_test_user("session_user", "S3ss!0nP@ss").await;

    let mut session = SessionData::new("session-key-abc".to_string());
    login(&mut session, &user);

    // Verify session has auth keys
    assert!(session.get("_auth_user_id").is_some());
    assert!(session.get("_auth_user_backend").is_some());
    assert!(session.get("_auth_user_hash").is_some());

    // Logout clears auth keys
    logout(&mut session);
    assert!(session.get("_auth_user_id").is_none());
    assert!(session.get("_auth_user_backend").is_none());
    assert!(session.get("_auth_user_hash").is_none());
}
