//! View system for django-rs.
//!
//! This module provides both function-based views (FBVs) and class-based views (CBVs),
//! mirroring Django's view layer:
//!
//! - [`function`] - Function-based views and decorator patterns
//! - [`class_based`] - The `View` trait, `TemplateView`, `RedirectView`
//! - [`generic`] - Generic CRUD views (`ListView`, `DetailView`, `CreateView`, etc.)

pub mod class_based;
pub mod function;
pub mod generic;

pub use class_based::{ContextMixin, RedirectView, TemplateResponseMixin, TemplateView, View};
pub use function::{
    login_required, require_get, require_http_methods, require_post, ViewFunction,
};
pub use generic::{CreateView, DeleteView, DetailView, ListView, UpdateView};
