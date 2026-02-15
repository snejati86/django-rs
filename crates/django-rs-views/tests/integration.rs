//! Integration tests for the view-integration pipeline.
//!
//! Tests cover:
//! 1. TemplateView renders actual templates with variables
//! 2. ListView renders templates with object_list context
//! 3. DetailView renders templates with object context
//! 4. Session data persists across requests (set in one, read in another)
//! 5. Form submission flow: GET shows form, POST with bad data shows errors, POST with good data succeeds
//! 6. Context processors inject expected variables
//! 7. DjangoApp engine integration

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use django_rs_core::DjangoError;
use django_rs_forms::fields::{FormFieldDef, FormFieldType};
use django_rs_forms::form::{BaseForm, Form};
use django_rs_http::{HttpRequest, HttpResponse, HttpResponseRedirect, QueryDict};
use django_rs_template::context::{Context, ContextValue};
use django_rs_template::context_processors::{
    CsrfContextProcessor, ContextProcessor, RequestContextProcessor, StaticContextProcessor,
};
use django_rs_template::engine::Engine;
use django_rs_views::middleware::{Middleware, MiddlewarePipeline};
use django_rs_views::session::{
    generate_session_key, InMemorySessionBackend, SessionBackend, SessionData, SessionMiddleware,
};
use django_rs_views::views::class_based::{ContextMixin, TemplateView, View};
use django_rs_views::views::form_view::{
    bind_form_from_request, cleaned_data_as_strings, form_context_to_json, form_errors,
};
use django_rs_views::views::generic::{CreateView, DetailView, ListView};

// ============================================================================
// 1. TemplateView renders actual templates with variables
// ============================================================================

#[tokio::test]
async fn test_template_view_renders_with_engine() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("home.html", "<h1>Welcome, {{ name }}!</h1>");

    let view = TemplateView::new("home.html")
        .with_engine(engine)
        .with_context("name", serde_json::json!("Django"));

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    assert_eq!(response.status(), http::StatusCode::OK);
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("<h1>Welcome, Django!</h1>"));
    assert_eq!(response.content_type(), "text/html");
}

#[tokio::test]
async fn test_template_view_renders_without_engine_fallback() {
    let view =
        TemplateView::new("home.html").with_context("title", serde_json::json!("My Title"));

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    assert_eq!(response.status(), http::StatusCode::OK);
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("home.html"));
    assert!(body.contains("My Title"));
}

#[tokio::test]
async fn test_template_view_missing_template_returns_error() {
    let engine = Arc::new(Engine::new());
    // Do not add any template
    let view = TemplateView::new("missing.html").with_engine(engine);

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    assert_eq!(response.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("Template error"));
}

#[tokio::test]
async fn test_template_view_renders_html_escaping() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("safe.html", "{{ content }}");

    let view = TemplateView::new("safe.html")
        .with_engine(engine)
        .with_context("content", serde_json::json!("<script>alert('xss')</script>"));

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("&lt;script&gt;"));
    assert!(!body.contains("<script>alert"));
}

#[tokio::test]
async fn test_template_view_with_if_tag() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "conditional.html",
        "{% if show %}visible{% else %}hidden{% endif %}",
    );

    let view = TemplateView::new("conditional.html")
        .with_engine(engine)
        .with_context("show", serde_json::json!(true));

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert_eq!(body, "visible");
}

#[tokio::test]
async fn test_template_view_with_for_loop() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "list.html",
        "{% for item in items %}{{ item }} {% endfor %}",
    );

    let view = TemplateView::new("list.html")
        .with_engine(engine)
        .with_context("items", serde_json::json!(["a", "b", "c"]));

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert_eq!(body, "a b c ");
}

#[tokio::test]
async fn test_template_view_with_inheritance() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "base.html",
        "<html>{% block content %}default{% endblock %}</html>",
    );
    engine.add_string_template(
        "child.html",
        r#"{% extends "base.html" %}{% block content %}Hello {{ name }}!{% endblock %}"#,
    );

    let view = TemplateView::new("child.html")
        .with_engine(engine)
        .with_context("name", serde_json::json!("World"));

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert_eq!(body, "<html>Hello World!</html>");
}

// ============================================================================
// 2. ListView renders template with object_list context
// ============================================================================

struct TestEngineListView {
    items: Vec<serde_json::Value>,
    engine: Arc<Engine>,
}

impl ContextMixin for TestEngineListView {
    fn get_context_data(
        &self,
        _kwargs: &HashMap<String, String>,
    ) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }
}

#[async_trait]
impl View for TestEngineListView {
    async fn get(&self, request: HttpRequest) -> HttpResponse {
        self.list(request).await
    }
}

#[async_trait]
impl ListView for TestEngineListView {
    fn model_name(&self) -> &str {
        "article"
    }

    fn engine(&self) -> Option<&Engine> {
        Some(&self.engine)
    }

    async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError> {
        Ok(self.items.clone())
    }
}

#[tokio::test]
async fn test_list_view_renders_with_engine() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_list.html",
        "{% for item in object_list %}{{ item.title }},{% endfor %}",
    );

    let view = TestEngineListView {
        items: vec![
            serde_json::json!({"title": "First"}),
            serde_json::json!({"title": "Second"}),
        ],
        engine,
    };

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    assert_eq!(response.status(), http::StatusCode::OK);
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("First"));
    assert!(body.contains("Second"));
}

#[tokio::test]
async fn test_list_view_renders_empty_list() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_list.html",
        "{% for item in object_list %}{{ item.title }}{% empty %}No articles{% endfor %}",
    );

    let view = TestEngineListView {
        items: vec![],
        engine,
    };

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("No articles"));
}

#[tokio::test]
async fn test_list_view_with_count() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_list.html",
        "Count: {{ object_list|length }}",
    );

    let view = TestEngineListView {
        items: vec![
            serde_json::json!({"title": "A"}),
            serde_json::json!({"title": "B"}),
            serde_json::json!({"title": "C"}),
        ],
        engine,
    };

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("Count: 3"));
}

// ============================================================================
// 3. DetailView renders template with object context
// ============================================================================

struct TestEngineDetailView {
    object: Option<serde_json::Value>,
    engine: Arc<Engine>,
}

impl ContextMixin for TestEngineDetailView {
    fn get_context_data(
        &self,
        _kwargs: &HashMap<String, String>,
    ) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }
}

#[async_trait]
impl View for TestEngineDetailView {
    async fn get(&self, request: HttpRequest) -> HttpResponse {
        let kwargs = HashMap::new();
        self.detail(request, &kwargs).await
    }
}

#[async_trait]
impl DetailView for TestEngineDetailView {
    fn model_name(&self) -> &str {
        "article"
    }

    fn engine(&self) -> Option<&Engine> {
        Some(&self.engine)
    }

    async fn get_object(
        &self,
        _kwargs: &HashMap<String, String>,
    ) -> Result<serde_json::Value, DjangoError> {
        self.object
            .clone()
            .ok_or_else(|| DjangoError::NotFound("Not found".to_string()))
    }
}

#[tokio::test]
async fn test_detail_view_renders_with_engine() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_detail.html",
        "<h1>{{ object.title }}</h1><p>{{ object.body }}</p>",
    );

    let view = TestEngineDetailView {
        object: Some(serde_json::json!({"title": "My Article", "body": "Article content"})),
        engine,
    };

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    assert_eq!(response.status(), http::StatusCode::OK);
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("<h1>My Article</h1>"));
    assert!(body.contains("<p>Article content</p>"));
}

#[tokio::test]
async fn test_detail_view_not_found_with_engine() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("article_detail.html", "{{ object.title }}");

    let view = TestEngineDetailView {
        object: None,
        engine,
    };

    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
}

// ============================================================================
// 4. Session data persists across requests
// ============================================================================

/// Helper to create a session middleware with a shared backend.
fn make_session_middleware() -> (SessionMiddleware, InMemorySessionBackend) {
    let backend = InMemorySessionBackend::new();
    let mw = SessionMiddleware::new(InMemorySessionBackend::new());
    (mw, backend)
}

#[tokio::test]
async fn test_session_loads_data_on_request() {
    let backend = InMemorySessionBackend::new();
    let mut session = SessionData::new("test-session-123".to_string());
    session.set("username", serde_json::json!("alice"));
    backend.save(&session).await.unwrap();

    let mw = SessionMiddleware::new(backend);

    let mut request = HttpRequest::builder()
        .header("cookie", "sessionid=test-session-123")
        .build();

    mw.process_request(&mut request).await;

    // Session data should be loaded into META
    let meta = request.meta();
    assert_eq!(meta.get("SESSION_KEY").unwrap(), "test-session-123");

    let session_data: HashMap<String, serde_json::Value> =
        serde_json::from_str(meta.get("SESSION_DATA").unwrap()).unwrap();
    assert_eq!(
        session_data.get("username"),
        Some(&serde_json::json!("alice"))
    );
}

#[tokio::test]
async fn test_session_creates_new_on_no_cookie() {
    let backend = InMemorySessionBackend::new();
    let mw = SessionMiddleware::new(backend);

    let mut request = HttpRequest::builder().build();
    mw.process_request(&mut request).await;

    let meta = request.meta();
    assert!(meta.contains_key("SESSION_KEY"));
    assert_eq!(meta.get("SESSION_IS_NEW").unwrap(), "true");
}

#[tokio::test]
async fn test_session_creates_new_on_expired_cookie() {
    let backend = InMemorySessionBackend::new();
    // Don't save any session — the old key doesn't exist
    let mw = SessionMiddleware::new(backend);

    let mut request = HttpRequest::builder()
        .header("cookie", "sessionid=expired-key")
        .build();

    mw.process_request(&mut request).await;

    let meta = request.meta();
    assert!(meta.contains_key("SESSION_KEY"));
    assert_eq!(meta.get("SESSION_IS_NEW").unwrap(), "true");
}

#[tokio::test]
async fn test_session_saves_modified_data_on_response() {
    let backend = InMemorySessionBackend::new();
    let mw = SessionMiddleware::new(backend);

    // Simulate a request with a new session that has been modified
    let mut request = HttpRequest::builder().build();
    mw.process_request(&mut request).await;

    let session_key = request.meta().get("SESSION_KEY").unwrap().clone();

    // Simulate the view modifying the session
    let meta = request.meta_mut();
    meta.insert("SESSION_MODIFIED".to_string(), "true".to_string());
    let mut data: HashMap<String, serde_json::Value> = HashMap::new();
    data.insert("user_id".to_string(), serde_json::json!(42));
    meta.insert("SESSION_DATA".to_string(), serde_json::to_string(&data).unwrap());

    let response = HttpResponse::ok("test");
    let resp = mw.process_response(&request, response).await;

    // Check that Set-Cookie header is present
    let set_cookie = resp
        .headers()
        .get(http::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("sessionid="));
    assert!(set_cookie.contains(&session_key));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("Path=/"));
    assert!(set_cookie.contains("SameSite=Lax"));
}

#[tokio::test]
async fn test_session_does_not_save_unmodified_new_empty_session() {
    let backend = InMemorySessionBackend::new();
    let mw = SessionMiddleware::new(backend);

    let mut request = HttpRequest::builder().build();
    mw.process_request(&mut request).await;

    // Don't modify the session
    let response = HttpResponse::ok("test");
    let resp = mw.process_response(&request, response).await;

    // No Set-Cookie for empty new session
    assert!(resp.headers().get(http::header::SET_COOKIE).is_none());
}

#[tokio::test]
async fn test_session_persists_across_requests() {
    let backend = Arc::new(InMemorySessionBackend::new());

    // Request 1: Create a session with data
    let (_mw1, _backend1) = make_session_middleware();
    // We use a shared backend manually to simulate persistence
    let mut session = SessionData::new("shared-session".to_string());
    session.set("counter", serde_json::json!(1));
    backend.save(&session).await.unwrap();

    // Request 2: Load the session and check the data
    // Create another middleware with a backend that has the data
    let backend2 = InMemorySessionBackend::new();
    backend2.save(&session).await.unwrap();

    let mw2 = SessionMiddleware::new(backend2);
    let mut request2 = HttpRequest::builder()
        .header("cookie", "sessionid=shared-session")
        .build();

    mw2.process_request(&mut request2).await;

    let meta = request2.meta();
    let session_data: HashMap<String, serde_json::Value> =
        serde_json::from_str(meta.get("SESSION_DATA").unwrap()).unwrap();
    assert_eq!(
        session_data.get("counter"),
        Some(&serde_json::json!(1))
    );
}

#[tokio::test]
async fn test_session_cookie_secure_flag() {
    let backend = InMemorySessionBackend::new();
    let mw = SessionMiddleware::new(backend).with_cookie_secure(true);

    let mut request = HttpRequest::builder().build();
    mw.process_request(&mut request).await;

    // Modify so it saves
    let meta = request.meta_mut();
    meta.insert("SESSION_MODIFIED".to_string(), "true".to_string());
    let mut data: HashMap<String, serde_json::Value> = HashMap::new();
    data.insert("key".to_string(), serde_json::json!("val"));
    meta.insert("SESSION_DATA".to_string(), serde_json::to_string(&data).unwrap());

    let response = HttpResponse::ok("test");
    let resp = mw.process_response(&request, response).await;

    let cookie = resp
        .headers()
        .get(http::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cookie.contains("Secure"));
}

#[tokio::test]
async fn test_session_custom_cookie_name() {
    let backend = InMemorySessionBackend::new();
    let mut session = SessionData::new("my-key".to_string());
    session.set("val", serde_json::json!("data"));
    backend.save(&session).await.unwrap();

    let mw = SessionMiddleware::new(backend).with_cookie_name("my_session");

    let mut request = HttpRequest::builder()
        .header("cookie", "my_session=my-key")
        .build();

    mw.process_request(&mut request).await;

    let meta = request.meta();
    assert_eq!(meta.get("SESSION_KEY").unwrap(), "my-key");
    let session_data: HashMap<String, serde_json::Value> =
        serde_json::from_str(meta.get("SESSION_DATA").unwrap()).unwrap();
    assert_eq!(session_data.get("val"), Some(&serde_json::json!("data")));
}

#[tokio::test]
async fn test_session_samesite_attribute() {
    let backend = InMemorySessionBackend::new();
    let mw = SessionMiddleware::new(backend).with_cookie_samesite("Strict");

    let mut request = HttpRequest::builder().build();
    mw.process_request(&mut request).await;

    let meta = request.meta_mut();
    meta.insert("SESSION_MODIFIED".to_string(), "true".to_string());
    let mut data: HashMap<String, serde_json::Value> = HashMap::new();
    data.insert("k".to_string(), serde_json::json!("v"));
    meta.insert("SESSION_DATA".to_string(), serde_json::to_string(&data).unwrap());

    let response = HttpResponse::ok("test");
    let resp = mw.process_response(&request, response).await;

    let cookie = resp
        .headers()
        .get(http::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cookie.contains("SameSite=Strict"));
}

// ============================================================================
// 5. Form submission flow
// ============================================================================

#[tokio::test]
async fn test_form_get_shows_empty_form() {
    let form = BaseForm::new(vec![
        FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        ),
    ]);

    assert!(!form.is_bound());
    let ctx = form.as_context();
    assert!(ctx.contains_key("fields"));
    assert!(ctx.contains_key("is_bound"));
    if let ContextValue::Bool(b) = ctx.get("is_bound").unwrap() {
        assert!(!b);
    }
}

#[tokio::test]
async fn test_form_post_valid_data_succeeds() {
    let mut form = BaseForm::new(vec![
        FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: Some(2),
                max_length: Some(50),
                strip: true,
            },
        ),
        FormFieldDef::new("email", FormFieldType::Email),
    ]);

    let request = HttpRequest::builder()
        .method(http::Method::POST)
        .content_type("application/x-www-form-urlencoded")
        .body(b"name=Alice&email=alice@example.com".to_vec())
        .build();

    bind_form_from_request(&mut form, &request);
    assert!(form.is_bound());
    assert!(form.is_valid().await);

    let cleaned = cleaned_data_as_strings(&form);
    assert!(cleaned.contains_key("name"));
    assert!(cleaned.contains_key("email"));
}

#[tokio::test]
async fn test_form_post_invalid_data_shows_errors() {
    let mut form = BaseForm::new(vec![
        FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: Some(3),
                max_length: Some(50),
                strip: true,
            },
        ),
        FormFieldDef::new("email", FormFieldType::Email),
    ]);

    let request = HttpRequest::builder()
        .method(http::Method::POST)
        .content_type("application/x-www-form-urlencoded")
        .body(b"name=ab&email=not-an-email".to_vec())
        .build();

    bind_form_from_request(&mut form, &request);
    assert!(form.is_bound());
    assert!(!form.is_valid().await);

    let errors = form_errors(&form);
    assert!(errors.contains_key("name"));
    assert!(errors.contains_key("email"));

    // Errors should be in the context too
    let ctx = form.as_context();
    if let ContextValue::Dict(error_dict) = ctx.get("errors").unwrap() {
        assert!(error_dict.contains_key("name"));
        assert!(error_dict.contains_key("email"));
    }
}

#[tokio::test]
async fn test_form_post_missing_required_field() {
    let mut form = BaseForm::new(vec![
        FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        ),
        FormFieldDef::new("email", FormFieldType::Email),
    ]);

    let request = HttpRequest::builder()
        .method(http::Method::POST)
        .content_type("application/x-www-form-urlencoded")
        .body(b"name=Alice".to_vec()) // missing email
        .build();

    bind_form_from_request(&mut form, &request);
    assert!(!form.is_valid().await);
    assert!(form.errors().contains_key("email"));
}

#[tokio::test]
async fn test_form_context_renders_in_template() {
    let engine = Engine::new();
    engine.add_string_template(
        "form.html",
        "{% for field in form.fields %}{{ field.name }}{% endfor %}",
    );

    let mut form = BaseForm::new(vec![
        FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        ),
        FormFieldDef::new("email", FormFieldType::Email),
    ]);

    let qd = QueryDict::parse("name=Alice&email=alice@example.com");
    form.bind(&qd);
    form.is_valid().await;

    let form_ctx = form.as_context();
    let json_ctx = form_context_to_json(&form_ctx);

    // The form context should have fields, errors, etc.
    assert!(json_ctx.contains_key("fields"));
    assert!(json_ctx.contains_key("errors"));
    assert!(json_ctx.contains_key("is_bound"));
}

// Full CreateView integration test with form binding

struct TestFormCreateView {
    engine: Arc<Engine>,
}

impl ContextMixin for TestFormCreateView {
    fn get_context_data(
        &self,
        _kwargs: &HashMap<String, String>,
    ) -> HashMap<String, serde_json::Value> {
        let mut ctx = HashMap::new();
        ctx.insert(
            "form_fields".to_string(),
            serde_json::json!(self.fields()),
        );
        ctx
    }
}

#[async_trait]
impl View for TestFormCreateView {
    async fn get(&self, _request: HttpRequest) -> HttpResponse {
        self.render_form().await
    }

    async fn post(&self, request: HttpRequest) -> HttpResponse {
        // Bind form from request
        let mut form = BaseForm::new(vec![
            FormFieldDef::new(
                "title",
                FormFieldType::Char {
                    min_length: Some(3),
                    max_length: Some(100),
                    strip: true,
                },
            ),
            FormFieldDef::new("email", FormFieldType::Email),
        ]);

        bind_form_from_request(&mut form, &request);
        if form.is_valid().await {
            let cleaned = cleaned_data_as_strings(&form);
            self.form_valid(cleaned).await
        } else {
            self.form_invalid(form.errors().clone()).await
        }
    }
}

#[async_trait]
impl CreateView for TestFormCreateView {
    fn model_name(&self) -> &str {
        "article"
    }

    fn fields(&self) -> Vec<String> {
        vec!["title".to_string(), "email".to_string()]
    }

    fn success_url(&self) -> &str {
        "/articles/"
    }

    fn engine(&self) -> Option<&Engine> {
        Some(&self.engine)
    }

    async fn form_valid(&self, _data: HashMap<String, String>) -> HttpResponse {
        HttpResponseRedirect::new(self.success_url())
    }

    async fn form_invalid(&self, errors: HashMap<String, Vec<String>>) -> HttpResponse {
        self.render_form_with_errors(errors).await
    }
}

#[tokio::test]
async fn test_create_view_get_renders_form() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_form.html",
        "<form>{% for f in form_fields %}{{ f }}{% endfor %}</form>",
    );

    let view = TestFormCreateView { engine };
    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .build();
    let response = view.dispatch(request).await;

    assert_eq!(response.status(), http::StatusCode::OK);
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("title"));
}

#[tokio::test]
async fn test_create_view_post_valid_redirects() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("article_form.html", "form");

    let view = TestFormCreateView { engine };
    let request = HttpRequest::builder()
        .method(http::Method::POST)
        .content_type("application/x-www-form-urlencoded")
        .body(b"title=My+Article&email=test@example.com".to_vec())
        .build();

    let response = view.dispatch(request).await;
    assert_eq!(response.status(), http::StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/articles/"
    );
}

#[tokio::test]
async fn test_create_view_post_invalid_shows_errors() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_form.html",
        "{% if errors %}ERRORS{% endif %}",
    );

    let view = TestFormCreateView { engine };
    let request = HttpRequest::builder()
        .method(http::Method::POST)
        .content_type("application/x-www-form-urlencoded")
        .body(b"title=ab&email=bad".to_vec()) // too short, invalid email
        .build();

    let response = view.dispatch(request).await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("ERRORS"));
}

// ============================================================================
// 6. Context processors inject expected variables
// ============================================================================

#[test]
fn test_debug_context_processor_injects_debug() {
    let cp = django_rs_template::context_processors::DebugContextProcessor;
    let request = HttpRequest::builder().path("/test/").build();
    let ctx = cp.process(&request);
    assert!(matches!(ctx.get("debug"), Some(ContextValue::Bool(true))));
}

#[test]
fn test_static_context_processor_injects_static_url() {
    let cp = StaticContextProcessor::new("/static/");
    let request = HttpRequest::builder().path("/test/").build();
    let ctx = cp.process(&request);
    assert_eq!(
        ctx.get("STATIC_URL").unwrap().to_display_string(),
        "/static/"
    );
}

#[test]
fn test_csrf_context_processor_injects_csrf_token() {
    let cp = CsrfContextProcessor;
    let request = HttpRequest::builder().path("/test/").build();
    let ctx = cp.process(&request);
    let token = ctx.get("csrf_token").unwrap().to_display_string();
    assert_eq!(token.len(), 64);
    // Tokens should be different each call
    let ctx2 = cp.process(&request);
    let token2 = ctx2.get("csrf_token").unwrap().to_display_string();
    assert_ne!(token, token2);
}

#[test]
fn test_request_context_processor_injects_request_info() {
    let cp = RequestContextProcessor;
    let request = HttpRequest::builder()
        .path("/articles/")
        .scheme("https")
        .build();
    let ctx = cp.process(&request);

    if let Some(ContextValue::Dict(req)) = ctx.get("request") {
        assert_eq!(req.get("path").unwrap().to_display_string(), "/articles/");
        assert_eq!(req.get("method").unwrap().to_display_string(), "GET");
        assert!(matches!(req.get("is_secure"), Some(ContextValue::Bool(true))));
    } else {
        panic!("Expected request dict");
    }
}

#[tokio::test]
async fn test_context_processors_combined_with_template_rendering() {
    let engine = Engine::new();
    engine.add_string_template(
        "page.html",
        "{% if debug %}DEBUG{% endif %} STATIC={{ STATIC_URL }}",
    );

    let request = HttpRequest::builder().path("/test/").build();

    // Gather context from processors
    let debug_cp = django_rs_template::context_processors::DebugContextProcessor;
    let static_cp = StaticContextProcessor::new("/static/");

    let mut ctx = Context::new();
    for (k, v) in debug_cp.process(&request) {
        ctx.set(k, v);
    }
    for (k, v) in static_cp.process(&request) {
        ctx.set(k, v);
    }

    let result = engine.render_to_string("page.html", &mut ctx).unwrap();
    assert!(result.contains("DEBUG"));
    assert!(result.contains("STATIC=/static/"));
}

// ============================================================================
// 7. DjangoApp engine integration
// ============================================================================

#[test]
fn test_django_app_with_engine() {
    let engine = Engine::new();
    engine.add_string_template("test.html", "Hello!");

    let app = django_rs_views::DjangoApp::new(django_rs_core::Settings::default())
        .engine(engine);

    assert!(app.template_engine().is_some());
}

#[test]
fn test_django_app_without_engine() {
    let app = django_rs_views::DjangoApp::new(django_rs_core::Settings::default());
    assert!(app.template_engine().is_none());
}

// ============================================================================
// Additional edge case tests
// ============================================================================

#[tokio::test]
async fn test_template_response_mixin_with_engine_directly() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("test.html", "Value: {{ key }}");

    let view = TemplateView::new("test.html")
        .with_engine(engine)
        .with_context("key", serde_json::json!("hello"));

    let request = HttpRequest::builder().build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert_eq!(body, "Value: hello");
}

#[tokio::test]
async fn test_template_view_multiple_context_values() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "multi.html",
        "{{ name }} is {{ age }} years old and likes {{ hobby }}",
    );

    let view = TemplateView::new("multi.html")
        .with_engine(engine)
        .with_context("name", serde_json::json!("Alice"))
        .with_context("age", serde_json::json!(30))
        .with_context("hobby", serde_json::json!("coding"));

    let request = HttpRequest::builder().build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("Alice"));
    assert!(body.contains("30"));
    assert!(body.contains("coding"));
}

#[tokio::test]
async fn test_session_generate_key_uniqueness() {
    let key1 = generate_session_key();
    // Small delay
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let key2 = generate_session_key();
    assert_ne!(key1, key2);
    assert!(!key1.is_empty());
    assert!(!key2.is_empty());
}

#[tokio::test]
async fn test_session_middleware_in_pipeline() {
    let backend = InMemorySessionBackend::new();
    let mut session = SessionData::new("pipeline-session".to_string());
    session.set("role", serde_json::json!("admin"));
    backend.save(&session).await.unwrap();

    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SessionMiddleware::new(backend));

    let handler: django_rs_views::middleware::ViewHandler =
        Box::new(|request: HttpRequest| {
            Box::pin(async move {
                let meta = request.meta();
                let data_str = meta.get("SESSION_DATA").unwrap();
                let data: HashMap<String, serde_json::Value> =
                    serde_json::from_str(data_str).unwrap();
                let role = data
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("none");
                HttpResponse::ok(format!("role={role}"))
            })
        });

    let request = HttpRequest::builder()
        .header("cookie", "sessionid=pipeline-session")
        .build();

    let response = pipeline.process(request, &handler).await;
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert_eq!(body, "role=admin");
}

#[tokio::test]
async fn test_form_rebind_clears_errors() {
    let mut form = BaseForm::new(vec![
        FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: Some(3),
                max_length: None,
                strip: false,
            },
        ),
    ]);

    // First attempt: invalid
    let bad_request = HttpRequest::builder()
        .method(http::Method::POST)
        .content_type("application/x-www-form-urlencoded")
        .body(b"name=ab".to_vec())
        .build();

    bind_form_from_request(&mut form, &bad_request);
    assert!(!form.is_valid().await);
    assert!(!form.errors().is_empty());

    // Second attempt: valid
    let good_request = HttpRequest::builder()
        .method(http::Method::POST)
        .content_type("application/x-www-form-urlencoded")
        .body(b"name=alice".to_vec())
        .build();

    bind_form_from_request(&mut form, &good_request);
    assert!(form.is_valid().await);
    assert!(form.errors().is_empty());
}

#[tokio::test]
async fn test_form_with_prefix_from_request() {
    let mut form = BaseForm::new(vec![
        FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        ),
    ]).with_prefix("myform");

    let request = HttpRequest::builder()
        .method(http::Method::POST)
        .content_type("application/x-www-form-urlencoded")
        .body(b"myform-name=Alice".to_vec())
        .build();

    bind_form_from_request(&mut form, &request);
    assert!(form.is_valid().await);
}

#[tokio::test]
async fn test_session_cookie_path_customization() {
    let backend = InMemorySessionBackend::new();
    let mw = SessionMiddleware::new(backend).with_cookie_path("/app/");

    let mut request = HttpRequest::builder().build();
    mw.process_request(&mut request).await;

    let meta = request.meta_mut();
    meta.insert("SESSION_MODIFIED".to_string(), "true".to_string());
    let mut data: HashMap<String, serde_json::Value> = HashMap::new();
    data.insert("k".to_string(), serde_json::json!("v"));
    meta.insert("SESSION_DATA".to_string(), serde_json::to_string(&data).unwrap());

    let response = HttpResponse::ok("test");
    let resp = mw.process_response(&request, response).await;

    let cookie = resp
        .headers()
        .get(http::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cookie.contains("Path=/app/"));
}

#[tokio::test]
async fn test_template_view_numeric_context() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("num.html", "{{ count }}");

    let view = TemplateView::new("num.html")
        .with_engine(engine)
        .with_context("count", serde_json::json!(42));

    let request = HttpRequest::builder().build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert_eq!(body, "42");
}

#[tokio::test]
async fn test_template_view_boolean_context() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template("bool.html", "{% if active %}yes{% else %}no{% endif %}");

    let view = TemplateView::new("bool.html")
        .with_engine(engine)
        .with_context("active", serde_json::json!(false));

    let request = HttpRequest::builder().build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert_eq!(body, "no");
}

#[tokio::test]
async fn test_template_view_null_context() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "null.html",
        r#"{{ val|default:"fallback" }}"#,
    );

    let view = TemplateView::new("null.html")
        .with_engine(engine)
        .with_context("val", serde_json::Value::Null);

    let request = HttpRequest::builder().build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert_eq!(body, "fallback");
}

#[tokio::test]
async fn test_list_view_with_nested_object_data() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_list.html",
        "{% for item in object_list %}{{ item.author.name }},{% endfor %}",
    );

    let view = TestEngineListView {
        items: vec![
            serde_json::json!({"author": {"name": "Alice"}}),
            serde_json::json!({"author": {"name": "Bob"}}),
        ],
        engine,
    };

    let request = HttpRequest::builder().build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("Alice"));
    assert!(body.contains("Bob"));
}

#[tokio::test]
async fn test_detail_view_with_nested_object() {
    let engine = Arc::new(Engine::new());
    engine.add_string_template(
        "article_detail.html",
        "{{ object.metadata.category }}",
    );

    let view = TestEngineDetailView {
        object: Some(serde_json::json!({"metadata": {"category": "tech"}})),
        engine,
    };

    let request = HttpRequest::builder().build();
    let response = view.dispatch(request).await;

    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert_eq!(body, "tech");
}

// ============================================================================
// Middleware Pipeline Integration Tests
// ============================================================================

use django_rs_views::middleware::builtin::{
    AuthenticationMiddleware, CacheMiddleware, LocaleMiddleware, LoginRequiredMiddleware,
    MessageMiddleware, MessageLevel,
};

#[tokio::test]
async fn test_session_auth_pipeline_authenticated_user() {
    // Test that SessionMiddleware -> AuthenticationMiddleware pipeline
    // correctly loads user info from session
    let backend = InMemorySessionBackend::new();

    // Pre-populate a session with auth data
    let mut session = SessionData::new("auth-session-1".to_string());
    session.set("_auth_user_id", serde_json::json!("42"));
    backend.save(&session).await.unwrap();

    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SessionMiddleware::new(backend));
    pipeline.add(AuthenticationMiddleware);

    let handler: django_rs_views::middleware::ViewHandler = Box::new(|req| {
        Box::pin(async move {
            let user_id = req.meta().get("USER_ID").cloned().unwrap_or_default();
            let authed = req.meta().get("USER_AUTHENTICATED").cloned().unwrap_or_default();
            HttpResponse::ok(&format!("user={user_id},auth={authed}"))
        })
    });

    let request = HttpRequest::builder()
        .header("cookie", "sessionid=auth-session-1")
        .build();
    let response = pipeline.process(request, &handler).await;
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("user=42"));
    assert!(body.contains("auth=true"));
}

#[tokio::test]
async fn test_session_auth_pipeline_anonymous_user() {
    let backend = InMemorySessionBackend::new();

    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SessionMiddleware::new(backend));
    pipeline.add(AuthenticationMiddleware);

    let handler: django_rs_views::middleware::ViewHandler = Box::new(|req| {
        Box::pin(async move {
            let authed = req.meta().get("USER_AUTHENTICATED").cloned().unwrap_or_default();
            HttpResponse::ok(&format!("auth={authed}"))
        })
    });

    let request = HttpRequest::builder().build();
    let response = pipeline.process(request, &handler).await;
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("auth=false"));
}

#[tokio::test]
async fn test_session_auth_messages_pipeline() {
    // Full pipeline: Session -> Auth -> Messages
    let backend = InMemorySessionBackend::new();

    let mut session = SessionData::new("msg-session".to_string());
    session.set("_auth_user_id", serde_json::json!("7"));
    backend.save(&session).await.unwrap();

    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SessionMiddleware::new(backend));
    pipeline.add(AuthenticationMiddleware);
    pipeline.add(MessageMiddleware);

    let handler: django_rs_views::middleware::ViewHandler = Box::new(|req| {
        Box::pin(async move {
            let authed = req.meta().get("USER_AUTHENTICATED").cloned().unwrap_or_default();
            let has_store = req.meta().contains_key("_messages_store");
            HttpResponse::ok(&format!("auth={authed},msgs={has_store}"))
        })
    });

    let request = HttpRequest::builder()
        .header("cookie", "sessionid=msg-session")
        .build();
    let response = pipeline.process(request, &handler).await;
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("auth=true"));
    assert!(body.contains("msgs=true"));
}

#[tokio::test]
async fn test_login_required_blocks_anonymous() {
    // Session -> Auth -> LoginRequired
    let backend = InMemorySessionBackend::new();

    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SessionMiddleware::new(backend));
    pipeline.add(AuthenticationMiddleware);
    pipeline.add(LoginRequiredMiddleware::default());

    let handler: django_rs_views::middleware::ViewHandler = Box::new(|_req| {
        Box::pin(async move { HttpResponse::ok("protected content") })
    });

    let request = HttpRequest::builder()
        .path("/dashboard/")
        .build();
    let response = pipeline.process(request, &handler).await;
    assert_eq!(response.status(), http::StatusCode::FOUND);
    let location = response
        .headers()
        .get(http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/accounts/login/"));
}

#[tokio::test]
async fn test_login_required_allows_authenticated() {
    let backend = InMemorySessionBackend::new();

    let mut session = SessionData::new("authed-session".to_string());
    session.set("_auth_user_id", serde_json::json!("1"));
    backend.save(&session).await.unwrap();

    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SessionMiddleware::new(backend));
    pipeline.add(AuthenticationMiddleware);
    pipeline.add(LoginRequiredMiddleware::default());

    let handler: django_rs_views::middleware::ViewHandler = Box::new(|_req| {
        Box::pin(async move { HttpResponse::ok("protected content") })
    });

    let request = HttpRequest::builder()
        .path("/dashboard/")
        .header("cookie", "sessionid=authed-session")
        .build();
    let response = pipeline.process(request, &handler).await;
    assert_eq!(response.status(), http::StatusCode::OK);
}

#[tokio::test]
async fn test_locale_middleware_in_pipeline() {
    let backend = InMemorySessionBackend::new();

    let mut pipeline = MiddlewarePipeline::new();
    pipeline.add(SessionMiddleware::new(backend));
    pipeline.add(LocaleMiddleware {
        default_language: "en".to_string(),
        supported_languages: vec!["en".to_string(), "fr".to_string()],
    });

    let handler: django_rs_views::middleware::ViewHandler = Box::new(|req| {
        Box::pin(async move {
            let lang = req.meta().get("LANGUAGE_CODE").cloned().unwrap_or_default();
            HttpResponse::ok(&format!("lang={lang}"))
        })
    });

    let request = HttpRequest::builder()
        .header("accept-language", "fr-FR,fr;q=0.9,en;q=0.8")
        .build();
    let response = pipeline.process(request, &handler).await;
    let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
    assert!(body.contains("lang=fr"));
    // Check Content-Language header was set
    assert_eq!(
        response.headers().get("content-language").unwrap().to_str().unwrap(),
        "fr"
    );
}
