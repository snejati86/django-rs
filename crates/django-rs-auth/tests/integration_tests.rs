//! Integration tests for the auth pipeline.
//!
//! Tests the full flow: register -> login -> access protected page -> logout -> redirected.

use django_rs_auth::backends::{AuthBackend, Credentials, ModelBackend};
use django_rs_auth::forms::{AuthenticationForm, UserCreationForm};
use django_rs_auth::session_auth;
use django_rs_auth::user::AbstractUser;
use django_rs_auth::views::{LoginConfig, LogoutConfig};
use django_rs_http::{HttpRequest, QueryDict};
use django_rs_views::session::SessionData;

async fn create_test_user(username: &str, password: &str) -> AbstractUser {
    let mut user = AbstractUser::new(username);
    user.set_password(password).await.unwrap();
    user
}

fn make_request_with_session() -> HttpRequest {
    HttpRequest::builder()
        .meta("SESSION_KEY", "integration-test-session")
        .meta("SESSION_DATA", "{}")
        .meta("SESSION_MODIFIED", "false")
        .meta("SESSION_IS_NEW", "true")
        .build()
}

// ── Full Registration -> Login -> Protected -> Logout Flow ──────────

#[tokio::test]
async fn test_full_registration_login_logout_flow() {
    // Step 1: Register a user via UserCreationForm
    let mut reg_form = UserCreationForm::new();
    let data = QueryDict::parse("username=testuser&password1=Str0ngP@ss!&password2=Str0ngP@ss!");
    reg_form.bind(&data);
    assert!(reg_form.is_valid().await, "Registration form should be valid");

    let username = reg_form.get_username().unwrap();
    let password = reg_form.get_password().unwrap();

    // Create the user (simulating what the view would do)
    let mut user = AbstractUser::new(&username);
    user.set_password(&password).await.unwrap();

    // Step 2: Add user to backend
    let backend = ModelBackend::new();
    backend.add_user(user.clone()).await;

    // Step 3: Authenticate via login form
    let mut login_form = AuthenticationForm::new();
    let login_data = QueryDict::parse("username=testuser&password=Str0ngP@ss!");
    login_form.bind(&login_data);
    assert!(login_form.is_valid().await, "Login form should be valid");

    let creds = Credentials::with_username(
        &login_form.get_username().unwrap(),
        &login_form.get_password().unwrap(),
    );
    let _backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(ModelBackend::new())];
    // Note: we need the user in the backend we query
    let auth_backend = ModelBackend::new();
    auth_backend.add_user(user.clone()).await;
    let auth_result = django_rs_auth::authenticate(
        &creds,
        &[Box::new(auth_backend) as Box<dyn AuthBackend>],
    )
    .await
    .unwrap();
    assert!(auth_result.is_some(), "Authentication should succeed");

    // Step 4: Login to session
    let mut request = make_request_with_session();
    session_auth::login_to_session(&mut request, &auth_result.unwrap());
    assert!(session_auth::is_authenticated(&request));

    // Step 5: Access a protected page (simulated by checking auth)
    assert!(session_auth::is_authenticated(&request));
    let user_id = session_auth::get_user_id_from_meta(&request);
    assert_eq!(user_id.as_deref(), Some("testuser"));

    // Step 6: Logout
    session_auth::logout_from_session(&mut request);
    assert!(!session_auth::is_authenticated(&request));
    assert!(session_auth::get_user_id_from_meta(&request).is_none());
}

#[tokio::test]
async fn test_full_flow_with_login_view() {
    // Create user and backend
    let user = create_test_user("alice", "MyStr0ng!Pass").await;
    let backend = ModelBackend::new();
    backend.add_user(user).await;

    let config = LoginConfig::default();
    let backends: Vec<Box<dyn AuthBackend>> = vec![Box::new(backend)];

    // Step 1: GET login page
    let get_request = HttpRequest::builder()
        .method(http::Method::GET)
        .path("/accounts/login/")
        .build();

    let response = django_rs_auth::views::login_view(get_request, &config, &backends).await;
    assert_eq!(response.status(), http::StatusCode::OK);

    // Step 2: POST valid credentials
    let post_request = HttpRequest::builder()
        .method(http::Method::POST)
        .path("/accounts/login/")
        .content_type("application/x-www-form-urlencoded")
        .body(b"username=alice&password=MyStr0ng!Pass".to_vec())
        .meta("SESSION_DATA", "{}")
        .meta("SESSION_KEY", "test-key")
        .build();

    let response = django_rs_auth::views::login_view(post_request, &config, &backends).await;
    assert_eq!(response.status(), http::StatusCode::FOUND);

    // Step 3: Logout
    let logout_config = LogoutConfig::default();
    let logout_request = HttpRequest::builder()
        .method(http::Method::POST)
        .path("/accounts/logout/")
        .meta("SESSION_DATA", "{}")
        .meta("SESSION_KEY", "test-key")
        .meta("USER_AUTHENTICATED", "true")
        .build();

    let response = django_rs_auth::views::logout_view(logout_request, &logout_config).await;
    assert_eq!(response.status(), http::StatusCode::FOUND);
}

#[tokio::test]
async fn test_session_persists_across_requests() {
    // Simulate session persistence across request boundaries
    let user = create_test_user("bob", "B0bStr0ng!").await;
    let backend = ModelBackend::new();
    backend.add_user(user.clone()).await;

    // Request 1: Login
    let mut request1 = make_request_with_session();
    session_auth::login_to_session(&mut request1, &user);

    // Extract session data from request1 (simulating session middleware persistence)
    let session_data = request1.meta().get("SESSION_DATA").unwrap().clone();

    // Request 2: Use stored session data
    let request2 = HttpRequest::builder()
        .meta("SESSION_KEY", "same-session")
        .meta("SESSION_DATA", &session_data)
        .meta("USER_AUTHENTICATED", "true")
        .build();

    // Should be able to recover the user from the session
    let loaded = session_auth::get_user_from_request(&request2, &backend).await;
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().username, "bob");
}

// ── Session Data Integration Tests ──────────────────────────────────

#[tokio::test]
async fn test_session_data_login_logout_via_session_backend() {
    let user = create_test_user("carol", "Car0lP@ss!").await;

    // Use SessionData directly (as the session middleware would)
    let mut session = SessionData::new("session-key-123".to_string());
    django_rs_auth::login(&mut session, &user);

    // Verify session has auth keys
    assert!(session.get("_auth_user_id").is_some());
    assert!(session.get("_auth_user_backend").is_some());
    assert!(session.get("_auth_user_hash").is_some());

    // Logout clears auth keys
    django_rs_auth::logout(&mut session);
    assert!(session.get("_auth_user_id").is_none());
    assert!(session.get("_auth_user_backend").is_none());
    assert!(session.get("_auth_user_hash").is_none());
}

#[tokio::test]
async fn test_get_user_from_session_via_backend() {
    let user = create_test_user("dave", "DaveP@ss123!").await;
    let backend = ModelBackend::new();
    backend.add_user(user.clone()).await;

    let mut session = SessionData::new("test-session".to_string());
    django_rs_auth::login(&mut session, &user);

    let loaded = session_auth::get_user_from_session(&session, &backend).await;
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().username, "dave");
}
