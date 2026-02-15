//! # django-rs-views
//!
//! View layer for the django-rs framework. Provides class-based views (CBV),
//! function-based views (FBV), generic views (`ListView`, `DetailView`, `CreateView`,
//! etc.), middleware pipeline, session framework, and HTTP server integration.
//!
//! ## Modules
//!
//! - [`middleware`] - Middleware trait and pipeline, built-in middleware components
//! - [`views`] - Function-based views, class-based views, and generic CRUD views
//! - [`session`] - Session framework with pluggable backends
//! - [`server`] - HTTP server integration via Axum
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_views::middleware::MiddlewarePipeline;
//! use django_rs_views::middleware::builtin::SecurityMiddleware;
//! use django_rs_views::views::class_based::{View, TemplateView};
//!
//! // Create a middleware pipeline
//! let mut pipeline = MiddlewarePipeline::new();
//! pipeline.add(SecurityMiddleware::default());
//!
//! // Create a class-based view
//! let view = TemplateView::new("home.html");
//! let view_fn = view.as_view();
//! ```

// These clippy lints are intentionally allowed for the views crate:
// - result_large_err: DjangoError is the framework error type
// - new_ret_no_self: Factory types like JsonResponse return a different type
// - too_many_lines: Some middleware implementations are inherently detailed
// - doc_markdown: backtick requirements for documentation items are too strict
#![allow(clippy::result_large_err)]
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::unnecessary_literal_bound)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::option_if_let_else)]

pub mod middleware;
pub mod server;
pub mod session;
pub mod views;

// Re-export the most commonly used types at the crate root.
pub use middleware::{Middleware, MiddlewarePipeline};
pub use server::DjangoApp;
pub use session::{
    CookieSessionBackend, InMemorySessionBackend, SessionBackend, SessionData, SessionMiddleware,
};
pub use views::{
    ContextMixin, CreateView, DeleteView, DetailView, ListView, RedirectView, TemplateView,
    TemplateResponseMixin, UpdateView, View, ViewFunction,
    bind_form_from_request, cleaned_data_as_strings, extract_post_data, form_context_to_json,
    form_errors, login_required_redirect, permission_required,
    LoginRequiredMixin, PermissionRequiredMixin,
};
