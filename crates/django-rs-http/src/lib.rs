//! # django-rs-http
//!
//! HTTP layer for the django-rs framework. Provides Request and Response types,
//! URL routing with named patterns, and the middleware pipeline built on tower.
//!
//! ## Modules
//!
//! - [`request`] - `HttpRequest` type with Django-compatible API
//! - [`response`] - `HttpResponse`, `JsonResponse`, redirect responses, and more
//! - [`querydict`] - `QueryDict` for immutable-by-default query/form parameters
//! - [`urls`] - URL pattern definitions, routing, path converters, and reverse resolution
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_http::{HttpRequest, HttpResponse, JsonResponse, QueryDict};
//! use django_rs_http::urls::pattern::path;
//! use django_rs_http::urls::resolver::{root, URLEntry};
//! use std::sync::Arc;
//!
//! // Define a handler
//! let handler = Arc::new(|req: HttpRequest| -> django_rs_http::BoxFuture {
//!     Box::pin(async {
//!         HttpResponse::ok("Hello from django-rs!")
//!     })
//! });
//!
//! // Define URL patterns
//! let patterns = vec![
//!     URLEntry::Pattern(path("hello/", handler, Some("hello")).unwrap()),
//! ];
//!
//! // Create a root resolver
//! let resolver = root(patterns).unwrap();
//!
//! // Resolve a URL
//! let m = resolver.resolve("hello/").unwrap();
//! assert_eq!(m.url_name.as_deref(), Some("hello"));
//! ```

// DjangoError is the project-wide error type and its size is by design.
#![allow(clippy::result_large_err)]
// Response factory types (JsonResponse, etc.) deliberately return HttpResponse from new().
#![allow(clippy::new_ret_no_self)]

pub mod querydict;
pub mod request;
pub mod response;
pub mod urls;

// Re-export primary types at the crate root for convenience.
pub use querydict::QueryDict;
pub use request::HttpRequest;
pub use response::{
    FileResponse, HttpResponse, HttpResponseForbidden, HttpResponseNotAllowed,
    HttpResponseNotFound, HttpResponsePermanentRedirect, HttpResponseRedirect,
    HttpResponseServerError, JsonResponse, ResponseContent, StreamingHttpResponse,
};

use std::future::Future;
use std::pin::Pin;

/// A boxed future that produces an `HttpResponse`.
///
/// This is the standard return type for route handler functions.
pub type BoxFuture = Pin<Box<dyn Future<Output = HttpResponse> + Send>>;
