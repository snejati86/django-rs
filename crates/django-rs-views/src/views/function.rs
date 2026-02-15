//! Function-based views and decorator patterns for django-rs.
//!
//! This module provides the [`ViewFunction`] type alias and "decorator" functions
//! that wrap view functions with additional behavior, mirroring Django's function-based
//! view decorators like `@require_http_methods`, `@require_GET`, `@login_required`.
//!
//! # Examples
//!
//! ```
//! use django_rs_views::views::function::{ViewFunction, require_get, require_post};
//! use django_rs_http::{HttpRequest, HttpResponse};
//!
//! let my_view: ViewFunction = Box::new(|_req| {
//!     Box::pin(async { HttpResponse::ok("Hello!") })
//! });
//!
//! // Wrap with require_GET decorator
//! let get_only = require_get(my_view);
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use django_rs_http::{HttpRequest, HttpResponse, HttpResponseRedirect};

/// The type for an async view function.
///
/// A view function takes an `HttpRequest` and returns a future that resolves
/// to an `HttpResponse`. This is the Rust equivalent of a Django view function.
pub type ViewFunction =
    Box<dyn Fn(HttpRequest) -> Pin<Box<dyn Future<Output = HttpResponse> + Send>> + Send + Sync>;

/// Wraps a view function to only allow the specified HTTP methods.
///
/// If the request method is not in the allowed list, returns a 405 Method Not Allowed
/// response. This mirrors Django's `@require_http_methods` decorator.
///
/// # Examples
///
/// ```
/// use django_rs_views::views::function::{ViewFunction, require_http_methods};
/// use django_rs_http::{HttpRequest, HttpResponse};
///
/// let my_view: ViewFunction = Box::new(|_req| {
///     Box::pin(async { HttpResponse::ok("Hello!") })
/// });
///
/// let restricted = require_http_methods(&["GET", "POST"], my_view);
/// ```
pub fn require_http_methods(methods: &[&str], view: ViewFunction) -> ViewFunction {
    let allowed: Vec<String> = methods.iter().map(|m| m.to_uppercase()).collect();
    let allowed_for_header: Vec<String> = allowed.clone();
    let view = Arc::new(view);

    Box::new(move |request: HttpRequest| {
        let allowed = allowed.clone();
        let allowed_for_header = allowed_for_header.clone();
        let view = view.clone();

        Box::pin(async move {
            let method = request.method().to_string();
            if allowed.iter().any(|m| m == &method) {
                view(request).await
            } else {
                let method_strs: Vec<&str> = allowed_for_header.iter().map(String::as_str).collect();
                HttpResponse::not_allowed(&method_strs)
            }
        })
    })
}

/// Wraps a view function to only allow GET and HEAD requests.
///
/// This mirrors Django's `@require_GET` decorator.
pub fn require_get(view: ViewFunction) -> ViewFunction {
    require_http_methods(&["GET", "HEAD"], view)
}

/// Wraps a view function to only allow POST requests.
///
/// This mirrors Django's `@require_POST` decorator.
pub fn require_post(view: ViewFunction) -> ViewFunction {
    require_http_methods(&["POST"], view)
}

/// Wraps a view function to require an authenticated user.
///
/// Checks the request's META for a `USER_AUTHENTICATED` flag. If not present
/// or set to `"false"`, returns a 403 Forbidden response. This is a simplified
/// version of Django's `@login_required` decorator that does not redirect.
///
/// For redirect-based behavior, use [`login_required_redirect`] instead.
pub fn login_required(view: ViewFunction) -> ViewFunction {
    let view = Arc::new(view);

    Box::new(move |request: HttpRequest| {
        let view = view.clone();

        Box::pin(async move {
            let is_authenticated = request
                .meta()
                .get("USER_AUTHENTICATED")
                .is_some_and(|v| v == "true");

            if is_authenticated {
                view(request).await
            } else {
                HttpResponse::forbidden("Login required")
            }
        })
    })
}

/// Wraps a view function to redirect unauthenticated users to a login URL.
///
/// If the user is not authenticated (based on the `USER_AUTHENTICATED` META key),
/// they are redirected to `login_url` with a query parameter indicating where to
/// return after login. The parameter name defaults to `"next"` but can be
/// customized via `redirect_field_name`.
///
/// This mirrors Django's `@login_required` decorator with its redirect behavior.
///
/// # Examples
///
/// ```
/// use django_rs_views::views::function::{ViewFunction, login_required_redirect};
/// use django_rs_http::{HttpRequest, HttpResponse};
///
/// let my_view: ViewFunction = Box::new(|_req| {
///     Box::pin(async { HttpResponse::ok("Protected content") })
/// });
///
/// let protected = login_required_redirect("/accounts/login/", "next", my_view);
/// ```
pub fn login_required_redirect(
    login_url: &str,
    redirect_field_name: &str,
    view: ViewFunction,
) -> ViewFunction {
    let login_url = login_url.to_string();
    let redirect_field_name = redirect_field_name.to_string();
    let view = Arc::new(view);

    Box::new(move |request: HttpRequest| {
        let login_url = login_url.clone();
        let redirect_field_name = redirect_field_name.clone();
        let view = view.clone();

        Box::pin(async move {
            let is_authenticated = request
                .meta()
                .get("USER_AUTHENTICATED")
                .is_some_and(|v| v == "true");

            if is_authenticated {
                view(request).await
            } else {
                // Build redirect URL with "next" parameter pointing to current path
                let current_path = request.get_full_path();
                let redirect_url = if login_url.contains('?') {
                    format!("{login_url}&{redirect_field_name}={current_path}")
                } else {
                    format!("{login_url}?{redirect_field_name}={current_path}")
                };
                HttpResponseRedirect::new(&redirect_url)
            }
        })
    })
}

/// Wraps a view function to require a specific permission.
///
/// Checks the request's META for `USER_PERMISSIONS` (a comma-separated list)
/// and verifies the user has the required permission. Unauthenticated users
/// are redirected to the login URL.
///
/// This mirrors Django's `@permission_required` decorator.
pub fn permission_required(
    perm: &str,
    login_url: &str,
    view: ViewFunction,
) -> ViewFunction {
    let perm = perm.to_string();
    let login_url = login_url.to_string();
    let view = Arc::new(view);

    Box::new(move |request: HttpRequest| {
        let perm = perm.clone();
        let login_url = login_url.clone();
        let view = view.clone();

        Box::pin(async move {
            let is_authenticated = request
                .meta()
                .get("USER_AUTHENTICATED")
                .is_some_and(|v| v == "true");

            if !is_authenticated {
                let current_path = request.get_full_path();
                let redirect_url = format!("{login_url}?next={current_path}");
                return HttpResponseRedirect::new(&redirect_url);
            }

            // Check permission from META
            let has_permission = request
                .meta()
                .get("USER_PERMISSIONS")
                .is_some_and(|perms| {
                    perms.split(',').any(|p| p.trim() == perm)
                });

            let is_superuser = request
                .meta()
                .get("USER_IS_SUPERUSER")
                .is_some_and(|v| v == "true");

            if has_permission || is_superuser {
                view(request).await
            } else {
                HttpResponse::forbidden("Permission denied")
            }
        })
    })
}

/// Trait for class-based views that require authentication.
///
/// Implementing this trait on a view ensures that only authenticated users
/// can access it. Unauthenticated users are redirected to the login URL.
///
/// This mirrors Django's `LoginRequiredMixin`.
pub trait LoginRequiredMixin {
    /// Returns the login URL to redirect unauthenticated users to.
    fn login_url(&self) -> &str {
        "/accounts/login/"
    }

    /// Returns the name of the query parameter for the redirect URL.
    fn redirect_field_name(&self) -> &str {
        "next"
    }

    /// Checks whether the request is from an authenticated user.
    fn check_login(&self, request: &HttpRequest) -> Option<HttpResponse> {
        let is_authenticated = request
            .meta()
            .get("USER_AUTHENTICATED")
            .is_some_and(|v| v == "true");

        if is_authenticated {
            None
        } else {
            let current_path = request.get_full_path();
            let login_url = self.login_url();
            let field = self.redirect_field_name();
            let redirect_url = format!("{login_url}?{field}={current_path}");
            Some(HttpResponseRedirect::new(&redirect_url))
        }
    }
}

/// Trait for class-based views that require specific permissions.
///
/// This mirrors Django's `PermissionRequiredMixin`.
pub trait PermissionRequiredMixin: LoginRequiredMixin {
    /// Returns the required permission string.
    fn permission_required(&self) -> &str;

    /// Checks whether the request's user has the required permission.
    fn check_permission(&self, request: &HttpRequest) -> Option<HttpResponse> {
        // First check login
        if let Some(response) = self.check_login(request) {
            return Some(response);
        }

        let perm = self.permission_required();
        let has_permission = request
            .meta()
            .get("USER_PERMISSIONS")
            .is_some_and(|perms| perms.split(',').any(|p| p.trim() == perm));

        let is_superuser = request
            .meta()
            .get("USER_IS_SUPERUSER")
            .is_some_and(|v| v == "true");

        if has_permission || is_superuser {
            None
        } else {
            Some(HttpResponse::forbidden("Permission denied"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_view() -> ViewFunction {
        Box::new(|_req| Box::pin(async { HttpResponse::ok("success") }))
    }

    #[tokio::test]
    async fn test_require_http_methods_allowed() {
        let view = require_http_methods(&["GET", "POST"], make_view());
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_require_http_methods_allowed_post() {
        let view = require_http_methods(&["GET", "POST"], make_view());
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_require_http_methods_not_allowed() {
        let view = require_http_methods(&["GET"], make_view());
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_require_http_methods_not_allowed_has_allow_header() {
        let view = require_http_methods(&["GET", "POST"], make_view());
        let request = HttpRequest::builder()
            .method(http::Method::DELETE)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
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
    async fn test_require_get_allows_get() {
        let view = require_get(make_view());
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_require_get_allows_head() {
        let view = require_get(make_view());
        let request = HttpRequest::builder()
            .method(http::Method::HEAD)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_require_get_blocks_post() {
        let view = require_get(make_view());
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_require_post_allows_post() {
        let view = require_post(make_view());
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_require_post_blocks_get() {
        let view = require_post(make_view());
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_login_required_authenticated() {
        let view = login_required(make_view());
        let request = HttpRequest::builder()
            .meta("USER_AUTHENTICATED", "true")
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_login_required_not_authenticated() {
        let view = login_required(make_view());
        let request = HttpRequest::builder().build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_login_required_explicitly_not_authenticated() {
        let view = login_required(make_view());
        let request = HttpRequest::builder()
            .meta("USER_AUTHENTICATED", "false")
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_require_http_methods_case_insensitive() {
        let view = require_http_methods(&["get"], make_view());
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_chained_decorators() {
        let view = login_required(require_get(make_view()));

        // Authenticated GET should work
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .meta("USER_AUTHENTICATED", "true")
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_chained_decorators_unauthenticated() {
        let view = login_required(require_get(make_view()));

        // Unauthenticated should fail
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
    }

    // ── login_required_redirect tests ─────────────────────────────────

    #[tokio::test]
    async fn test_login_required_redirect_authenticated() {
        let view = login_required_redirect("/accounts/login/", "next", make_view());
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .meta("USER_AUTHENTICATED", "true")
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_login_required_redirect_unauthenticated() {
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
        assert_eq!(location, "/accounts/login/?next=/protected/page/");
    }

    #[tokio::test]
    async fn test_login_required_redirect_with_query_string() {
        let view = login_required_redirect("/accounts/login/", "next", make_view());
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .path("/protected/")
            .query_string("tab=settings")
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::FOUND);
        let location = response
            .headers()
            .get(http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.contains("next=/protected/?tab=settings"));
    }

    #[tokio::test]
    async fn test_login_required_redirect_custom_field_name() {
        let view = login_required_redirect("/login/", "redirect_to", make_view());
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .path("/dashboard/")
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::FOUND);
        let location = response
            .headers()
            .get(http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(location, "/login/?redirect_to=/dashboard/");
    }

    #[tokio::test]
    async fn test_login_required_redirect_explicitly_not_authenticated() {
        let view = login_required_redirect("/accounts/login/", "next", make_view());
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .path("/protected/")
            .meta("USER_AUTHENTICATED", "false")
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::FOUND);
    }

    // ── permission_required tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_permission_required_has_perm() {
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
    async fn test_permission_required_no_perm() {
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
    async fn test_permission_required_superuser() {
        let view = permission_required("blog.delete_post", "/accounts/login/", make_view());
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .meta("USER_AUTHENTICATED", "true")
            .meta("USER_IS_SUPERUSER", "true")
            .build();
        let response = view(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_permission_required_unauthenticated_redirects() {
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

    // ── LoginRequiredMixin tests ──────────────────────────────────────

    struct TestLoginView;
    impl LoginRequiredMixin for TestLoginView {}

    #[test]
    fn test_login_mixin_authenticated() {
        let view = TestLoginView;
        let request = HttpRequest::builder()
            .meta("USER_AUTHENTICATED", "true")
            .build();
        assert!(view.check_login(&request).is_none());
    }

    #[test]
    fn test_login_mixin_unauthenticated() {
        let view = TestLoginView;
        let request = HttpRequest::builder()
            .path("/protected/")
            .build();
        let response = view.check_login(&request);
        assert!(response.is_some());
        let response = response.unwrap();
        assert_eq!(response.status(), http::StatusCode::FOUND);
    }

    #[test]
    fn test_login_mixin_default_url() {
        let view = TestLoginView;
        assert_eq!(view.login_url(), "/accounts/login/");
        assert_eq!(view.redirect_field_name(), "next");
    }

    struct CustomLoginView;
    impl LoginRequiredMixin for CustomLoginView {
        fn login_url(&self) -> &str {
            "/custom/login/"
        }
        fn redirect_field_name(&self) -> &str {
            "return_to"
        }
    }

    #[test]
    fn test_login_mixin_custom_url() {
        let view = CustomLoginView;
        let request = HttpRequest::builder()
            .path("/secret/")
            .build();
        let response = view.check_login(&request).unwrap();
        assert_eq!(response.status(), http::StatusCode::FOUND);
        let location = response
            .headers()
            .get(http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.contains("/custom/login/"));
        assert!(location.contains("return_to=/secret/"));
    }

    // ── PermissionRequiredMixin tests ─────────────────────────────────

    struct TestPermView;
    impl LoginRequiredMixin for TestPermView {}
    impl PermissionRequiredMixin for TestPermView {
        fn permission_required(&self) -> &str {
            "blog.add_post"
        }
    }

    #[test]
    fn test_perm_mixin_has_perm() {
        let view = TestPermView;
        let request = HttpRequest::builder()
            .meta("USER_AUTHENTICATED", "true")
            .meta("USER_PERMISSIONS", "blog.add_post")
            .build();
        assert!(view.check_permission(&request).is_none());
    }

    #[test]
    fn test_perm_mixin_no_perm() {
        let view = TestPermView;
        let request = HttpRequest::builder()
            .meta("USER_AUTHENTICATED", "true")
            .meta("USER_PERMISSIONS", "blog.change_post")
            .build();
        let response = view.check_permission(&request).unwrap();
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_perm_mixin_unauthenticated() {
        let view = TestPermView;
        let request = HttpRequest::builder()
            .path("/blog/create/")
            .build();
        let response = view.check_permission(&request).unwrap();
        assert_eq!(response.status(), http::StatusCode::FOUND);
    }
}
