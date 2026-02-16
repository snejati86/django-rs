//! Security end-to-end tests for django-rs.
//!
//! These tests verify that security mechanisms work correctly when requests flow
//! through the full middleware + view pipeline:
//!   - SQL injection prevention (via parameterized queries)
//!   - XSS prevention (via template auto-escaping)
//!   - CSRF protection (via CsrfMiddleware)
//!   - Security headers (via SecurityMiddleware)

use std::collections::HashMap;
use std::sync::Arc;

use django_rs_auth::csrf::{
    generate_csrf_token, mask_csrf_token, CsrfMiddleware,
};
use django_rs_core::Settings;
use django_rs_db::value::Value;
use django_rs_db::DbExecutor;
use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{root, URLEntry};
use django_rs_http::{HttpRequest, HttpResponse};
use django_rs_template::context::{Context, ContextValue};
use django_rs_template::engine::Engine;
use django_rs_test::client::TestClient;
use django_rs_test::test_database::TestDatabase;
use django_rs_views::middleware::builtin::SecurityMiddleware;
use django_rs_views::server::DjangoApp;

type Handler = Arc<dyn Fn(HttpRequest) -> django_rs_http::BoxFuture + Send + Sync>;

// ============================================================================
// SQL Injection Prevention (~4)
// ============================================================================

/// 1. Filter value with single quotes properly escaped in SQL via parameterized queries.
#[tokio::test]
async fn test_sql_injection_single_quotes_escaped() {
    let db = TestDatabase::new();
    db.execute_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .unwrap();
    db.execute_sql(
        "INSERT INTO users (name) VALUES (?)",
        &[Value::from("Alice")],
    )
    .await
    .unwrap();
    db.execute_sql(
        "INSERT INTO users (name) VALUES (?)",
        &[Value::from("Bob")],
    )
    .await
    .unwrap();

    // Attempt SQL injection via parameterized query
    let malicious_input = "'; DROP TABLE users; --";
    let rows = db
        .query(
            "SELECT * FROM users WHERE name = ?",
            &[Value::from(malicious_input)],
        )
        .await
        .unwrap();

    // Should find no rows (not crash or drop the table)
    assert!(rows.is_empty(), "No user should match the injection string");

    // Table should still exist with data intact
    let all_rows = db.query("SELECT * FROM users", &[]).await.unwrap();
    assert_eq!(
        all_rows.len(),
        2,
        "Users table should still have 2 rows after injection attempt"
    );
}

/// 2. Filter value with semicolons doesn't execute extra SQL.
#[tokio::test]
async fn test_sql_injection_semicolons_safe() {
    let db = TestDatabase::new();
    db.execute_raw("CREATE TABLE items (id INTEGER PRIMARY KEY, title TEXT NOT NULL)")
        .await
        .unwrap();
    db.execute_sql(
        "INSERT INTO items (title) VALUES (?)",
        &[Value::from("Widget")],
    )
    .await
    .unwrap();

    // Attempt to sneak in a second statement
    let malicious_input = "Widget; DELETE FROM items";
    let rows = db
        .query(
            "SELECT * FROM items WHERE title = ?",
            &[Value::from(malicious_input)],
        )
        .await
        .unwrap();

    assert!(rows.is_empty(), "No item should match the injection string");

    // Original data should be intact
    let all_rows = db.query("SELECT * FROM items", &[]).await.unwrap();
    assert_eq!(
        all_rows.len(),
        1,
        "Items table should still have 1 row after injection attempt"
    );
}

/// 3. Raw SQL with parameters doesn't interpolate user input.
#[tokio::test]
async fn test_sql_injection_raw_parameters_safe() {
    let db = TestDatabase::new();
    db.execute_raw("CREATE TABLE posts (id INTEGER PRIMARY KEY, content TEXT NOT NULL)")
        .await
        .unwrap();
    db.execute_sql(
        "INSERT INTO posts (content) VALUES (?)",
        &[Value::from("Hello World")],
    )
    .await
    .unwrap();

    // Try UNION injection
    let malicious_input = "x' UNION SELECT id, content FROM posts WHERE '1'='1";
    let rows = db
        .query(
            "SELECT * FROM posts WHERE content = ?",
            &[Value::from(malicious_input)],
        )
        .await
        .unwrap();

    assert!(rows.is_empty(), "UNION injection should return no rows");

    // Verify original data is intact
    let all_rows = db.query("SELECT * FROM posts", &[]).await.unwrap();
    assert_eq!(all_rows.len(), 1);
}

/// 4. Admin search with SQL metacharacters is safe.
#[tokio::test]
async fn test_sql_injection_metacharacters_safe() {
    let db = TestDatabase::new();
    db.execute_raw("CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .unwrap();
    db.execute_sql(
        "INSERT INTO products (name) VALUES (?)",
        &[Value::from("Apple")],
    )
    .await
    .unwrap();

    // Various SQL metacharacters that should all be safe with parameterized queries
    let dangerous_inputs = vec![
        "1 OR 1=1",
        "1; DROP TABLE products",
        "' OR '1'='1",
        "1 UNION SELECT * FROM products",
        "Robert'); DROP TABLE products;--",
        "%",
        "_",
    ];

    for input in dangerous_inputs {
        let rows = db
            .query(
                "SELECT * FROM products WHERE name = ?",
                &[Value::from(input)],
            )
            .await
            .unwrap();

        assert!(
            rows.is_empty(),
            "Metacharacter input '{input}' should not match any row"
        );
    }

    // Table still exists with original data
    let all_rows = db.query("SELECT * FROM products", &[]).await.unwrap();
    assert_eq!(
        all_rows.len(),
        1,
        "Products table should be intact after all injection attempts"
    );
}

// ============================================================================
// XSS Prevention (~4)
// ============================================================================

/// 5. Template auto-escapes HTML entities in variables (< > & " ').
#[tokio::test]
async fn test_xss_auto_escapes_html_entities() {
    let engine = Engine::new();
    engine.add_string_template("test.html", "{{ content }}");

    let mut ctx = Context::new();
    ctx.set(
        "content",
        ContextValue::from(r#"<script>alert("xss")</script> & "quotes" 'apos'"#),
    );
    let result = engine.render_to_string("test.html", &mut ctx).unwrap();

    assert!(
        result.contains("&lt;script&gt;"),
        "< should be escaped: {result}"
    );
    assert!(
        result.contains("&gt;"),
        "> should be escaped: {result}"
    );
    assert!(
        result.contains("&amp;"),
        "& should be escaped: {result}"
    );
    assert!(
        result.contains("&quot;"),
        "double quotes should be escaped: {result}"
    );
    // Make sure no raw HTML
    assert!(
        !result.contains("<script>"),
        "Raw <script> should not appear: {result}"
    );
}

/// 6. json_script / verbatim filter handles script tag in data safely.
#[tokio::test]
async fn test_xss_script_tag_in_template_variable() {
    let engine = Engine::new();
    engine.add_string_template(
        "safe_output.html",
        "<div>{{ user_input }}</div>",
    );

    let mut ctx = Context::new();
    ctx.set(
        "user_input",
        ContextValue::from("</script><script>alert('xss')</script>"),
    );
    let result = engine.render_to_string("safe_output.html", &mut ctx).unwrap();

    // The </script> should be escaped
    assert!(
        !result.contains("</script><script>"),
        "Closing script tag should be escaped: {result}"
    );
    assert!(
        result.contains("&lt;/script&gt;"),
        "Script tags should be escaped: {result}"
    );
}

/// 7. Form field values are HTML-escaped in rendered output.
#[tokio::test]
async fn test_xss_form_field_values_escaped() {
    let engine = Engine::new();
    engine.add_string_template(
        "form.html",
        r#"<input type="text" value="{{ field_value }}">"#,
    );

    let mut ctx = Context::new();
    ctx.set(
        "field_value",
        ContextValue::from(r#"" onfocus="alert('xss')"#),
    );
    let result = engine.render_to_string("form.html", &mut ctx).unwrap();

    assert!(
        result.contains("&quot;"),
        "Double quotes in value should be escaped: {result}"
    );
    assert!(
        !result.contains(r#"onfocus="alert"#),
        "Event handler should not be injected: {result}"
    );
}

/// 8. User-provided content rendered via template is escaped even through full pipeline.
#[tokio::test]
async fn test_xss_full_pipeline_escaping() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "comment.html",
        "<p class=\"comment\">{{ comment_text }}</p>",
    );

    let engine_clone = engine.clone();
    let handler: Handler = Arc::new(move |req: HttpRequest| {
        let eng = engine_clone.clone();
        Box::pin(async move {
            // Simulate reading user input from query parameter
            let comment = req.get().get("text").unwrap_or("").to_string();
            let mut ctx = Context::new();
            ctx.set("comment_text", ContextValue::from(comment.as_str()));
            match eng.render_to_string("comment.html", &mut ctx) {
                Ok(html) => HttpResponse::ok(html),
                Err(e) => HttpResponse::server_error(format!("Error: {e}")),
            }
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("comment/", handler, Some("comment")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client
        .get("/comment/?text=%3Cimg+src%3Dx+onerror%3Dalert(1)%3E")
        .await;
    assert_eq!(response.status_code(), 200);
    let body = response.text();
    assert!(
        !body.contains("<img src=x onerror=alert(1)>"),
        "XSS payload should be escaped, got: {body}"
    );
    assert!(
        body.contains("&lt;img"),
        "Should be HTML-escaped: {body}"
    );
}

// ============================================================================
// CSRF Protection (~4)
// ============================================================================

/// 9. POST without CSRF token gets rejected (403).
#[tokio::test]
async fn test_csrf_post_without_token_rejected() {
    let csrf_mw = CsrfMiddleware::new();
    let handler: Handler = Arc::new(|_req| Box::pin(async { HttpResponse::ok("success") }));

    let patterns = vec![URLEntry::Pattern(
        path("submit/", handler, Some("submit")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default())
        .urls(resolver)
        .middleware(csrf_mw);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let mut data = HashMap::new();
    data.insert("field".to_string(), "value".to_string());
    let response = client.post("/submit/", &data).await;

    assert_eq!(
        response.status_code(),
        403,
        "POST without CSRF token should be rejected"
    );
}

/// 10. POST with valid CSRF token succeeds.
#[tokio::test]
async fn test_csrf_post_with_valid_token_succeeds() {
    let csrf_mw = CsrfMiddleware::new();
    let handler: Handler = Arc::new(|_req| Box::pin(async { HttpResponse::ok("csrf_ok") }));

    let patterns = vec![URLEntry::Pattern(
        path("submit/", handler, Some("submit")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default())
        .urls(resolver)
        .middleware(csrf_mw);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    // First, do a GET to get the CSRF cookie
    let get_response = client.get("/submit/").await;
    assert_eq!(get_response.status_code(), 200);

    // Extract the CSRF token from the cookie
    let csrf_token = client
        .cookies()
        .get("csrftoken")
        .cloned()
        .expect("Should have csrftoken cookie after GET");

    // Now POST with the CSRF token in a cookie + header
    client.set_cookie("csrftoken", &csrf_token);
    // We need to manually construct the request with X-CSRFToken header.
    // Since TestClient.post() doesn't support custom headers, we simulate
    // by putting the token in the form data as csrfmiddlewaretoken.
    let mut data = HashMap::new();
    data.insert("field".to_string(), "value".to_string());
    data.insert("csrfmiddlewaretoken".to_string(), csrf_token);
    let response = client.post("/submit/", &data).await;

    assert_eq!(
        response.status_code(),
        200,
        "POST with valid CSRF token should succeed, got: {}",
        response.text()
    );
    assert!(response.text().contains("csrf_ok"));
}

/// 11. CSRF token is masked (not raw secret) -- masked tokens are double length.
#[tokio::test]
async fn test_csrf_token_is_masked() {
    let raw_token = generate_csrf_token();
    assert_eq!(raw_token.len(), 64, "Raw token should be 64 hex chars");

    let masked = mask_csrf_token(&raw_token);
    assert_eq!(
        masked.len(),
        128,
        "Masked token should be 128 hex chars (double the raw token)"
    );

    // Masked token should be different from raw token
    assert_ne!(masked, raw_token, "Masked token should differ from raw");

    // Each masking should produce a different result
    let masked2 = mask_csrf_token(&raw_token);
    assert_ne!(masked, masked2, "Different maskings should produce different values");
}

/// 12. GET/HEAD/OPTIONS requests bypass CSRF check.
#[tokio::test]
async fn test_csrf_safe_methods_bypass_check() {
    let csrf_mw = CsrfMiddleware::new();
    let handler: Handler = Arc::new(|_req| Box::pin(async { HttpResponse::ok("safe_method_ok") }));

    let patterns = vec![URLEntry::Pattern(
        path("endpoint/", handler, Some("endpoint")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default())
        .urls(resolver)
        .middleware(csrf_mw);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    // GET should pass
    let response = client.get("/endpoint/").await;
    assert_eq!(response.status_code(), 200, "GET should bypass CSRF");

    // HEAD should pass
    let response = client.head("/endpoint/").await;
    assert_eq!(response.status_code(), 200, "HEAD should bypass CSRF");

    // OPTIONS should pass
    let response = client.options("/endpoint/").await;
    assert_eq!(response.status_code(), 200, "OPTIONS should bypass CSRF");
}

// ============================================================================
// Security Headers (~3)
// ============================================================================

/// 13. Response includes X-Content-Type-Options: nosniff.
#[tokio::test]
async fn test_security_header_x_content_type_options() {
    let handler: Handler = Arc::new(|_req| Box::pin(async { HttpResponse::ok("secured") }));

    let patterns = vec![URLEntry::Pattern(
        path("secure/", handler, Some("secure")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default())
        .urls(resolver)
        .middleware(SecurityMiddleware::default());
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/secure/").await;
    assert_eq!(response.status_code(), 200);
    assert_eq!(
        response.header("x-content-type-options"),
        Some("nosniff"),
        "Should have X-Content-Type-Options: nosniff"
    );
}

/// 14. Response includes X-Frame-Options.
#[tokio::test]
async fn test_security_header_x_frame_options() {
    let handler: Handler = Arc::new(|_req| Box::pin(async { HttpResponse::ok("framed") }));

    let patterns = vec![URLEntry::Pattern(
        path("frame/", handler, Some("frame")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default())
        .urls(resolver)
        .middleware(SecurityMiddleware::default());
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/frame/").await;
    assert_eq!(response.status_code(), 200);
    assert_eq!(
        response.header("x-frame-options"),
        Some("DENY"),
        "Should have X-Frame-Options: DENY"
    );
}

/// 15. SecurityMiddleware sets configurable X-Frame-Options (e.g., SAMEORIGIN).
#[tokio::test]
async fn test_security_header_x_frame_options_sameorigin() {
    let handler: Handler = Arc::new(|_req| Box::pin(async { HttpResponse::ok("framed") }));

    let patterns = vec![URLEntry::Pattern(
        path("frame/", handler, Some("frame")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let security_mw = SecurityMiddleware {
        x_frame_options: "SAMEORIGIN".to_string(),
        ..SecurityMiddleware::default()
    };

    let app = DjangoApp::new(Settings::default())
        .urls(resolver)
        .middleware(security_mw);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/frame/").await;
    assert_eq!(response.status_code(), 200);
    assert_eq!(
        response.header("x-frame-options"),
        Some("SAMEORIGIN"),
        "Should have X-Frame-Options: SAMEORIGIN"
    );
}
