//! Redirects framework.
//!
//! Provides URL redirect management with a fallback middleware. When a view
//! returns a 404 response, the [`RedirectFallbackMiddleware`] checks if a
//! redirect is registered for the requested path and performs the redirect
//! if one is found.
//!
//! This mirrors Django's `django.contrib.redirects` framework.
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_views::contrib::redirects::{Redirect, RedirectRegistry};
//!
//! let mut registry = RedirectRegistry::new();
//! registry.register(Redirect::permanent("/old-page/", "/new-page/"));
//! registry.register(Redirect::temporary("/promo/", "/sale/"));
//!
//! let redirect = registry.get_redirect("/old-page/");
//! assert!(redirect.is_some());
//! assert_eq!(redirect.unwrap().new_path, "/new-page/");
//! assert!(redirect.unwrap().is_permanent);
//! ```

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use async_trait::async_trait;

use django_rs_core::DjangoError;
use django_rs_http::{HttpRequest, HttpResponse};

use crate::middleware::Middleware;

/// A URL redirect entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    /// The old path that should be redirected from.
    pub old_path: String,
    /// The new path to redirect to.
    pub new_path: String,
    /// Optional site ID this redirect belongs to. `None` matches all sites.
    pub site_id: Option<u64>,
    /// Whether this is a permanent (301) or temporary (302) redirect.
    pub is_permanent: bool,
}

impl Redirect {
    /// Creates a permanent redirect (301).
    pub fn permanent(old_path: impl Into<String>, new_path: impl Into<String>) -> Self {
        Self {
            old_path: old_path.into(),
            new_path: new_path.into(),
            site_id: None,
            is_permanent: true,
        }
    }

    /// Creates a temporary redirect (302).
    pub fn temporary(old_path: impl Into<String>, new_path: impl Into<String>) -> Self {
        Self {
            old_path: old_path.into(),
            new_path: new_path.into(),
            site_id: None,
            is_permanent: false,
        }
    }

    /// Sets the site ID for this redirect.
    #[must_use]
    pub fn with_site(mut self, site_id: u64) -> Self {
        self.site_id = Some(site_id);
        self
    }
}

/// An in-memory registry of redirects.
#[derive(Debug, Clone, Default)]
pub struct RedirectRegistry {
    /// Redirects indexed by old_path for quick lookup.
    redirects: HashMap<String, Vec<Redirect>>,
}

impl RedirectRegistry {
    /// Creates a new empty redirect registry.
    pub fn new() -> Self {
        Self {
            redirects: HashMap::new(),
        }
    }

    /// Registers a redirect.
    pub fn register(&mut self, redirect: Redirect) {
        self.redirects
            .entry(redirect.old_path.clone())
            .or_default()
            .push(redirect);
    }

    /// Removes all redirects for a given old path.
    pub fn unregister(&mut self, old_path: &str) -> Vec<Redirect> {
        self.redirects.remove(old_path).unwrap_or_default()
    }

    /// Looks up a redirect by old path, optionally filtering by site ID.
    pub fn get_redirect(&self, old_path: &str) -> Option<&Redirect> {
        self.redirects.get(old_path).and_then(|redirects| {
            redirects.first()
        })
    }

    /// Looks up a redirect by old path and site ID.
    ///
    /// First tries to find a site-specific redirect, then falls back to
    /// redirects without a site ID.
    pub fn get_redirect_for_site(&self, old_path: &str, site_id: u64) -> Option<&Redirect> {
        self.redirects.get(old_path).and_then(|redirects| {
            // First, try to find a site-specific redirect
            redirects
                .iter()
                .find(|r| r.site_id == Some(site_id))
                .or_else(|| {
                    // Fall back to redirects without a specific site
                    redirects.iter().find(|r| r.site_id.is_none())
                })
        })
    }

    /// Returns the total number of redirect entries.
    pub fn len(&self) -> usize {
        self.redirects.values().map(Vec::len).sum()
    }

    /// Returns `true` if no redirects are registered.
    pub fn is_empty(&self) -> bool {
        self.redirects.is_empty()
    }

    /// Clears all redirects.
    pub fn clear(&mut self) {
        self.redirects.clear();
    }
}

// ── Global registry ─────────────────────────────────────────────────────

/// Returns the global redirect registry singleton.
pub fn global_redirect_registry() -> &'static RwLock<RedirectRegistry> {
    static REGISTRY: OnceLock<RwLock<RedirectRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(RedirectRegistry::new()))
}

/// Convenience: registers a redirect in the global registry.
pub fn register_redirect(redirect: Redirect) {
    let mut registry = global_redirect_registry()
        .write()
        .expect("redirect registry lock poisoned");
    registry.register(redirect);
}

// ── Middleware ────────────────────────────────────────────────────────────

/// Middleware that checks for redirects when a view returns 404.
///
/// If the response status is 404 Not Found, the middleware checks the
/// redirect registry for a matching path. If found, it returns the
/// appropriate redirect response (301 or 302).
///
/// ## Usage
///
/// ```
/// use django_rs_views::contrib::redirects::{Redirect, RedirectFallbackMiddleware};
/// use django_rs_views::middleware::MiddlewarePipeline;
///
/// let mw = RedirectFallbackMiddleware::from_registry(vec![
///     Redirect::permanent("/old/", "/new/"),
/// ]);
///
/// let mut pipeline = MiddlewarePipeline::new();
/// pipeline.add(mw);
/// ```
pub struct RedirectFallbackMiddleware {
    registry: RedirectRegistry,
}

impl RedirectFallbackMiddleware {
    /// Creates middleware from an existing redirect registry.
    pub fn new(registry: RedirectRegistry) -> Self {
        Self { registry }
    }

    /// Creates middleware from a list of redirects.
    pub fn from_registry(redirects: Vec<Redirect>) -> Self {
        let mut registry = RedirectRegistry::new();
        for redirect in redirects {
            registry.register(redirect);
        }
        Self { registry }
    }

    /// Creates middleware that uses the global redirect registry.
    ///
    /// Note: This snapshots the registry at creation time.
    pub fn from_global() -> Self {
        let global = global_redirect_registry()
            .read()
            .expect("redirect registry lock poisoned");
        Self {
            registry: global.clone(),
        }
    }
}

#[async_trait]
impl Middleware for RedirectFallbackMiddleware {
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

        if let Some(redirect) = self.registry.get_redirect(path) {
            if redirect.is_permanent {
                let mut resp = HttpResponse::new(http::StatusCode::MOVED_PERMANENTLY, "");
                if let Ok(value) =
                    http::header::HeaderValue::from_str(&redirect.new_path)
                {
                    resp.headers_mut()
                        .insert(http::header::LOCATION, value);
                }
                return resp;
            }
            return HttpResponse::redirect(&redirect.new_path);
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
    fn test_redirect_permanent() {
        let r = Redirect::permanent("/old/", "/new/");
        assert_eq!(r.old_path, "/old/");
        assert_eq!(r.new_path, "/new/");
        assert!(r.is_permanent);
        assert!(r.site_id.is_none());
    }

    #[test]
    fn test_redirect_temporary() {
        let r = Redirect::temporary("/promo/", "/sale/");
        assert!(!r.is_permanent);
    }

    #[test]
    fn test_redirect_with_site() {
        let r = Redirect::permanent("/old/", "/new/").with_site(42);
        assert_eq!(r.site_id, Some(42));
    }

    #[test]
    fn test_registry_new_is_empty() {
        let registry = RedirectRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = RedirectRegistry::new();
        registry.register(Redirect::permanent("/old/", "/new/"));

        assert_eq!(registry.len(), 1);
        let r = registry.get_redirect("/old/").unwrap();
        assert_eq!(r.new_path, "/new/");
    }

    #[test]
    fn test_registry_get_missing() {
        let registry = RedirectRegistry::new();
        assert!(registry.get_redirect("/nonexistent/").is_none());
    }

    #[test]
    fn test_registry_multiple_same_path() {
        let mut registry = RedirectRegistry::new();
        registry.register(Redirect::permanent("/old/", "/new1/").with_site(1));
        registry.register(Redirect::permanent("/old/", "/new2/").with_site(2));

        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_registry_get_for_site() {
        let mut registry = RedirectRegistry::new();
        registry.register(Redirect::permanent("/old/", "/site1-new/").with_site(1));
        registry.register(Redirect::permanent("/old/", "/site2-new/").with_site(2));
        registry.register(Redirect::permanent("/old/", "/default-new/"));

        let r = registry.get_redirect_for_site("/old/", 1).unwrap();
        assert_eq!(r.new_path, "/site1-new/");

        let r = registry.get_redirect_for_site("/old/", 2).unwrap();
        assert_eq!(r.new_path, "/site2-new/");

        // Falls back to non-site-specific redirect
        let r = registry.get_redirect_for_site("/old/", 999).unwrap();
        assert_eq!(r.new_path, "/default-new/");
    }

    #[test]
    fn test_registry_unregister() {
        let mut registry = RedirectRegistry::new();
        registry.register(Redirect::permanent("/old/", "/new/"));
        let removed = registry.unregister("/old/");
        assert_eq!(removed.len(), 1);
        assert!(registry.get_redirect("/old/").is_none());
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = RedirectRegistry::new();
        registry.register(Redirect::permanent("/a/", "/b/"));
        registry.register(Redirect::permanent("/c/", "/d/"));
        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_redirect_equality() {
        let a = Redirect::permanent("/old/", "/new/");
        let b = Redirect::permanent("/old/", "/new/");
        assert_eq!(a, b);
    }

    #[test]
    fn test_redirect_clone() {
        let r = Redirect::permanent("/old/", "/new/").with_site(1);
        let cloned = r.clone();
        assert_eq!(r, cloned);
    }

    // ── Middleware tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_middleware_ignores_non_404() {
        let mw = RedirectFallbackMiddleware::from_registry(vec![
            Redirect::permanent("/old/", "/new/"),
        ]);

        let mut request = HttpRequest::builder().path("/old/").build();
        assert!(mw.process_request(&mut request).await.is_none());

        let response = HttpResponse::ok("ok");
        let result = mw.process_response(&request, response).await;
        assert_eq!(result.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_middleware_permanent_redirect_on_404() {
        let mw = RedirectFallbackMiddleware::from_registry(vec![
            Redirect::permanent("/old/", "/new/"),
        ]);

        let request = HttpRequest::builder().path("/old/").build();
        let response = HttpResponse::not_found("Not Found");
        let result = mw.process_response(&request, response).await;

        assert_eq!(result.status(), http::StatusCode::MOVED_PERMANENTLY);
        assert_eq!(
            result.headers().get(http::header::LOCATION).unwrap().to_str().unwrap(),
            "/new/"
        );
    }

    #[tokio::test]
    async fn test_middleware_temporary_redirect_on_404() {
        let mw = RedirectFallbackMiddleware::from_registry(vec![
            Redirect::temporary("/promo/", "/sale/"),
        ]);

        let request = HttpRequest::builder().path("/promo/").build();
        let response = HttpResponse::not_found("Not Found");
        let result = mw.process_response(&request, response).await;

        assert_eq!(result.status(), http::StatusCode::FOUND);
        assert_eq!(
            result.headers().get(http::header::LOCATION).unwrap().to_str().unwrap(),
            "/sale/"
        );
    }

    #[tokio::test]
    async fn test_middleware_no_matching_redirect() {
        let mw = RedirectFallbackMiddleware::from_registry(vec![
            Redirect::permanent("/old/", "/new/"),
        ]);

        let request = HttpRequest::builder().path("/unknown/").build();
        let response = HttpResponse::not_found("Not Found");
        let result = mw.process_response(&request, response).await;

        assert_eq!(result.status(), http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_middleware_from_empty_registry() {
        let mw = RedirectFallbackMiddleware::new(RedirectRegistry::new());

        let request = HttpRequest::builder().path("/anything/").build();
        let response = HttpResponse::not_found("Not Found");
        let result = mw.process_response(&request, response).await;

        assert_eq!(result.status(), http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_middleware_process_exception_returns_none() {
        let mw = RedirectFallbackMiddleware::new(RedirectRegistry::new());
        let request = HttpRequest::builder().build();
        let error = DjangoError::NotFound("test".into());
        assert!(mw.process_exception(&request, &error).await.is_none());
    }

    #[test]
    fn test_default_registry() {
        let registry = RedirectRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_global_registry_access() {
        let registry = global_redirect_registry();
        let _guard = registry.read().unwrap();
    }
}
