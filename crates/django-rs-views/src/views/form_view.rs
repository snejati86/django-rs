//! Form-view integration for django-rs.
//!
//! This module provides helpers to connect the forms framework to views,
//! enabling extraction of POST data from requests, binding to forms,
//! validation, and template rendering with form errors.
//!
//! ## Key Functions
//!
//! - [`bind_form_from_request`] - Extracts POST data from an `HttpRequest` and binds it to a form
//! - [`extract_post_data`] - Extracts form data from the request body as a `QueryDict`
//! - [`form_context_to_json`] - Converts form context (ContextValues) to serde_json for views

use std::collections::HashMap;
use std::sync::Arc;

use django_rs_forms::form::{BaseForm, Form};
use django_rs_http::{HttpRequest, HttpResponse, QueryDict};
use django_rs_template::context::{Context, ContextValue};
use django_rs_template::engine::Engine;

/// Extracts POST form data from an `HttpRequest`.
///
/// Parses the request body as URL-encoded form data if the content type is
/// `application/x-www-form-urlencoded`. Otherwise returns an empty `QueryDict`.
///
/// # Examples
///
/// ```
/// use django_rs_views::views::form_view::extract_post_data;
/// use django_rs_http::HttpRequest;
///
/// let request = HttpRequest::builder()
///     .method(http::Method::POST)
///     .content_type("application/x-www-form-urlencoded")
///     .body(b"name=Alice&email=alice@example.com".to_vec())
///     .build();
///
/// let data = extract_post_data(&request);
/// assert_eq!(data.get("name"), Some("Alice"));
/// ```
pub fn extract_post_data(request: &HttpRequest) -> QueryDict {
    // The HttpRequest already parses POST data in its constructor
    // We can use the post() method directly, but we return a clone
    // since QueryDict is immutable by default
    request.post().clone()
}

/// Binds form data from an HTTP request to a form.
///
/// Extracts URL-encoded POST data from the request body and calls
/// `form.bind()` with it. After this call, the form is bound and
/// ready for validation via `form.is_valid()`.
///
/// # Examples
///
/// ```
/// use django_rs_views::views::form_view::bind_form_from_request;
/// use django_rs_forms::form::{BaseForm, Form};
/// use django_rs_forms::fields::{FormFieldDef, FormFieldType};
/// use django_rs_http::HttpRequest;
///
/// let mut form = BaseForm::new(vec![
///     FormFieldDef::new("name", FormFieldType::Char {
///         min_length: None, max_length: None, strip: false,
///     }),
/// ]);
///
/// let request = HttpRequest::builder()
///     .method(http::Method::POST)
///     .content_type("application/x-www-form-urlencoded")
///     .body(b"name=Alice".to_vec())
///     .build();
///
/// bind_form_from_request(&mut form, &request);
/// assert!(form.is_bound());
/// ```
pub fn bind_form_from_request(form: &mut dyn Form, request: &HttpRequest) {
    let data = extract_post_data(request);
    form.bind(&data);
}

/// Converts a form's template context (HashMap<String, ContextValue>) to
/// serde_json::Value for use in generic view contexts.
///
/// This bridges the gap between the forms framework (which uses ContextValue)
/// and the views framework (which uses serde_json::Value).
pub fn form_context_to_json(
    form_ctx: &HashMap<String, ContextValue>,
) -> HashMap<String, serde_json::Value> {
    form_ctx
        .iter()
        .map(|(k, v)| (k.clone(), context_value_to_json(v)))
        .collect()
}

/// Converts a `ContextValue` to a `serde_json::Value`.
fn context_value_to_json(value: &ContextValue) -> serde_json::Value {
    match value {
        ContextValue::String(s) | ContextValue::SafeString(s) => {
            serde_json::Value::String(s.clone())
        }
        ContextValue::Integer(i) => serde_json::json!(i),
        ContextValue::Float(f) => serde_json::json!(f),
        ContextValue::Bool(b) => serde_json::Value::Bool(*b),
        ContextValue::None => serde_json::Value::Null,
        ContextValue::List(items) => {
            let arr: Vec<serde_json::Value> = items.iter().map(context_value_to_json).collect();
            serde_json::Value::Array(arr)
        }
        ContextValue::Dict(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), context_value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
    }
}

/// Helper to get cleaned data from a valid form as a `HashMap<String, String>`.
///
/// Converts `Value` types to strings for simpler processing in view handlers.
pub fn cleaned_data_as_strings(form: &dyn Form) -> HashMap<String, String> {
    form.cleaned_data()
        .iter()
        .map(|(k, v)| (k.clone(), format!("{v}")))
        .collect()
}

/// Helper to get form errors as a `HashMap<String, Vec<String>>`.
pub fn form_errors(form: &dyn Form) -> HashMap<String, Vec<String>> {
    form.errors().clone()
}

/// Type alias for a form factory function.
///
/// The factory creates a new `BaseForm` instance each time the view needs one.
/// This avoids the need to clone `FormFieldDef` values (which contain trait objects).
pub type FormFactory = Arc<dyn Fn() -> BaseForm + Send + Sync>;

/// A generic view for displaying and processing a form.
///
/// Mirrors Django's `FormView` generic class-based view. On GET requests,
/// renders the form template with an empty form. On POST requests, validates
/// the form data and either redirects (on success) or re-renders with errors.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use django_rs_views::FormView;
/// use django_rs_forms::fields::{FormFieldDef, FormFieldType};
/// use django_rs_forms::form::BaseForm;
///
/// let view = FormView::new("contact.html", "/thanks/")
///     .form_factory(Arc::new(|| BaseForm::new(vec![
///         FormFieldDef::new("name", FormFieldType::Char {
///             min_length: Some(1), max_length: Some(100), strip: true,
///         }),
///         FormFieldDef::new("email", FormFieldType::Email),
///     ])));
/// ```
pub struct FormView {
    form_factory: Option<FormFactory>,
    template_name: String,
    success_url: String,
    initial: HashMap<String, String>,
    engine: Option<Arc<Engine>>,
}

impl FormView {
    /// Creates a new `FormView` with the given template and success URL.
    pub fn new(template_name: &str, success_url: &str) -> Self {
        Self {
            form_factory: None,
            template_name: template_name.to_string(),
            success_url: success_url.to_string(),
            initial: HashMap::new(),
            engine: None,
        }
    }

    /// Sets the form factory that creates new form instances.
    #[must_use]
    pub fn form_factory(mut self, factory: FormFactory) -> Self {
        self.form_factory = Some(factory);
        self
    }

    /// Sets the template engine for rendering.
    #[must_use]
    pub fn engine(mut self, engine: Arc<Engine>) -> Self {
        self.engine = Some(engine);
        self
    }

    /// Sets initial field values.
    #[must_use]
    pub fn initial(mut self, key: &str, value: &str) -> Self {
        self.initial.insert(key.to_string(), value.to_string());
        self
    }

    /// Returns the template name.
    pub fn template_name(&self) -> &str {
        &self.template_name
    }

    /// Returns the success URL.
    pub fn success_url(&self) -> &str {
        &self.success_url
    }

    /// Creates a new form instance using the configured factory.
    fn create_form(&self) -> BaseForm {
        if let Some(ref factory) = self.form_factory {
            factory()
        } else {
            BaseForm::new(vec![])
        }
    }

    /// Dispatches the request to the appropriate handler.
    ///
    /// - GET: Renders the form template with an empty form
    /// - POST: Validates the form, calls `form_valid` or `form_invalid`
    pub async fn dispatch(&self, request: &HttpRequest) -> HttpResponse {
        match *request.method() {
            http::Method::GET | http::Method::HEAD => self.render_form(None),
            http::Method::POST => self.process_form(request).await,
            _ => HttpResponse::not_allowed(&["GET", "POST"]),
        }
    }

    /// Handles a valid form submission by redirecting to the success URL.
    pub fn form_valid(&self, _cleaned_data: &HashMap<String, serde_json::Value>) -> HttpResponse {
        HttpResponse::redirect(&self.success_url)
    }

    /// Handles an invalid form submission by re-rendering the template with errors.
    pub fn form_invalid(
        &self,
        errors: &HashMap<String, Vec<String>>,
        form_context: &HashMap<String, ContextValue>,
    ) -> HttpResponse {
        let mut context: HashMap<String, serde_json::Value> = HashMap::new();

        // Add form context
        let form_json = form_context_to_json(form_context);
        context.insert(
            "form".to_string(),
            serde_json::to_value(&form_json).unwrap_or_default(),
        );

        // Add errors directly
        context.insert(
            "errors".to_string(),
            serde_json::to_value(errors).unwrap_or_default(),
        );

        // Add initial values
        if !self.initial.is_empty() {
            context.insert(
                "initial".to_string(),
                serde_json::to_value(&self.initial).unwrap_or_default(),
            );
        }

        self.render_template(&context)
    }

    /// Renders the form template (for GET requests or re-render on error).
    fn render_form(
        &self,
        extra_context: Option<HashMap<String, serde_json::Value>>,
    ) -> HttpResponse {
        let form = self.create_form();
        let form_ctx = form.as_context();
        let form_json = form_context_to_json(&form_ctx);

        let mut context: HashMap<String, serde_json::Value> = HashMap::new();
        context.insert(
            "form".to_string(),
            serde_json::to_value(&form_json).unwrap_or_default(),
        );

        // Add initial values
        if !self.initial.is_empty() {
            context.insert(
                "initial".to_string(),
                serde_json::to_value(&self.initial).unwrap_or_default(),
            );
        }

        if let Some(extra) = extra_context {
            context.extend(extra);
        }

        self.render_template(&context)
    }

    /// Processes a POST request: binds, validates, and dispatches.
    async fn process_form(&self, request: &HttpRequest) -> HttpResponse {
        let mut form = self.create_form();
        let data = extract_post_data(request);
        form.bind(&data);

        if form.is_valid().await {
            let cleaned: HashMap<String, serde_json::Value> = form
                .cleaned_data()
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
                .collect();
            self.form_valid(&cleaned)
        } else {
            let errors = form.errors().clone();
            let form_ctx = form.as_context();
            self.form_invalid(&errors, &form_ctx)
        }
    }

    /// Renders the template with the given context.
    fn render_template(&self, context: &HashMap<String, serde_json::Value>) -> HttpResponse {
        if let Some(ref engine) = self.engine {
            let mut template_context = Context::new();
            for (key, value) in context {
                template_context.set(key.clone(), ContextValue::from(value.clone()));
            }
            match engine.render_to_string(&self.template_name, &mut template_context) {
                Ok(html) => {
                    let mut response = HttpResponse::ok(html);
                    response.set_content_type("text/html");
                    response
                }
                Err(e) => HttpResponse::server_error(format!("Template error: {e}")),
            }
        } else {
            // Fallback: JSON representation
            let body = serde_json::to_string_pretty(context).unwrap_or_default();
            let html = format!(
                "<!-- Template: {} -->\n<html><body><pre>{body}</pre></body></html>",
                self.template_name
            );
            let mut response = HttpResponse::ok(html);
            response.set_content_type("text/html");
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use django_rs_forms::fields::{FormFieldDef, FormFieldType};
    #[allow(unused_imports)]
    use django_rs_forms::form::BaseForm;
    use std::sync::Arc;

    fn make_test_form() -> BaseForm {
        BaseForm::new(vec![
            FormFieldDef::new(
                "username",
                FormFieldType::Char {
                    min_length: Some(3),
                    max_length: Some(20),
                    strip: true,
                },
            ),
            FormFieldDef::new("email", FormFieldType::Email),
        ])
    }

    #[test]
    fn test_extract_post_data_form_encoded() {
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"username=alice&email=alice@example.com".to_vec())
            .build();

        let data = extract_post_data(&request);
        assert_eq!(data.get("username"), Some("alice"));
        assert_eq!(data.get("email"), Some("alice@example.com"));
    }

    #[test]
    fn test_extract_post_data_empty() {
        let request = HttpRequest::builder().method(http::Method::GET).build();

        let data = extract_post_data(&request);
        assert!(data.is_empty());
    }

    #[test]
    fn test_bind_form_from_request() {
        let mut form = make_test_form();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"username=alice&email=alice@example.com".to_vec())
            .build();

        bind_form_from_request(&mut form, &request);
        assert!(form.is_bound());
    }

    #[tokio::test]
    async fn test_bind_form_valid() {
        let mut form = make_test_form();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"username=alice&email=alice@example.com".to_vec())
            .build();

        bind_form_from_request(&mut form, &request);
        assert!(form.is_valid().await);
        assert!(form.errors().is_empty());
    }

    #[tokio::test]
    async fn test_bind_form_invalid() {
        let mut form = make_test_form();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"username=ab&email=not-email".to_vec())
            .build();

        bind_form_from_request(&mut form, &request);
        assert!(!form.is_valid().await);
        assert!(form.errors().contains_key("username"));
        assert!(form.errors().contains_key("email"));
    }

    #[tokio::test]
    async fn test_bind_form_missing_fields() {
        let mut form = make_test_form();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"".to_vec())
            .build();

        bind_form_from_request(&mut form, &request);
        assert!(!form.is_valid().await);
        assert!(form.errors().contains_key("username"));
        assert!(form.errors().contains_key("email"));
    }

    #[tokio::test]
    async fn test_cleaned_data_as_strings() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=alice&email=alice@example.com");
        form.bind(&qd);
        form.is_valid().await;

        let strings = cleaned_data_as_strings(&form);
        assert!(strings.contains_key("username"));
        assert!(strings.contains_key("email"));
    }

    #[tokio::test]
    async fn test_form_errors_helper() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=ab&email=bad");
        form.bind(&qd);
        form.is_valid().await;

        let errors = form_errors(&form);
        assert!(!errors.is_empty());
        assert!(errors.contains_key("username"));
    }

    #[test]
    fn test_form_context_to_json() {
        let mut ctx = HashMap::new();
        ctx.insert("is_bound".to_string(), ContextValue::Bool(true));
        ctx.insert("name".to_string(), ContextValue::String("test".to_string()));

        let json = form_context_to_json(&ctx);
        assert_eq!(json.get("is_bound"), Some(&serde_json::json!(true)));
        assert_eq!(json.get("name"), Some(&serde_json::json!("test")));
    }

    #[test]
    fn test_context_value_to_json_types() {
        assert_eq!(
            context_value_to_json(&ContextValue::Integer(42)),
            serde_json::json!(42)
        );
        assert_eq!(
            context_value_to_json(&ContextValue::Float(3.14)),
            serde_json::json!(3.14)
        );
        assert_eq!(
            context_value_to_json(&ContextValue::Bool(true)),
            serde_json::json!(true)
        );
        assert_eq!(
            context_value_to_json(&ContextValue::None),
            serde_json::Value::Null
        );
        assert_eq!(
            context_value_to_json(&ContextValue::String("hello".to_string())),
            serde_json::json!("hello")
        );
        assert_eq!(
            context_value_to_json(&ContextValue::SafeString("<b>safe</b>".to_string())),
            serde_json::json!("<b>safe</b>")
        );

        let list = ContextValue::List(vec![ContextValue::Integer(1), ContextValue::Integer(2)]);
        assert_eq!(context_value_to_json(&list), serde_json::json!([1, 2]));

        let mut dict = HashMap::new();
        dict.insert("key".to_string(), ContextValue::String("value".to_string()));
        let dict_val = ContextValue::Dict(dict);
        assert_eq!(
            context_value_to_json(&dict_val),
            serde_json::json!({"key": "value"})
        );
    }

    #[tokio::test]
    async fn test_form_as_context_to_json() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=alice&email=alice@example.com");
        form.bind(&qd);
        form.is_valid().await;

        let ctx = form.as_context();
        let json = form_context_to_json(&ctx);
        assert!(json.contains_key("fields"));
        assert!(json.contains_key("errors"));
        assert!(json.contains_key("is_bound"));
    }

    #[test]
    fn test_extract_post_data_json_body_ignored() {
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/json")
            .body(b"{\"key\": \"value\"}".to_vec())
            .build();

        let data = extract_post_data(&request);
        assert!(data.is_empty());
    }

    #[tokio::test]
    async fn test_full_form_submission_flow() {
        // Simulate: GET shows empty form, POST with good data validates
        let mut form = make_test_form();
        assert!(!form.is_bound());

        // POST with valid data
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"username=alice&email=alice@example.com".to_vec())
            .build();

        bind_form_from_request(&mut form, &request);
        assert!(form.is_bound());
        assert!(form.is_valid().await);

        let cleaned = cleaned_data_as_strings(&form);
        assert!(!cleaned.is_empty());
    }

    // ── FormView tests ──────────────────────────────────────────────

    fn make_form_view() -> FormView {
        FormView::new("contact.html", "/thanks/").form_factory(Arc::new(|| {
            BaseForm::new(vec![
                FormFieldDef::new(
                    "name",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(100),
                        strip: true,
                    },
                ),
                FormFieldDef::new("email", FormFieldType::Email),
            ])
        }))
    }

    #[tokio::test]
    async fn test_formview_get_renders_form() {
        let view = make_form_view();
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.dispatch(&request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("contact.html"));
    }

    #[tokio::test]
    async fn test_formview_post_valid_redirects() {
        let view = make_form_view();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"name=Alice&email=alice@example.com".to_vec())
            .build();
        let response = view.dispatch(&request).await;
        assert_eq!(response.status(), http::StatusCode::FOUND);
        assert_eq!(
            response
                .headers()
                .get(http::header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "/thanks/"
        );
    }

    #[tokio::test]
    async fn test_formview_post_invalid_shows_errors() {
        let view = make_form_view();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"name=&email=not-an-email".to_vec())
            .build();
        let response = view.dispatch(&request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("errors"));
    }

    #[tokio::test]
    async fn test_formview_post_missing_fields() {
        let view = make_form_view();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"".to_vec())
            .build();
        let response = view.dispatch(&request).await;
        // Should re-render with errors, not redirect
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("errors"));
    }

    #[tokio::test]
    async fn test_formview_method_not_allowed() {
        let view = make_form_view();
        let request = HttpRequest::builder().method(http::Method::DELETE).build();
        let response = view.dispatch(&request).await;
        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_formview_head_renders_form() {
        let view = make_form_view();
        let request = HttpRequest::builder().method(http::Method::HEAD).build();
        let response = view.dispatch(&request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[test]
    fn test_formview_template_name() {
        let view = make_form_view();
        assert_eq!(view.template_name(), "contact.html");
    }

    #[test]
    fn test_formview_success_url() {
        let view = make_form_view();
        assert_eq!(view.success_url(), "/thanks/");
    }

    #[tokio::test]
    async fn test_formview_initial_data() {
        let view = make_form_view()
            .initial("name", "Default Name")
            .initial("email", "default@example.com");
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.dispatch(&request).await;
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("Default Name"));
        assert!(body.contains("default@example.com"));
    }

    #[tokio::test]
    async fn test_formview_form_valid_returns_redirect() {
        let view = make_form_view();
        let cleaned = HashMap::new();
        let response = view.form_valid(&cleaned);
        assert_eq!(response.status(), http::StatusCode::FOUND);
    }

    #[test]
    fn test_formview_form_invalid_shows_template() {
        let view = make_form_view();
        let mut errors = HashMap::new();
        errors.insert("name".to_string(), vec!["Required".to_string()]);
        let form_ctx = HashMap::new();
        let response = view.form_invalid(&errors, &form_ctx);
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("Required"));
    }
}
