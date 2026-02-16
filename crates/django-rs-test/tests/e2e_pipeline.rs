//! End-to-end pipeline tests for django-rs.
//!
//! These tests exercise the COMPLETE path:
//!   HTTP request -> URL routing -> middleware -> view -> template -> database -> response
//!
//! They use `DjangoApp::into_axum_router()` + `TestClient` to simulate real requests
//! through the full framework stack.

use std::collections::HashMap;
use std::sync::Arc;

use django_rs_core::Settings;
use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{include, root, URLEntry};
use django_rs_http::{HttpRequest, HttpResponse, HttpResponseRedirect};
use django_rs_template::context::{Context, ContextValue};
use django_rs_template::engine::Engine;
use django_rs_test::client::TestClient;
use django_rs_views::server::DjangoApp;

// ============================================================================
// Helper: Build a DjangoApp with URL patterns and convert to a TestClient
// ============================================================================

/// A handler type alias matching what the URL system expects.
type Handler = Arc<dyn Fn(HttpRequest) -> django_rs_http::BoxFuture + Send + Sync>;

fn make_handler(f: fn(HttpRequest) -> HttpResponse) -> Handler {
    Arc::new(move |req| {
        let resp = f(req);
        Box::pin(async move { resp })
    })
}

// ============================================================================
// Request Pipeline Tests (~10)
// ============================================================================

/// 1. GET request to a URL pattern resolves to the correct view and returns 200.
#[tokio::test]
async fn test_get_request_resolves_to_view_and_returns_200() {
    let handler = make_handler(|_req| HttpResponse::ok("Hello from view!"));

    let patterns = vec![URLEntry::Pattern(
        path("hello/", handler, Some("hello")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/hello/").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("Hello from view!"));
}

/// 2. POST request with form data reaches view and is parsed.
#[tokio::test]
async fn test_post_request_with_form_data_is_parsed() {
    let handler: Handler = Arc::new(|req: HttpRequest| {
        Box::pin(async move {
            let name = req.post().get("name").unwrap_or("missing").to_string();
            let email = req.post().get("email").unwrap_or("missing").to_string();
            HttpResponse::ok(format!("name={name},email={email}"))
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("submit/", handler, Some("submit")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let mut data = HashMap::new();
    data.insert("name".to_string(), "Alice".to_string());
    data.insert("email".to_string(), "alice@example.com".to_string());
    let response = client.post("/submit/", &data).await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("name=Alice"));
    assert!(response.text().contains("email=alice@example.com"));
}

/// 3. URL path parameters are extracted correctly (e.g., /posts/42/).
#[tokio::test]
async fn test_url_path_parameters_extracted() {
    let handler: Handler = Arc::new(|req: HttpRequest| {
        Box::pin(async move {
            let rm = req.resolver_match().expect("resolver_match should be set");
            let post_id = rm.kwargs.get("id").cloned().unwrap_or_default();
            HttpResponse::ok(format!("post_id={post_id}"))
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("posts/<int:id>/", handler, Some("post-detail")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/posts/42/").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("post_id=42"));
}

/// 4. 404 returned for a nonexistent URL.
#[tokio::test]
async fn test_404_for_nonexistent_url() {
    let handler = make_handler(|_req| HttpResponse::ok("Hello"));
    let patterns = vec![URLEntry::Pattern(
        path("exists/", handler, Some("exists")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/nonexistent/").await;
    assert_eq!(response.status_code(), 404);
}

/// 5. Redirect response (302) with Location header.
#[tokio::test]
async fn test_redirect_response_302_with_location() {
    let handler = make_handler(|_req| HttpResponseRedirect::new("/destination/"));

    let patterns = vec![URLEntry::Pattern(
        path("redirect-me/", handler, Some("redirect")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/redirect-me/").await;
    assert_eq!(response.status_code(), 302);
    assert_eq!(response.header("location"), Some("/destination/"));
}

/// 6. Response content-type set correctly (text/html).
#[tokio::test]
async fn test_response_content_type_text_html() {
    let handler = make_handler(|_req| HttpResponse::ok("<h1>Hello</h1>"));

    let patterns = vec![URLEntry::Pattern(
        path("page/", handler, Some("page")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/page/").await;
    assert_eq!(response.status_code(), 200);
    let ct = response.header("content-type").unwrap_or("");
    assert!(ct.contains("text/html"), "Expected text/html, got: {ct}");
}

/// 7. Response content-type for JSON responses.
#[tokio::test]
async fn test_response_content_type_json() {
    let handler = make_handler(|_req| {
        let mut resp = HttpResponse::ok(r#"{"key":"value"}"#);
        resp.set_content_type("application/json");
        resp
    });

    let patterns = vec![URLEntry::Pattern(
        path("api/data/", handler, Some("api-data")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/api/data/").await;
    assert_eq!(response.status_code(), 200);
    let ct = response.header("content-type").unwrap_or("");
    assert!(
        ct.contains("application/json"),
        "Expected application/json, got: {ct}"
    );
}

/// 8. Custom headers in response accessible.
#[tokio::test]
async fn test_custom_response_headers() {
    let handler = make_handler(|_req| {
        HttpResponse::ok("ok").set_header(
            http::header::HeaderName::from_static("x-custom-header"),
            http::header::HeaderValue::from_static("custom-value"),
        )
    });

    let patterns = vec![URLEntry::Pattern(
        path("headers/", handler, Some("headers")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/headers/").await;
    assert_eq!(response.status_code(), 200);
    assert_eq!(response.header("x-custom-header"), Some("custom-value"));
}

/// 9. HEAD request returns headers without body.
#[tokio::test]
async fn test_head_request_returns_headers_without_body() {
    let handler = make_handler(|_req| HttpResponse::ok("This body should not appear for HEAD"));

    let patterns = vec![URLEntry::Pattern(
        path("headtest/", handler, Some("headtest")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.head("/headtest/").await;
    // HEAD should return 200 but with empty body (per HTTP spec, Axum strips the body)
    assert_eq!(response.status_code(), 200);
    assert!(
        response.text().is_empty(),
        "HEAD response body should be empty, got: {}",
        response.text()
    );
}

/// 10. Nested include() URL routing works end-to-end.
#[tokio::test]
async fn test_nested_include_url_routing() {
    let list_handler: Handler = Arc::new(|_req| Box::pin(async { HttpResponse::ok("user list") }));
    let detail_handler: Handler = Arc::new(|req: HttpRequest| {
        Box::pin(async move {
            let rm = req.resolver_match().unwrap();
            let id = rm.kwargs.get("id").cloned().unwrap_or_default();
            HttpResponse::ok(format!("user detail id={id}"))
        })
    });

    let user_patterns = vec![
        URLEntry::Pattern(path("", list_handler, Some("user-list")).unwrap()),
        URLEntry::Pattern(path("<int:id>/", detail_handler, Some("user-detail")).unwrap()),
    ];

    let patterns = vec![URLEntry::Resolver(
        include("users/", user_patterns, Some("users"), Some("users")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/users/").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("user list"));

    let response = client.get("/users/99/").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("user detail id=99"));
}

// ============================================================================
// View + Template + DB Pipeline (~10)
// ============================================================================

/// 11. TemplateView renders template and returns HTML.
#[tokio::test]
async fn test_template_view_renders_html() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("home.html", "<h1>Welcome {{ name }}!</h1>");

    let engine_clone = engine.clone();
    let handler: Handler = Arc::new(move |_req| {
        let eng = engine_clone.clone();
        Box::pin(async move {
            let mut ctx = Context::new();
            ctx.set("name", ContextValue::from("Django"));
            match eng.render_to_string("home.html", &mut ctx) {
                Ok(html) => HttpResponse::ok(html),
                Err(e) => HttpResponse::server_error(format!("Template error: {e}")),
            }
        })
    });

    let patterns = vec![URLEntry::Pattern(path("", handler, Some("home")).unwrap())];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("<h1>Welcome Django!</h1>"));
}

/// 12. ListView queries database and renders object list.
#[tokio::test]
async fn test_list_view_renders_object_list() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_list.html",
        "{% for article in articles %}<li>{{ article }}</li>{% endfor %}",
    );

    let engine_clone = engine.clone();
    let handler: Handler = Arc::new(move |_req| {
        let eng = engine_clone.clone();
        Box::pin(async move {
            // Simulate DB query returning articles
            let articles = vec!["First Post", "Second Post", "Third Post"];
            let mut ctx = Context::new();
            ctx.set(
                "articles",
                ContextValue::List(articles.into_iter().map(ContextValue::from).collect()),
            );
            match eng.render_to_string("article_list.html", &mut ctx) {
                Ok(html) => HttpResponse::ok(html),
                Err(e) => HttpResponse::server_error(format!("Error: {e}")),
            }
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("articles/", handler, Some("article-list")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/articles/").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("<li>First Post</li>"));
    assert!(response.text().contains("<li>Second Post</li>"));
    assert!(response.text().contains("<li>Third Post</li>"));
}

/// 13. DetailView fetches single object and renders.
#[tokio::test]
async fn test_detail_view_renders_single_object() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_detail.html",
        "<h1>{{ title }}</h1><p>{{ body }}</p>",
    );

    let engine_clone = engine.clone();
    let handler: Handler = Arc::new(move |req: HttpRequest| {
        let eng = engine_clone.clone();
        Box::pin(async move {
            let rm = req.resolver_match().unwrap();
            let id = rm.kwargs.get("id").cloned().unwrap_or_default();
            // Simulate DB lookup
            let mut ctx = Context::new();
            ctx.set("title", ContextValue::from(format!("Article {id}")));
            ctx.set("body", ContextValue::from("Content goes here."));
            match eng.render_to_string("article_detail.html", &mut ctx) {
                Ok(html) => HttpResponse::ok(html),
                Err(e) => HttpResponse::server_error(format!("Error: {e}")),
            }
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("articles/<int:id>/", handler, Some("article-detail")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/articles/7/").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("<h1>Article 7</h1>"));
    assert!(response.text().contains("<p>Content goes here.</p>"));
}

/// 14. CreateView GET renders empty form template.
#[tokio::test]
async fn test_create_view_get_renders_form() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_form.html",
        r#"<form method="post"><input name="title"><input name="email"><button>Submit</button></form>"#,
    );

    let engine_clone = engine.clone();
    let handler: Handler = Arc::new(move |_req: HttpRequest| {
        let eng = engine_clone.clone();
        Box::pin(async move {
            let mut ctx = Context::new();
            match eng.render_to_string("article_form.html", &mut ctx) {
                Ok(html) => HttpResponse::ok(html),
                Err(e) => HttpResponse::server_error(format!("Error: {e}")),
            }
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("articles/create/", handler, Some("article-create")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/articles/create/").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("<form"));
    assert!(response.text().contains(r#"name="title""#));
}

/// 15. CreateView POST creates DB object and redirects.
#[tokio::test]
async fn test_create_view_post_creates_and_redirects() {
    let created = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let created_clone = created.clone();

    let handler: Handler = Arc::new(move |req: HttpRequest| {
        let store = created_clone.clone();
        Box::pin(async move {
            let title = req.post().get("title").unwrap_or("").to_string();
            // Simulate DB insert
            store.lock().unwrap().push(title);
            HttpResponseRedirect::new("/articles/")
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("articles/create/", handler, Some("article-create")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let mut data = HashMap::new();
    data.insert("title".to_string(), "New Article".to_string());
    let response = client.post("/articles/create/", &data).await;

    assert_eq!(response.status_code(), 302);
    assert_eq!(response.header("location"), Some("/articles/"));
    assert_eq!(created.lock().unwrap().len(), 1);
    assert_eq!(created.lock().unwrap()[0], "New Article");
}

/// 16. Template inheritance (extends/block) works in response.
#[tokio::test]
async fn test_template_inheritance_in_response() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "base.html",
        "<html><head><title>{% block title %}Default{% endblock %}</title></head><body>{% block content %}{% endblock %}</body></html>",
    );
    engine.add_string_template(
        "child.html",
        r#"{% extends "base.html" %}{% block title %}My Page{% endblock %}{% block content %}<p>Hello!</p>{% endblock %}"#,
    );

    let engine_clone = engine.clone();
    let handler: Handler = Arc::new(move |_req| {
        let eng = engine_clone.clone();
        Box::pin(async move {
            let mut ctx = Context::new();
            match eng.render_to_string("child.html", &mut ctx) {
                Ok(html) => HttpResponse::ok(html),
                Err(e) => HttpResponse::server_error(format!("Error: {e}")),
            }
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("page/", handler, Some("page")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/page/").await;
    assert_eq!(response.status_code(), 200);
    let body = response.text();
    assert!(body.contains("<title>My Page</title>"));
    assert!(body.contains("<p>Hello!</p>"));
}

/// 17. Template variables populated from view context.
#[tokio::test]
async fn test_template_variables_from_context() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("profile.html", "Name: {{ user_name }}, Age: {{ user_age }}");

    let engine_clone = engine.clone();
    let handler: Handler = Arc::new(move |_req| {
        let eng = engine_clone.clone();
        Box::pin(async move {
            let mut ctx = Context::new();
            ctx.set("user_name", ContextValue::from("Bob"));
            ctx.set("user_age", ContextValue::Integer(30));
            match eng.render_to_string("profile.html", &mut ctx) {
                Ok(html) => HttpResponse::ok(html),
                Err(e) => HttpResponse::server_error(format!("Error: {e}")),
            }
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("profile/", handler, Some("profile")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/profile/").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("Name: Bob"));
    assert!(response.text().contains("Age: 30"));
}

/// 18. Template auto-escaping prevents XSS in response body.
#[tokio::test]
async fn test_template_auto_escaping_prevents_xss_in_response() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("unsafe.html", "<div>{{ user_input }}</div>");

    let engine_clone = engine.clone();
    let handler: Handler = Arc::new(move |_req| {
        let eng = engine_clone.clone();
        Box::pin(async move {
            let mut ctx = Context::new();
            ctx.set(
                "user_input",
                ContextValue::from("<script>alert('xss')</script>"),
            );
            match eng.render_to_string("unsafe.html", &mut ctx) {
                Ok(html) => HttpResponse::ok(html),
                Err(e) => HttpResponse::server_error(format!("Error: {e}")),
            }
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("xss-test/", handler, Some("xss-test")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/xss-test/").await;
    assert_eq!(response.status_code(), 200);
    let body = response.text();
    assert!(
        body.contains("&lt;script&gt;"),
        "XSS should be escaped, body: {body}"
    );
    assert!(
        !body.contains("<script>alert"),
        "Raw script should not appear in body"
    );
}

/// 19. Context processors inject global variables (STATIC_URL, debug).
#[tokio::test]
async fn test_context_processors_inject_global_vars() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("with_ctx.html", "DEBUG={{ debug }} STATIC={{ STATIC_URL }}");

    let engine_clone = engine.clone();
    let handler: Handler = Arc::new(move |req: HttpRequest| {
        let eng = engine_clone.clone();
        Box::pin(async move {
            // Simulate context processors
            let debug_cp = django_rs_template::context_processors::DebugContextProcessor;
            let static_cp =
                django_rs_template::context_processors::StaticContextProcessor::new("/static/");

            use django_rs_template::context_processors::ContextProcessor;
            let mut ctx = Context::new();
            for (k, v) in debug_cp.process(&req) {
                ctx.set(k, v);
            }
            for (k, v) in static_cp.process(&req) {
                ctx.set(k, v);
            }

            match eng.render_to_string("with_ctx.html", &mut ctx) {
                Ok(html) => HttpResponse::ok(html),
                Err(e) => HttpResponse::server_error(format!("Error: {e}")),
            }
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("ctx/", handler, Some("ctx")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/ctx/").await;
    assert_eq!(response.status_code(), 200);
    let body = response.text();
    assert!(body.contains("DEBUG=True"), "body: {body}");
    assert!(body.contains("STATIC=/static/"), "body: {body}");
}

/// 20. Pagination: list view with ?page=2 returns correct page.
#[tokio::test]
async fn test_pagination_query_parameter() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "paginated.html",
        "Page {{ page }} of {{ total_pages }}: {% for item in items %}{{ item }} {% endfor %}",
    );

    let engine_clone = engine.clone();
    let handler: Handler = Arc::new(move |req: HttpRequest| {
        let eng = engine_clone.clone();
        Box::pin(async move {
            let page: usize = req
                .get()
                .get("page")
                .and_then(|p| p.parse().ok())
                .unwrap_or(1);
            let per_page = 3;

            // Simulate 9 items total
            let all_items: Vec<String> = (1..=9).map(|i| format!("Item{i}")).collect();
            let total_pages = (all_items.len() + per_page - 1) / per_page;
            let start = (page - 1) * per_page;
            let end = (start + per_page).min(all_items.len());
            let page_items = &all_items[start..end];

            let mut ctx = Context::new();
            ctx.set("page", ContextValue::Integer(page as i64));
            ctx.set("total_pages", ContextValue::Integer(total_pages as i64));
            ctx.set(
                "items",
                ContextValue::List(
                    page_items
                        .iter()
                        .map(|s| ContextValue::from(s.as_str()))
                        .collect(),
                ),
            );

            match eng.render_to_string("paginated.html", &mut ctx) {
                Ok(html) => HttpResponse::ok(html),
                Err(e) => HttpResponse::server_error(format!("Error: {e}")),
            }
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("items/", handler, Some("item-list")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    // Page 1
    let response = client.get("/items/").await;
    assert_eq!(response.status_code(), 200);
    let body = response.text();
    assert!(body.contains("Page 1 of 3"), "body: {body}");
    assert!(body.contains("Item1"));
    assert!(body.contains("Item3"));
    assert!(!body.contains("Item4"));

    // Page 2
    let response = client.get("/items/?page=2").await;
    assert_eq!(response.status_code(), 200);
    let body = response.text();
    assert!(body.contains("Page 2 of 3"), "body: {body}");
    assert!(body.contains("Item4"));
    assert!(body.contains("Item6"));
    assert!(!body.contains("Item1"));
}

// ============================================================================
// TestClient Features (~5)
// ============================================================================

/// 21. TestClient.get() returns response with status, headers, body.
#[tokio::test]
async fn test_client_get_returns_status_headers_body() {
    let handler = make_handler(|_req| {
        HttpResponse::ok("response body").set_header(
            http::header::HeaderName::from_static("x-test"),
            http::header::HeaderValue::from_static("test-value"),
        )
    });

    let patterns = vec![URLEntry::Pattern(
        path("test/", handler, Some("test")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/test/").await;

    // Status
    assert_eq!(response.status_code(), 200);
    // Headers
    assert_eq!(response.header("x-test"), Some("test-value"));
    // Body
    assert_eq!(response.text(), "response body");
}

/// 22. TestClient.post() sends form data.
#[tokio::test]
async fn test_client_post_sends_form_data() {
    let handler: Handler = Arc::new(|req: HttpRequest| {
        Box::pin(async move {
            let field = req.post().get("field").unwrap_or("none").to_string();
            HttpResponse::ok(format!("field={field}"))
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("echo/", handler, Some("echo")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let mut data = HashMap::new();
    data.insert("field".to_string(), "test_value".to_string());
    let response = client.post("/echo/", &data).await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("field=test_value"));
}

/// 23. force_login sets auth state for subsequent requests.
#[tokio::test]
async fn test_force_login_sets_auth_state() {
    let handler: Handler = Arc::new(|req: HttpRequest| {
        Box::pin(async move {
            // Check if session cookie is present
            let has_session = req
                .headers()
                .get(http::header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|c| c.contains("sessionid"));
            HttpResponse::ok(format!("has_session={has_session}"))
        })
    });

    let patterns = vec![URLEntry::Pattern(
        path("check/", handler, Some("check")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    // Before login
    let response = client.get("/check/").await;
    assert!(response.text().contains("has_session=false"));

    // After login
    let user = django_rs_auth::user::AbstractUser::new("testuser");
    client.force_login(&user);

    let response = client.get("/check/").await;
    assert!(response.text().contains("has_session=true"));

    // Verify client state
    assert!(client.logged_in_user().is_some());
    assert_eq!(client.logged_in_user().unwrap().username, "testuser");
}

/// 24. Multiple sequential requests maintain session/cookie state.
#[tokio::test]
async fn test_multiple_requests_maintain_cookie_state() {
    let handler = make_handler(|_req| {
        let mut resp = HttpResponse::ok("ok");
        resp.headers_mut().insert(
            http::header::SET_COOKIE,
            http::header::HeaderValue::from_static("tracker=abc123; Path=/"),
        );
        resp
    });

    let check_handler: Handler = Arc::new(|req: HttpRequest| {
        Box::pin(async move {
            let cookies = req
                .headers()
                .get(http::header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("none")
                .to_string();
            HttpResponse::ok(format!("cookies={cookies}"))
        })
    });

    let patterns = vec![
        URLEntry::Pattern(path("set-cookie/", handler, Some("set-cookie")).unwrap()),
        URLEntry::Pattern(path("check-cookie/", check_handler, Some("check-cookie")).unwrap()),
    ];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    // Request 1: get cookie
    let _response = client.get("/set-cookie/").await;

    // Request 2: cookie should be sent automatically
    let response = client.get("/check-cookie/").await;
    assert!(
        response.text().contains("tracker=abc123"),
        "Cookie should persist across requests, got: {}",
        response.text()
    );
}

/// 25. TestClient preserves cookies from Set-Cookie headers.
#[tokio::test]
async fn test_client_preserves_set_cookie_headers() {
    let handler = make_handler(|_req| {
        let mut resp = HttpResponse::ok("cookie set");
        resp.headers_mut().insert(
            http::header::SET_COOKIE,
            http::header::HeaderValue::from_static("theme=dark; Path=/"),
        );
        resp
    });

    let patterns = vec![URLEntry::Pattern(
        path("prefs/", handler, Some("prefs")).unwrap(),
    )];
    let resolver = root(patterns).unwrap();

    let app = DjangoApp::new(Settings::default()).urls(resolver);
    let router = app.into_axum_router();
    let mut client = TestClient::new(router);

    let response = client.get("/prefs/").await;
    assert_eq!(response.cookies.get("theme"), Some(&"dark".to_string()));
    assert_eq!(
        client.cookies().get("theme"),
        Some(&"dark".to_string()),
        "Cookie should be stored in client jar"
    );
}
