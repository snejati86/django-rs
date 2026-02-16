//! HTTP server integration for django-rs.
//!
//! This module provides [`DjangoApp`], the main application builder that combines
//! URL routing, middleware, and settings into a runnable web server. It integrates
//! with Axum to provide the actual HTTP server implementation.
//!
//! This mirrors Django's `manage.py runserver` and the WSGI/ASGI application setup.
//!
//! # Examples
//!
//! ```no_run
//! use django_rs_views::server::DjangoApp;
//! use django_rs_core::Settings;
//! use django_rs_http::urls::resolver::{root, URLEntry};
//! use django_rs_http::urls::pattern::path;
//! use django_rs_http::{HttpRequest, HttpResponse};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let handler = Arc::new(|_req: HttpRequest| -> django_rs_http::BoxFuture {
//!     Box::pin(async { HttpResponse::ok("Hello!") })
//! });
//!
//! let patterns = vec![
//!     URLEntry::Pattern(path("", handler, Some("home")).unwrap()),
//! ];
//! let resolver = root(patterns).unwrap();
//!
//! let app = DjangoApp::new(Settings::default())
//!     .urls(resolver);
//!
//! // app.run("0.0.0.0:8000").await?;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::response::IntoResponse;
use axum::routing::any;

use django_rs_core::{DjangoError, Settings};
use django_rs_http::urls::resolver::URLResolver;
use django_rs_http::{HttpRequest, HttpResponse};
use django_rs_template::engine::Engine;

use crate::middleware::{Middleware, MiddlewarePipeline, ViewHandler};

/// The main application type for django-rs.
///
/// `DjangoApp` combines a URL resolver, middleware pipeline, settings, and a
/// template engine into a single application that can be converted to an Axum
/// router or run directly as an HTTP server.
///
/// This mirrors Django's application setup in `wsgi.py` or `asgi.py`.
pub struct DjangoApp {
    url_conf: Option<URLResolver>,
    middleware: MiddlewarePipeline,
    settings: Settings,
    engine: Option<Arc<Engine>>,
}

impl DjangoApp {
    /// Creates a new `DjangoApp` with the given settings.
    pub fn new(settings: Settings) -> Self {
        Self {
            url_conf: None,
            middleware: MiddlewarePipeline::new(),
            settings,
            engine: None,
        }
    }

    /// Sets the URL configuration for this application.
    #[must_use]
    pub fn urls(mut self, url_conf: URLResolver) -> Self {
        self.url_conf = Some(url_conf);
        self
    }

    /// Adds a middleware to the application's pipeline.
    #[must_use]
    pub fn middleware(mut self, middleware: impl Middleware + 'static) -> Self {
        self.middleware.add(middleware);
        self
    }

    /// Sets the template engine for this application.
    #[must_use]
    pub fn engine(mut self, engine: Engine) -> Self {
        self.engine = Some(Arc::new(engine));
        self
    }

    /// Returns a reference to the application settings.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Returns a reference to the template engine, if configured.
    pub fn template_engine(&self) -> Option<&Arc<Engine>> {
        self.engine.as_ref()
    }

    /// Returns `true` if URL configuration has been set.
    pub fn has_urls(&self) -> bool {
        self.url_conf.is_some()
    }

    /// Returns the number of middleware in the pipeline.
    pub fn middleware_count(&self) -> usize {
        self.middleware.len()
    }

    /// Converts the application into an Axum router.
    ///
    /// The router handles all incoming requests by running them through the
    /// middleware pipeline and URL resolver.
    pub fn into_axum_router(self) -> axum::Router {
        let url_conf = Arc::new(self.url_conf);
        let middleware = Arc::new(self.middleware);
        let settings = Arc::new(self.settings);

        let handler = move |req: Request<Body>| {
            let url_conf = url_conf.clone();
            let middleware = middleware.clone();
            let settings = settings.clone();

            async move {
                let (parts, body) = req.into_parts();
                let body_bytes = axum::body::to_bytes(body, usize::MAX)
                    .await
                    .unwrap_or_default()
                    .to_vec();

                let django_request = HttpRequest::from_axum(parts, body_bytes);

                let view_handler: ViewHandler = Box::new(move |mut request: HttpRequest| {
                    let url_conf = url_conf.clone();
                    let _settings = settings.clone();

                    Box::pin(async move {
                        let Some(url_conf) = url_conf.as_ref() else {
                            return HttpResponse::server_error("No URL configuration provided");
                        };

                        // Strip leading slash for URL resolution. Django's URL
                        // patterns don't include a leading slash (e.g. "articles/"
                        // not "/articles/"), but HTTP request paths always start
                        // with "/". This mirrors Django's WSGIHandler behavior.
                        let path = request.path().to_string();
                        let path = path.strip_prefix('/').unwrap_or(&path);
                        match url_conf.resolve(path) {
                            Ok(resolver_match) => {
                                request.set_resolver_match(resolver_match.clone());
                                let handler = &resolver_match.func;
                                handler(request).await
                            }
                            Err(DjangoError::NotFound(msg)) => HttpResponse::not_found(msg),
                            Err(e) => HttpResponse::server_error(format!("Routing error: {e}")),
                        }
                    })
                        as std::pin::Pin<Box<dyn std::future::Future<Output = HttpResponse> + Send>>
                });

                let response = middleware.process(django_request, &view_handler).await;
                response.into_response()
            }
        };

        axum::Router::new()
            .route("/{*path}", any(handler.clone()))
            .route("/", any(handler))
    }

    /// Runs the application as an HTTP server on the given address.
    ///
    /// This starts a Tokio-based HTTP server using Axum.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to bind to the address or encounters
    /// a runtime error.
    pub async fn run(self, addr: &str) -> Result<(), DjangoError> {
        let debug = self.settings.debug;
        let router = self.into_axum_router();
        let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
            DjangoError::ImproperlyConfigured(format!("Failed to bind to {addr}: {e}"))
        })?;

        if debug {
            tracing::info!("Starting development server at http://{addr}/");
        }

        axum::serve(listener, router)
            .await
            .map_err(|e| DjangoError::InternalServerError(format!("Server error: {e}")))?;

        Ok(())
    }
}

impl std::fmt::Debug for DjangoApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DjangoApp")
            .field("has_urls", &self.url_conf.is_some())
            .field("middleware_count", &self.middleware.len())
            .field("has_engine", &self.engine.is_some())
            .field("debug", &self.settings.debug)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_django_app_new() {
        let app = DjangoApp::new(Settings::default());
        assert!(!app.has_urls());
        assert_eq!(app.middleware_count(), 0);
        assert!(app.settings().debug);
    }

    #[test]
    fn test_django_app_with_urls() {
        let resolver = django_rs_http::urls::resolver::root(vec![]).unwrap();
        let app = DjangoApp::new(Settings::default()).urls(resolver);
        assert!(app.has_urls());
    }

    #[test]
    fn test_django_app_with_middleware() {
        use crate::middleware::builtin::SecurityMiddleware;

        let app = DjangoApp::new(Settings::default()).middleware(SecurityMiddleware::default());
        assert_eq!(app.middleware_count(), 1);
    }

    #[test]
    fn test_django_app_chained_middleware() {
        use crate::middleware::builtin::{CommonMiddleware, SecurityMiddleware};

        let app = DjangoApp::new(Settings::default())
            .middleware(SecurityMiddleware::default())
            .middleware(CommonMiddleware::default());
        assert_eq!(app.middleware_count(), 2);
    }

    #[test]
    fn test_django_app_settings() {
        let settings = Settings {
            debug: false,
            secret_key: "test-secret".to_string(),
            ..Settings::default()
        };
        let app = DjangoApp::new(settings);
        assert!(!app.settings().debug);
        assert_eq!(app.settings().secret_key, "test-secret");
    }

    #[test]
    fn test_django_app_debug() {
        let app = DjangoApp::new(Settings::default());
        let debug = format!("{app:?}");
        assert!(debug.contains("DjangoApp"));
        assert!(debug.contains("debug"));
    }

    #[test]
    fn test_django_app_into_axum_router() {
        let resolver = django_rs_http::urls::resolver::root(vec![]).unwrap();
        let app = DjangoApp::new(Settings::default()).urls(resolver);
        let _router = app.into_axum_router();
        // Verify it compiles and creates a router
    }

    #[test]
    fn test_django_app_into_axum_router_no_urls() {
        let app = DjangoApp::new(Settings::default());
        let _router = app.into_axum_router();
        // Should still create a router (will return 500 for missing URL conf)
    }

    #[tokio::test]
    async fn test_django_app_run_invalid_address() {
        let app = DjangoApp::new(Settings::default());
        let result = app.run("invalid-address").await;
        assert!(result.is_err());
    }
}
