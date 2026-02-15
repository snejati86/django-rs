//! Class-based views (CBVs) for django-rs.
//!
//! This module provides the [`View`] trait and related mixins that mirror Django's
//! class-based view system. CBVs organize view logic into traits with default
//! implementations for common HTTP methods.
//!
//! ## Key Types
//!
//! - [`View`] - The base trait for all class-based views
//! - [`ContextMixin`] - Provides template context data
//! - [`TemplateResponseMixin`] - Renders templates with context using the template engine
//! - [`TemplateView`] - A concrete view that renders a template
//! - [`RedirectView`] - A concrete view that redirects to a URL

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;

use django_rs_http::{HttpRequest, HttpResponse, HttpResponseRedirect};
use django_rs_template::context::{Context, ContextValue};
use django_rs_template::engine::Engine;

use super::function::ViewFunction;

/// The base trait for class-based views, mirroring Django's `View`.
///
/// Provides HTTP method dispatch and default implementations that return
/// 405 Method Not Allowed. Override the specific HTTP method handlers
/// (e.g., `get`, `post`) to implement your view logic.
///
/// # Examples
///
/// ```
/// use async_trait::async_trait;
/// use django_rs_views::views::class_based::View;
/// use django_rs_http::{HttpRequest, HttpResponse};
///
/// struct MyView;
///
/// #[async_trait]
/// impl View for MyView {
///     async fn get(&self, _request: HttpRequest) -> HttpResponse {
///         HttpResponse::ok("Hello from MyView!")
///     }
/// }
/// ```
#[async_trait]
pub trait View: Send + Sync {
    /// Returns the list of HTTP methods this view allows.
    fn allowed_methods(&self) -> Vec<http::Method> {
        vec![
            http::Method::GET,
            http::Method::POST,
            http::Method::PUT,
            http::Method::PATCH,
            http::Method::DELETE,
            http::Method::HEAD,
            http::Method::OPTIONS,
        ]
    }

    /// Dispatches the request to the appropriate HTTP method handler.
    ///
    /// This is the main entry point for the view. It checks the request method
    /// and calls the corresponding handler method.
    async fn dispatch(&self, request: HttpRequest) -> HttpResponse {
        match *request.method() {
            http::Method::GET => self.get(request).await,
            http::Method::POST => self.post(request).await,
            http::Method::PUT => self.put(request).await,
            http::Method::PATCH => self.patch(request).await,
            http::Method::DELETE => self.delete(request).await,
            http::Method::HEAD => self.head(request).await,
            http::Method::OPTIONS => self.options(request).await,
            _ => self.http_method_not_allowed(request).await,
        }
    }

    /// Handles GET requests. Returns 405 by default.
    async fn get(&self, request: HttpRequest) -> HttpResponse {
        self.http_method_not_allowed(request).await
    }

    /// Handles POST requests. Returns 405 by default.
    async fn post(&self, request: HttpRequest) -> HttpResponse {
        self.http_method_not_allowed(request).await
    }

    /// Handles PUT requests. Returns 405 by default.
    async fn put(&self, request: HttpRequest) -> HttpResponse {
        self.http_method_not_allowed(request).await
    }

    /// Handles PATCH requests. Returns 405 by default.
    async fn patch(&self, request: HttpRequest) -> HttpResponse {
        self.http_method_not_allowed(request).await
    }

    /// Handles DELETE requests. Returns 405 by default.
    async fn delete(&self, request: HttpRequest) -> HttpResponse {
        self.http_method_not_allowed(request).await
    }

    /// Handles HEAD requests. Delegates to `get` by default.
    async fn head(&self, request: HttpRequest) -> HttpResponse {
        self.get(request).await
    }

    /// Handles OPTIONS requests. Returns the list of allowed methods.
    async fn options(&self, _request: HttpRequest) -> HttpResponse {
        let methods = self.allowed_methods();
        let method_strs: Vec<&str> = methods.iter().map(http::Method::as_str).collect();
        let mut response = HttpResponse::ok("");
        if let Ok(value) = http::header::HeaderValue::from_str(&method_strs.join(", ")) {
            response.headers_mut().insert(http::header::ALLOW, value);
        }
        response
    }

    /// Returns a 405 Method Not Allowed response with the allowed methods header.
    async fn http_method_not_allowed(&self, _request: HttpRequest) -> HttpResponse {
        let methods = self.allowed_methods();
        let method_strs: Vec<&str> = methods.iter().map(http::Method::as_str).collect();
        HttpResponse::not_allowed(&method_strs)
    }

    /// Converts this class-based view into a function-based view.
    ///
    /// This allows CBVs to be used in URL patterns that expect a function handler.
    /// Named `as_view` to match Django's convention.
    #[allow(clippy::wrong_self_convention)]
    fn as_view(self) -> ViewFunction
    where
        Self: Sized + 'static,
    {
        let view = std::sync::Arc::new(self);
        Box::new(move |request: HttpRequest| -> Pin<Box<dyn Future<Output = HttpResponse> + Send>> {
            let view = view.clone();
            Box::pin(async move { view.dispatch(request).await })
        })
    }
}

/// Mixin that provides template context data.
///
/// This mirrors Django's `ContextMixin`. Implementors provide extra context
/// data that will be merged into the template rendering context.
pub trait ContextMixin {
    /// Returns context data for template rendering.
    ///
    /// The `kwargs` parameter contains URL path parameters extracted by the
    /// URL resolver.
    fn get_context_data(
        &self,
        kwargs: &HashMap<String, String>,
    ) -> HashMap<String, serde_json::Value>;
}

/// Mixin that renders a template with context data.
///
/// This mirrors Django's `TemplateResponseMixin`. It provides methods to
/// determine the template name and render the template to a response.
///
/// When an `Engine` is provided via `render_to_response_with_engine`, the
/// template is rendered using the full template engine. When no engine is
/// available, it falls back to a JSON-based representation for backward
/// compatibility.
pub trait TemplateResponseMixin: View {
    /// Returns the primary template name.
    fn template_name(&self) -> &str;

    /// Returns a list of template names to try, in order.
    fn get_template_names(&self) -> Vec<String> {
        vec![self.template_name().to_string()]
    }

    /// Renders the template with the given context using the template engine.
    ///
    /// Converts serde_json::Value context into template ContextValues and renders
    /// through the Engine. Falls back to `render_to_response` if the engine is None.
    fn render_to_response_with_engine(
        &self,
        context: HashMap<String, serde_json::Value>,
        engine: Option<&Engine>,
    ) -> HttpResponse {
        let Some(engine) = engine else {
            return self.render_to_response(context);
        };

        let template_name = self.template_name();
        let mut template_context = Context::new();
        for (key, value) in context {
            template_context.set(key, ContextValue::from(value));
        }

        match engine.render_to_string(template_name, &mut template_context) {
            Ok(html) => {
                let mut response = HttpResponse::ok(html);
                response.set_content_type("text/html");
                response
            }
            Err(e) => HttpResponse::server_error(format!("Template error: {e}")),
        }
    }

    /// Renders the template with the given context and returns an `HttpResponse`.
    ///
    /// This is a fallback implementation that serializes the context as JSON
    /// when no template engine is available. For engine-backed rendering, use
    /// `render_to_response_with_engine`.
    fn render_to_response(
        &self,
        context: HashMap<String, serde_json::Value>,
    ) -> HttpResponse {
        let template_name = self.template_name();
        let context_json = serde_json::to_string_pretty(&context).unwrap_or_default();
        let body = format!(
            "<!-- Template: {template_name} -->\n<html><body><pre>{context_json}</pre></body></html>"
        );
        let mut response = HttpResponse::ok(body);
        response.set_content_type("text/html");
        response
    }
}

/// A view that renders a template. Equivalent to Django's `TemplateView`.
///
/// Combines the `View`, `ContextMixin`, and `TemplateResponseMixin` functionality
/// to render a simple template with optional context data.
///
/// When an `Engine` is attached via `with_engine`, templates are rendered using the
/// full template engine. Otherwise, a JSON fallback is used.
///
/// # Examples
///
/// ```
/// use django_rs_views::views::class_based::TemplateView;
///
/// let view = TemplateView::new("home.html");
/// ```
pub struct TemplateView {
    template: String,
    extra_context: HashMap<String, serde_json::Value>,
    engine: Option<Arc<Engine>>,
}

impl TemplateView {
    /// Creates a new `TemplateView` that renders the given template.
    pub fn new(template: &str) -> Self {
        Self {
            template: template.to_string(),
            extra_context: HashMap::new(),
            engine: None,
        }
    }

    /// Attaches a template engine to this view.
    #[must_use]
    pub fn with_engine(mut self, engine: Arc<Engine>) -> Self {
        self.engine = Some(engine);
        self
    }

    /// Adds extra context data to this view.
    #[must_use]
    pub fn with_context(mut self, key: &str, value: serde_json::Value) -> Self {
        self.extra_context.insert(key.to_string(), value);
        self
    }
}

impl ContextMixin for TemplateView {
    fn get_context_data(
        &self,
        kwargs: &HashMap<String, String>,
    ) -> HashMap<String, serde_json::Value> {
        let mut context = self.extra_context.clone();
        // Add kwargs to context
        for (key, value) in kwargs {
            context.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
        context
    }
}

impl TemplateResponseMixin for TemplateView {
    fn template_name(&self) -> &str {
        &self.template
    }
}

#[async_trait]
impl View for TemplateView {
    async fn get(&self, _request: HttpRequest) -> HttpResponse {
        let context = self.get_context_data(&HashMap::new());
        self.render_to_response_with_engine(context, self.engine.as_deref())
    }
}

/// A view that redirects to a URL. Equivalent to Django's `RedirectView`.
///
/// Supports both permanent (301) and temporary (302) redirects.
///
/// # Examples
///
/// ```
/// use django_rs_views::views::class_based::RedirectView;
///
/// let view = RedirectView::new("/new-url/");
/// let permanent = RedirectView::permanent("/permanent-url/");
/// ```
pub struct RedirectView {
    url: String,
    permanent: bool,
}

impl RedirectView {
    /// Creates a new `RedirectView` that issues a 302 temporary redirect.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            permanent: false,
        }
    }

    /// Creates a new `RedirectView` that issues a 301 permanent redirect.
    pub fn permanent(url: &str) -> Self {
        Self {
            url: url.to_string(),
            permanent: true,
        }
    }

    /// Returns the target URL for the redirect.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns whether this is a permanent redirect.
    pub fn is_permanent(&self) -> bool {
        self.permanent
    }
}

#[async_trait]
impl View for RedirectView {
    fn allowed_methods(&self) -> Vec<http::Method> {
        vec![
            http::Method::GET,
            http::Method::HEAD,
            http::Method::POST,
            http::Method::OPTIONS,
        ]
    }

    async fn get(&self, _request: HttpRequest) -> HttpResponse {
        if self.permanent {
            django_rs_http::HttpResponsePermanentRedirect::new(&self.url)
        } else {
            HttpResponseRedirect::new(&self.url)
        }
    }

    async fn post(&self, request: HttpRequest) -> HttpResponse {
        self.get(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestView;

    #[async_trait]
    impl View for TestView {
        async fn get(&self, _request: HttpRequest) -> HttpResponse {
            HttpResponse::ok("GET response")
        }

        async fn post(&self, _request: HttpRequest) -> HttpResponse {
            HttpResponse::ok("POST response")
        }
    }

    struct GetOnlyView;

    #[async_trait]
    impl View for GetOnlyView {
        fn allowed_methods(&self) -> Vec<http::Method> {
            vec![http::Method::GET, http::Method::HEAD]
        }

        async fn get(&self, _request: HttpRequest) -> HttpResponse {
            HttpResponse::ok("GET only")
        }
    }

    #[tokio::test]
    async fn test_view_dispatch_get() {
        let view = TestView;
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(response.content_bytes().unwrap(), b"GET response");
    }

    #[tokio::test]
    async fn test_view_dispatch_post() {
        let view = TestView;
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(response.content_bytes().unwrap(), b"POST response");
    }

    #[tokio::test]
    async fn test_view_dispatch_method_not_allowed() {
        let view = TestView;
        let request = HttpRequest::builder()
            .method(http::Method::DELETE)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_view_dispatch_put_not_allowed() {
        let view = TestView;
        let request = HttpRequest::builder()
            .method(http::Method::PUT)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_view_head_delegates_to_get() {
        let view = TestView;
        let request = HttpRequest::builder()
            .method(http::Method::HEAD)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(response.content_bytes().unwrap(), b"GET response");
    }

    #[tokio::test]
    async fn test_view_options() {
        let view = TestView;
        let request = HttpRequest::builder()
            .method(http::Method::OPTIONS)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let allow = response
            .headers()
            .get(http::header::ALLOW)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(allow.contains("GET"));
        assert!(allow.contains("POST"));
    }

    #[tokio::test]
    async fn test_view_allowed_methods_default() {
        let view = TestView;
        let methods = view.allowed_methods();
        assert_eq!(methods.len(), 7);
    }

    #[tokio::test]
    async fn test_view_custom_allowed_methods() {
        let view = GetOnlyView;
        let methods = view.allowed_methods();
        assert_eq!(methods.len(), 2);
    }

    #[tokio::test]
    async fn test_view_as_view() {
        let view = TestView;
        let view_fn = view.as_view();
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view_fn(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_template_view_get() {
        let view = TemplateView::new("home.html");
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("home.html"));
    }

    #[tokio::test]
    async fn test_template_view_with_context() {
        let view = TemplateView::new("home.html")
            .with_context("title", serde_json::json!("Home Page"));
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view.dispatch(request).await;
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("Home Page"));
    }

    #[tokio::test]
    async fn test_template_view_template_name() {
        let view = TemplateView::new("about.html");
        assert_eq!(view.template_name(), "about.html");
    }

    #[tokio::test]
    async fn test_template_view_get_template_names() {
        let view = TemplateView::new("about.html");
        let names = view.get_template_names();
        assert_eq!(names, vec!["about.html"]);
    }

    #[tokio::test]
    async fn test_template_view_context_mixin() {
        let view = TemplateView::new("test.html")
            .with_context("key", serde_json::json!("value"));
        let mut kwargs = HashMap::new();
        kwargs.insert("id".to_string(), "42".to_string());
        let context = view.get_context_data(&kwargs);
        assert_eq!(context.get("key").unwrap(), &serde_json::json!("value"));
        assert_eq!(context.get("id").unwrap(), &serde_json::json!("42"));
    }

    #[tokio::test]
    async fn test_redirect_view_temporary() {
        let view = RedirectView::new("/new-url/");
        let request = HttpRequest::builder()
            .method(http::Method::GET)
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
            "/new-url/"
        );
    }

    #[tokio::test]
    async fn test_redirect_view_permanent() {
        let view = RedirectView::permanent("/permanent/");
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::MOVED_PERMANENTLY);
    }

    #[tokio::test]
    async fn test_redirect_view_post() {
        let view = RedirectView::new("/new-url/");
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::FOUND);
    }

    #[tokio::test]
    async fn test_redirect_view_url() {
        let view = RedirectView::new("/target/");
        assert_eq!(view.url(), "/target/");
    }

    #[tokio::test]
    async fn test_redirect_view_is_permanent() {
        let view = RedirectView::new("/temp/");
        assert!(!view.is_permanent());
        let view = RedirectView::permanent("/perm/");
        assert!(view.is_permanent());
    }

    #[tokio::test]
    async fn test_redirect_view_allowed_methods() {
        let view = RedirectView::new("/url/");
        let methods = view.allowed_methods();
        assert!(methods.contains(&http::Method::GET));
        assert!(methods.contains(&http::Method::POST));
    }
}
