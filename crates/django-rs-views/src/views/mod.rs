//! View system for django-rs.
//!
//! This module provides both function-based views (FBVs) and class-based views (CBVs),
//! mirroring Django's view layer:
//!
//! - [`function`] - Function-based views and decorator patterns
//! - [`class_based`] - The `View` trait, `TemplateView`, `RedirectView`
//! - [`generic`] - Generic CRUD views (`ListView`, `DetailView`, `CreateView`, etc.)
//! - [`form_view`] - Form-view integration helpers
//! - [`archive`] - Date-based archive views (`ArchiveIndexView`, `YearArchiveView`, etc.)

pub mod archive;
pub mod class_based;
pub mod form_view;
pub mod function;
pub mod generic;

pub use archive::{
    ArchiveIndexView, DateDetailView, DateMixin, DayArchiveView, MonthArchiveView,
    TodayArchiveView, YearArchiveView,
};
pub use class_based::{ContextMixin, RedirectView, TemplateResponseMixin, TemplateView, View};
pub use form_view::{
    bind_form_from_request, cleaned_data_as_strings, extract_post_data, form_context_to_json,
    form_errors, FormView,
};
pub use function::{
    login_required, login_required_redirect, permission_required,
    require_get, require_http_methods, require_post,
    LoginRequiredMixin, PermissionRequiredMixin, ViewFunction,
};
pub use generic::{CreateView, DeleteView, DetailView, ListView, UpdateView};
