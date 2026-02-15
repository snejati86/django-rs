//! Middleware framework for django-rs.
//!
//! This module provides the [`Middleware`] trait and [`MiddlewarePipeline`] for
//! processing requests and responses. It mirrors Django's middleware system where
//! middleware components can intercept requests before they reach the view and
//! responses before they are sent to the client.
//!
//! ## Middleware Execution Order
//!
//! Middleware is processed in order for requests (first added = first to process)
//! and in reverse order for responses (first added = last to process). This
//! matches Django's "onion" model.

pub mod builtin;

use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;

use django_rs_core::DjangoError;
use django_rs_http::{HttpRequest, HttpResponse};

/// The type for an async view handler function used in the pipeline.
pub type ViewHandler =
    Box<dyn Fn(HttpRequest) -> Pin<Box<dyn Future<Output = HttpResponse> + Send>> + Send + Sync>;

/// A middleware component that can process requests and responses.
///
/// Middleware is the Django-rs equivalent of Django's middleware classes.
/// Each middleware can:
/// - Inspect or modify the request before it reaches the view (`process_request`)
/// - Inspect or modify the response after the view returns (`process_response`)
/// - Handle exceptions raised during view processing (`process_exception`)
///
/// # Examples
///
/// ```
/// use async_trait::async_trait;
/// use django_rs_views::middleware::{Middleware, MiddlewarePipeline};
/// use django_rs_http::{HttpRequest, HttpResponse};
/// use django_rs_core::DjangoError;
///
/// struct LoggingMiddleware;
///
/// #[async_trait]
/// impl Middleware for LoggingMiddleware {
///     async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
///         None // Allow request to continue
///     }
///
///     async fn process_response(&self, _request: &HttpRequest, response: HttpResponse) -> HttpResponse {
///         response
///     }
///
///     async fn process_exception(&self, _request: &HttpRequest, _error: &DjangoError) -> Option<HttpResponse> {
///         None
///     }
/// }
/// ```
#[async_trait]
pub trait Middleware: Send + Sync {
    /// Process an incoming request before it reaches the view.
    ///
    /// Return `Some(HttpResponse)` to short-circuit the pipeline and skip the
    /// view. Return `None` to allow the request to continue to the next
    /// middleware and eventually the view.
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse>;

    /// Process the response after the view has been called.
    ///
    /// This is called in reverse middleware order (last added = first to
    /// process the response).
    async fn process_response(
        &self,
        request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse;

    /// Handle an exception that occurred during view processing.
    ///
    /// Return `Some(HttpResponse)` to provide a custom error response.
    /// Return `None` to let the default error handling proceed.
    async fn process_exception(
        &self,
        request: &HttpRequest,
        error: &DjangoError,
    ) -> Option<HttpResponse>;
}

/// A pipeline of middleware components that processes requests and responses.
///
/// The pipeline runs middleware in order for requests and in reverse order
/// for responses, implementing Django's "onion" model of middleware processing.
///
/// # Examples
///
/// ```
/// use django_rs_views::middleware::MiddlewarePipeline;
/// use django_rs_views::middleware::builtin::SecurityMiddleware;
///
/// let mut pipeline = MiddlewarePipeline::new();
/// pipeline.add(SecurityMiddleware::default());
/// ```
pub struct MiddlewarePipeline {
    middlewares: Vec<Box<dyn Middleware>>,
}

impl Default for MiddlewarePipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl MiddlewarePipeline {
    /// Creates a new empty middleware pipeline.
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }

    /// Adds a middleware to the end of the pipeline.
    pub fn add(&mut self, middleware: impl Middleware + 'static) {
        self.middlewares.push(Box::new(middleware));
    }

    /// Returns the number of middleware components in the pipeline.
    pub fn len(&self) -> usize {
        self.middlewares.len()
    }

    /// Returns `true` if the pipeline has no middleware components.
    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }

    /// Processes a request through the full middleware pipeline and view handler.
    ///
    /// 1. Calls `process_request` on each middleware in order. If any returns
    ///    `Some(response)`, short-circuits and runs `process_response` in reverse
    ///    on only the middleware that already ran.
    /// 2. Calls the view handler with a rebuilt request.
    /// 3. Calls `process_response` on each middleware in reverse order.
    pub async fn process(
        &self,
        mut request: HttpRequest,
        handler: &ViewHandler,
    ) -> HttpResponse {
        // Phase 1: process_request (forward order)
        for (i, mw) in self.middlewares.iter().enumerate() {
            if let Some(response) = mw.process_request(&mut request).await {
                // Short-circuit: run process_response on already-processed middleware
                let mut resp = response;
                for j in (0..=i).rev() {
                    resp = self.middlewares[j]
                        .process_response(&request, resp)
                        .await;
                }
                return resp;
            }
        }

        // Phase 2: call the view handler
        // Build the handler request from the current (possibly modified) request state
        let handler_request = rebuild_request(&request);
        let response = handler(handler_request).await;

        // Phase 3: process_response (reverse order)
        let mut resp = response;
        for mw in self.middlewares.iter().rev() {
            resp = mw.process_response(&request, resp).await;
        }

        resp
    }
}

/// Rebuilds an `HttpRequest` from an existing one to pass ownership to the handler.
///
/// This creates a new request with the same method, path, query string, headers,
/// and metadata as the original.
fn rebuild_request(request: &HttpRequest) -> HttpRequest {
    let mut builder = HttpRequest::builder()
        .method(request.method().clone())
        .path(request.path())
        .query_string(request.query_string())
        .scheme(request.scheme())
        .body(request.body().to_vec());

    if let Some(ct) = request.content_type() {
        builder = builder.content_type(ct);
    }

    for (name, value) in request.headers() {
        if let Ok(v) = value.to_str() {
            builder = builder.header(name.as_str(), v);
        }
    }

    for (key, value) in request.meta() {
        builder = builder.meta(key, value);
    }

    let mut req = builder.build();
    if let Some(resolver_match) = request.resolver_match() {
        req.set_resolver_match(resolver_match.clone());
    }
    req
}

impl std::fmt::Debug for MiddlewarePipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiddlewarePipeline")
            .field("middleware_count", &self.middlewares.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct OrderTracker {
        name: String,
        request_order: Arc<AtomicUsize>,
        response_order: Arc<AtomicUsize>,
        request_log: Arc<std::sync::Mutex<Vec<String>>>,
        response_log: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Middleware for OrderTracker {
        async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
            let order = self.request_order.fetch_add(1, Ordering::SeqCst);
            self.request_log
                .lock()
                .unwrap()
                .push(format!("{}:{order}", self.name));
            None
        }

        async fn process_response(
            &self,
            _request: &HttpRequest,
            response: HttpResponse,
        ) -> HttpResponse {
            let order = self.response_order.fetch_add(1, Ordering::SeqCst);
            self.response_log
                .lock()
                .unwrap()
                .push(format!("{}:{order}", self.name));
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

    struct ShortCircuitMiddleware;

    #[async_trait]
    impl Middleware for ShortCircuitMiddleware {
        async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
            Some(HttpResponse::forbidden("Blocked"))
        }

        async fn process_response(
            &self,
            _request: &HttpRequest,
            response: HttpResponse,
        ) -> HttpResponse {
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

    struct PassthroughMiddleware;

    #[async_trait]
    impl Middleware for PassthroughMiddleware {
        async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
            None
        }

        async fn process_response(
            &self,
            _request: &HttpRequest,
            response: HttpResponse,
        ) -> HttpResponse {
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

    struct HeaderAddingMiddleware {
        header_name: &'static str,
        header_value: &'static str,
    }

    #[async_trait]
    impl Middleware for HeaderAddingMiddleware {
        async fn process_request(&self, _request: &mut HttpRequest) -> Option<HttpResponse> {
            None
        }

        async fn process_response(
            &self,
            _request: &HttpRequest,
            response: HttpResponse,
        ) -> HttpResponse {
            response.set_header(
                http::header::HeaderName::from_static(self.header_name),
                http::header::HeaderValue::from_static(self.header_value),
            )
        }

        async fn process_exception(
            &self,
            _request: &HttpRequest,
            _error: &DjangoError,
        ) -> Option<HttpResponse> {
            None
        }
    }

    fn make_handler() -> ViewHandler {
        Box::new(|_req| Box::pin(async { HttpResponse::ok("view response") }))
    }

    #[tokio::test]
    async fn test_pipeline_new_is_empty() {
        let pipeline = MiddlewarePipeline::new();
        assert!(pipeline.is_empty());
        assert_eq!(pipeline.len(), 0);
    }

    #[tokio::test]
    async fn test_pipeline_add_middleware() {
        let mut pipeline = MiddlewarePipeline::new();
        pipeline.add(PassthroughMiddleware);
        assert_eq!(pipeline.len(), 1);
        assert!(!pipeline.is_empty());
    }

    #[tokio::test]
    async fn test_pipeline_no_middleware() {
        let pipeline = MiddlewarePipeline::new();
        let handler = make_handler();
        let request = HttpRequest::builder().build();
        let response = pipeline.process(request, &handler).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_pipeline_passthrough_middleware() {
        let mut pipeline = MiddlewarePipeline::new();
        pipeline.add(PassthroughMiddleware);
        let handler = make_handler();
        let request = HttpRequest::builder().build();
        let response = pipeline.process(request, &handler).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_pipeline_short_circuit() {
        let mut pipeline = MiddlewarePipeline::new();
        pipeline.add(ShortCircuitMiddleware);
        let handler = make_handler();
        let request = HttpRequest::builder().build();
        let response = pipeline.process(request, &handler).await;
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_pipeline_middleware_ordering() {
        let request_order = Arc::new(AtomicUsize::new(0));
        let response_order = Arc::new(AtomicUsize::new(0));
        let request_log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let response_log = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut pipeline = MiddlewarePipeline::new();
        pipeline.add(OrderTracker {
            name: "A".to_string(),
            request_order: request_order.clone(),
            response_order: response_order.clone(),
            request_log: request_log.clone(),
            response_log: response_log.clone(),
        });
        pipeline.add(OrderTracker {
            name: "B".to_string(),
            request_order: request_order.clone(),
            response_order: response_order.clone(),
            request_log: request_log.clone(),
            response_log: response_log.clone(),
        });
        pipeline.add(OrderTracker {
            name: "C".to_string(),
            request_order,
            response_order,
            request_log: request_log.clone(),
            response_log: response_log.clone(),
        });

        let handler = make_handler();
        let request = HttpRequest::builder().build();
        pipeline.process(request, &handler).await;

        let req_log = request_log.lock().unwrap();
        assert_eq!(req_log[0], "A:0");
        assert_eq!(req_log[1], "B:1");
        assert_eq!(req_log[2], "C:2");

        let resp_log = response_log.lock().unwrap();
        // Response order is reversed
        assert_eq!(resp_log[0], "C:0");
        assert_eq!(resp_log[1], "B:1");
        assert_eq!(resp_log[2], "A:2");
    }

    #[tokio::test]
    async fn test_pipeline_short_circuit_only_runs_processed_middleware() {
        let response_log = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut pipeline = MiddlewarePipeline::new();
        pipeline.add(OrderTracker {
            name: "A".to_string(),
            request_order: Arc::new(AtomicUsize::new(0)),
            response_order: Arc::new(AtomicUsize::new(0)),
            request_log: Arc::new(std::sync::Mutex::new(Vec::new())),
            response_log: response_log.clone(),
        });
        pipeline.add(ShortCircuitMiddleware);
        pipeline.add(OrderTracker {
            name: "C".to_string(),
            request_order: Arc::new(AtomicUsize::new(0)),
            response_order: Arc::new(AtomicUsize::new(0)),
            request_log: Arc::new(std::sync::Mutex::new(Vec::new())),
            response_log: response_log.clone(),
        });

        let handler = make_handler();
        let request = HttpRequest::builder().build();
        let response = pipeline.process(request, &handler).await;

        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);

        // C should not have processed the response since it was never reached
        let resp_log = response_log.lock().unwrap();
        assert_eq!(resp_log.len(), 1); // Only A
        assert!(resp_log[0].starts_with("A:"));
    }

    #[tokio::test]
    async fn test_pipeline_default() {
        let pipeline = MiddlewarePipeline::default();
        assert!(pipeline.is_empty());
    }

    #[tokio::test]
    async fn test_pipeline_debug() {
        let mut pipeline = MiddlewarePipeline::new();
        pipeline.add(PassthroughMiddleware);
        let debug = format!("{pipeline:?}");
        assert!(debug.contains("middleware_count"));
        assert!(debug.contains('1'));
    }

    #[tokio::test]
    async fn test_pipeline_multiple_middleware() {
        let mut pipeline = MiddlewarePipeline::new();
        pipeline.add(PassthroughMiddleware);
        pipeline.add(PassthroughMiddleware);
        pipeline.add(PassthroughMiddleware);
        assert_eq!(pipeline.len(), 3);
    }

    #[tokio::test]
    async fn test_pipeline_response_headers_added_by_middleware() {
        let mut pipeline = MiddlewarePipeline::new();
        pipeline.add(HeaderAddingMiddleware {
            header_name: "x-custom",
            header_value: "test-value",
        });

        let handler = make_handler();
        let request = HttpRequest::builder().build();
        let response = pipeline.process(request, &handler).await;

        assert_eq!(
            response.headers().get("x-custom").unwrap().to_str().unwrap(),
            "test-value"
        );
    }

    #[tokio::test]
    async fn test_rebuild_request_preserves_method() {
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .path("/test/")
            .query_string("key=value")
            .build();
        let rebuilt = rebuild_request(&request);
        assert_eq!(rebuilt.method(), &http::Method::POST);
        assert_eq!(rebuilt.path(), "/test/");
        assert_eq!(rebuilt.query_string(), "key=value");
    }

    #[tokio::test]
    async fn test_rebuild_request_preserves_headers() {
        let request = HttpRequest::builder()
            .header("x-test", "hello")
            .build();
        let rebuilt = rebuild_request(&request);
        assert_eq!(
            rebuilt.headers().get("x-test").unwrap().to_str().unwrap(),
            "hello"
        );
    }
}
