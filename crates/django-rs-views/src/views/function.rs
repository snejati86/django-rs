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

use django_rs_http::{HttpRequest, HttpResponse};

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
/// version of Django's `@login_required` decorator.
///
/// In a full implementation, this would redirect to the login URL, but for now
/// it returns a 403 to indicate the user is not authenticated.
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
}
