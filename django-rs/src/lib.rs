//! # django-rs
//!
//! A full-featured Django equivalent web framework for Rust.
//!
//! This is the meta-crate that re-exports all sub-crates for convenient access.
//! You can depend on `django-rs` to get the entire framework, or depend on
//! individual crates for finer-grained control.

/// Core types, settings, app registry, and error types.
pub use django_rs_core as core;

/// Procedural macros for models, views, forms, and more.
pub use django_rs_macros as macros;

/// ORM: Model definitions, `QuerySet`, Manager, and expressions.
pub use django_rs_db as db;

/// Database backends: `PostgreSQL`, `MySQL`, `SQLite`.
pub use django_rs_db_backends as db_backends;

/// Migration engine and auto-detection.
pub use django_rs_db_migrations as db_migrations;

/// HTTP layer: Request, Response, URL routing.
pub use django_rs_http as http;

/// Class-based views, generic views, and middleware.
pub use django_rs_views as views;

/// Forms, `ModelForms`, and widgets.
pub use django_rs_forms as forms;

/// DTL-compatible template engine.
pub use django_rs_template as template;

/// Authentication: Users, Permissions, Groups, Sessions.
pub use django_rs_auth as auth;

/// Auto-generated admin panel.
pub use django_rs_admin as admin;

/// Signal dispatcher for decoupled event handling.
pub use django_rs_signals as signals;

/// Management commands (CLI).
pub use django_rs_cli as cli;

/// Testing framework and utilities.
pub use django_rs_test as test;
