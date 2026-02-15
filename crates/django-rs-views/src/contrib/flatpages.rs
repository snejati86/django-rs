//! Flatpages framework.
//!
//! Provides simple flat page serving with an in-memory registry and fallback
//! middleware. When a view returns a 404 response, the [`FlatpageFallbackMiddleware`]
//! checks if a flatpage is registered for the requested URL and renders it.
//!
//! This mirrors Django's `django.contrib.flatpages` framework.
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_views::contrib::flatpages::{FlatPage, FlatPageRegistry};
//!
//! let mut registry = FlatPageRegistry::new();
//! registry.register(FlatPage::new("/about/", "About Us", "<h1>About</h1>"));
//!
//! let page = registry.get_by_url("/about/");
//! assert!(page.is_some());
//! assert_eq!(page.unwrap().title, "About Us");
//! ```

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use async_trait::async_trait;

use django_rs_core::DjangoError;
use django_rs_http::{HttpRequest, HttpResponse};

use crate::middleware::Middleware;

/// A flat page entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatPage {
    /// The URL path for this page (e.g., "/about/").
    pub url: String,
    /// The page title.
    pub title: String,
    /// The page content (HTML).
    pub content: String,
    /// The template name to use for rendering. Defaults to `"flatpages/default.html"`.
    pub template_name: String,
    /// The site IDs this page belongs to. Empty means all sites.
    pub sites: Vec<u64>,
    /// Whether this page requires authentication.
    pub registration_required: bool,
}

impl FlatPage {
    /// Creates a new flat page with default template.
    pub fn new(
        url: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            title: title.into(),
            content: content.into(),
            template_name: "flatpages/default.html".to_string(),
            sites: Vec::new(),
            registration_required: false,
        }
    }

    /// Sets the template name for this page.
    #[must_use]
    pub fn with_template(mut self, template_name: impl Into<String>) -> Self {
        self.template_name = template_name.into();
        self
    }

    /// Sets the site IDs for this page.
    #[must_use]
    pub fn with_sites(mut self, sites: Vec<u64>) -> Self {
        self.sites = sites;
        self
    }

    /// Sets whether this page requires authentication.
    #[must_use]
    pub fn with_registration_required(mut self, required: bool) -> Self {
        self.registration_required = required;
        self
    }
}

/// An in-memory registry of flat pages.
#[derive(Debug, Clone, Default)]
pub struct FlatPageRegistry {
    pages: HashMap<String, FlatPage>,
}

impl FlatPageRegistry {
    /// Creates a new empty flat page registry.
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
        }
    }

    /// Registers a flat page.
    pub fn register(&mut self, page: FlatPage) {
        self.pages.insert(page.url.clone(), page);
    }

    /// Removes a flat page by URL.
    pub fn unregister(&mut self, url: &str) -> Option<FlatPage> {
        self.pages.remove(url)
    }

    /// Looks up a flat page by URL.
    pub fn get_by_url(&self, url: &str) -> Option<&FlatPage> {
        self.pages.get(url)
    }

    /// Looks up a flat page by URL filtered by site ID.
    ///
    /// Returns the page if it matches the URL and either has no site
    /// restriction or includes the given site ID.
    pub fn get_by_url_for_site(&self, url: &str, site_id: u64) -> Option<&FlatPage> {
        self.pages.get(url).filter(|page| {
            page.sites.is_empty() || page.sites.contains(&site_id)
        })
    }

    /// Returns the number of registered pages.
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Returns `true` if no pages are registered.
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// Returns all registered pages.
    pub fn all(&self) -> Vec<&FlatPage> {
        self.pages.values().collect()
    }

    /// Clears all registered pages.
    pub fn clear(&mut self) {
        self.pages.clear();
    }
}

// ── Global registry ─────────────────────────────────────────────────────

/// Returns the global flat page registry singleton.
pub fn global_flatpage_registry() -> &'static RwLock<FlatPageRegistry> {
    static REGISTRY: OnceLock<RwLock<FlatPageRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(FlatPageRegistry::new()))
}

/// Convenience: registers a flat page in the global registry.
pub fn register_flatpage(page: FlatPage) {
    let mut registry = global_flatpage_registry()
        .write()
        .expect("flatpage registry lock poisoned");
    registry.register(page);
}

// ── View ─────────────────────────────────────────────────────────────────

/// Renders a flat page as an HTML response.
///
/// This creates a simple HTML page with the flatpage's title and content.
/// In a production system, this would use the template engine with the
/// flatpage's `template_name`.
pub fn render_flatpage(page: &FlatPage) -> HttpResponse {
    let html = format!(
        "<!DOCTYPE html>\n<html>\n<head><title>{title}</title></head>\n\
         <body>\n<h1>{title}</h1>\n{content}\n</body>\n</html>",
        title = html_escape(&page.title),
        content = page.content,
    );
    let mut response = HttpResponse::ok(html);
    response.set_content_type("text/html");
    response
}

/// Basic HTML escaping for title text.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

// ── Middleware ────────────────────────────────────────────────────────────

/// Middleware that serves flat pages when a view returns 404.
///
/// If the response status is 404 Not Found, the middleware checks the
/// flatpage registry for a matching URL. If found, it renders the
/// flatpage and returns it as a 200 OK response.
///
/// ## Usage
///
/// ```
/// use django_rs_views::contrib::flatpages::{FlatPage, FlatpageFallbackMiddleware};
/// use django_rs_views::middleware::MiddlewarePipeline;
///
/// let mw = FlatpageFallbackMiddleware::from_pages(vec![
///     FlatPage::new("/about/", "About Us", "<p>About content.</p>"),
/// ]);
///
/// let mut pipeline = MiddlewarePipeline::new();
/// pipeline.add(mw);
/// ```
pub struct FlatpageFallbackMiddleware {
    registry: FlatPageRegistry,
}

impl FlatpageFallbackMiddleware {
    /// Creates middleware from an existing registry.
    pub fn new(registry: FlatPageRegistry) -> Self {
        Self { registry }
    }

    /// Creates middleware from a list of flat pages.
    pub fn from_pages(pages: Vec<FlatPage>) -> Self {
        let mut registry = FlatPageRegistry::new();
        for page in pages {
            registry.register(page);
        }
        Self { registry }
    }

    /// Creates middleware that uses the global flatpage registry.
    ///
    /// Note: This snapshots the registry at creation time.
    pub fn from_global() -> Self {
        let global = global_flatpage_registry()
            .read()
            .expect("flatpage registry lock poisoned");
        Self {
            registry: global.clone(),
        }
    }
}

#[async_trait]
impl Middleware for FlatpageFallbackMiddleware {
    async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
        None
    }

    async fn process_response(
        &self,
        request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        // Only intercept 404 responses
        if response.status() != http::StatusCode::NOT_FOUND {
            return response;
        }

        let path = request.path();

        if let Some(page) = self.registry.get_by_url(path) {
            // Check authentication requirement
            if page.registration_required {
                let is_authenticated = request
                    .meta()
                    .get("USER_IS_AUTHENTICATED")
                    .is_some_and(|v| v == "true");
                if !is_authenticated {
                    return response;
                }
            }
            return render_flatpage(page);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flatpage_new() {
        let page = FlatPage::new("/about/", "About", "<p>Hello</p>");
        assert_eq!(page.url, "/about/");
        assert_eq!(page.title, "About");
        assert_eq!(page.content, "<p>Hello</p>");
        assert_eq!(page.template_name, "flatpages/default.html");
        assert!(page.sites.is_empty());
        assert!(!page.registration_required);
    }

    #[test]
    fn test_flatpage_with_template() {
        let page = FlatPage::new("/test/", "Test", "content")
            .with_template("custom/template.html");
        assert_eq!(page.template_name, "custom/template.html");
    }

    #[test]
    fn test_flatpage_with_sites() {
        let page = FlatPage::new("/test/", "Test", "content")
            .with_sites(vec![1, 2, 3]);
        assert_eq!(page.sites, vec![1, 2, 3]);
    }

    #[test]
    fn test_flatpage_with_registration_required() {
        let page = FlatPage::new("/test/", "Test", "content")
            .with_registration_required(true);
        assert!(page.registration_required);
    }

    #[test]
    fn test_registry_new_is_empty() {
        let registry = FlatPageRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = FlatPageRegistry::new();
        registry.register(FlatPage::new("/about/", "About", "content"));

        assert_eq!(registry.len(), 1);
        let page = registry.get_by_url("/about/").unwrap();
        assert_eq!(page.title, "About");
    }

    #[test]
    fn test_registry_get_missing() {
        let registry = FlatPageRegistry::new();
        assert!(registry.get_by_url("/missing/").is_none());
    }

    #[test]
    fn test_registry_overwrite() {
        let mut registry = FlatPageRegistry::new();
        registry.register(FlatPage::new("/page/", "Old", "old content"));
        registry.register(FlatPage::new("/page/", "New", "new content"));

        assert_eq!(registry.len(), 1);
        let page = registry.get_by_url("/page/").unwrap();
        assert_eq!(page.title, "New");
    }

    #[test]
    fn test_registry_unregister() {
        let mut registry = FlatPageRegistry::new();
        registry.register(FlatPage::new("/about/", "About", "content"));
        let removed = registry.unregister("/about/");
        assert!(removed.is_some());
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_get_by_url_for_site() {
        let mut registry = FlatPageRegistry::new();
        registry.register(
            FlatPage::new("/about/", "About", "content")
                .with_sites(vec![1, 2]),
        );

        assert!(registry.get_by_url_for_site("/about/", 1).is_some());
        assert!(registry.get_by_url_for_site("/about/", 2).is_some());
        assert!(registry.get_by_url_for_site("/about/", 3).is_none());
    }

    #[test]
    fn test_registry_get_by_url_for_site_no_restriction() {
        let mut registry = FlatPageRegistry::new();
        registry.register(FlatPage::new("/about/", "About", "content"));

        // No site restriction means matches all sites
        assert!(registry.get_by_url_for_site("/about/", 1).is_some());
        assert!(registry.get_by_url_for_site("/about/", 999).is_some());
    }

    #[test]
    fn test_registry_all() {
        let mut registry = FlatPageRegistry::new();
        registry.register(FlatPage::new("/a/", "A", "a"));
        registry.register(FlatPage::new("/b/", "B", "b"));

        assert_eq!(registry.all().len(), 2);
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = FlatPageRegistry::new();
        registry.register(FlatPage::new("/a/", "A", "a"));
        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_render_flatpage() {
        let page = FlatPage::new("/about/", "About Us", "<p>Content</p>");
        let response = render_flatpage(&page);

        assert_eq!(response.status(), http::StatusCode::OK);
        let body = response.content_bytes().unwrap();
        let body_str = String::from_utf8(body).unwrap();
        assert!(body_str.contains("<title>About Us</title>"));
        assert!(body_str.contains("<h1>About Us</h1>"));
        assert!(body_str.contains("<p>Content</p>"));
    }

    #[test]
    fn test_render_flatpage_escapes_title() {
        let page = FlatPage::new("/xss/", "Title <script>", "content");
        let response = render_flatpage(&page);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(!body.contains("<script>"));
        assert!(body.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<div>"), "&lt;div&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(html_escape("it's"), "it&#x27;s");
    }

    #[test]
    fn test_flatpage_equality() {
        let a = FlatPage::new("/about/", "About", "content");
        let b = FlatPage::new("/about/", "About", "content");
        assert_eq!(a, b);
    }

    #[test]
    fn test_flatpage_clone() {
        let page = FlatPage::new("/about/", "About", "content").with_sites(vec![1]);
        let cloned = page.clone();
        assert_eq!(page, cloned);
    }

    // ── Middleware tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_middleware_ignores_non_404() {
        let mw = FlatpageFallbackMiddleware::from_pages(vec![
            FlatPage::new("/about/", "About", "content"),
        ]);

        let request = HttpRequest::builder().path("/about/").build();
        let response = HttpResponse::ok("ok");
        let result = mw.process_response(&request, response).await;
        assert_eq!(result.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_middleware_serves_flatpage_on_404() {
        let mw = FlatpageFallbackMiddleware::from_pages(vec![
            FlatPage::new("/about/", "About Us", "<p>About content</p>"),
        ]);

        let request = HttpRequest::builder().path("/about/").build();
        let response = HttpResponse::not_found("Not Found");
        let result = mw.process_response(&request, response).await;

        assert_eq!(result.status(), http::StatusCode::OK);
        let body = String::from_utf8(result.content_bytes().unwrap()).unwrap();
        assert!(body.contains("About Us"));
        assert!(body.contains("<p>About content</p>"));
    }

    #[tokio::test]
    async fn test_middleware_no_matching_flatpage() {
        let mw = FlatpageFallbackMiddleware::from_pages(vec![
            FlatPage::new("/about/", "About", "content"),
        ]);

        let request = HttpRequest::builder().path("/unknown/").build();
        let response = HttpResponse::not_found("Not Found");
        let result = mw.process_response(&request, response).await;

        assert_eq!(result.status(), http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_middleware_registration_required_not_authenticated() {
        let mw = FlatpageFallbackMiddleware::from_pages(vec![
            FlatPage::new("/private/", "Private", "secret")
                .with_registration_required(true),
        ]);

        let request = HttpRequest::builder().path("/private/").build();
        let response = HttpResponse::not_found("Not Found");
        let result = mw.process_response(&request, response).await;

        // Should remain 404 because user is not authenticated
        assert_eq!(result.status(), http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_middleware_registration_required_authenticated() {
        let mw = FlatpageFallbackMiddleware::from_pages(vec![
            FlatPage::new("/private/", "Private", "secret content")
                .with_registration_required(true),
        ]);

        let request = HttpRequest::builder()
            .path("/private/")
            .meta("USER_IS_AUTHENTICATED", "true")
            .build();
        let response = HttpResponse::not_found("Not Found");
        let result = mw.process_response(&request, response).await;

        assert_eq!(result.status(), http::StatusCode::OK);
        let body = String::from_utf8(result.content_bytes().unwrap()).unwrap();
        assert!(body.contains("secret content"));
    }

    #[tokio::test]
    async fn test_middleware_process_request_returns_none() {
        let mw = FlatpageFallbackMiddleware::new(FlatPageRegistry::new());
        let mut request = HttpRequest::builder().build();
        assert!(mw.process_request(&mut request).await.is_none());
    }

    #[tokio::test]
    async fn test_middleware_process_exception_returns_none() {
        let mw = FlatpageFallbackMiddleware::new(FlatPageRegistry::new());
        let request = HttpRequest::builder().build();
        let error = DjangoError::NotFound("test".into());
        assert!(mw.process_exception(&request, &error).await.is_none());
    }

    #[test]
    fn test_default_registry() {
        let registry = FlatPageRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_global_registry_access() {
        let registry = global_flatpage_registry();
        let _guard = registry.read().unwrap();
    }
}
