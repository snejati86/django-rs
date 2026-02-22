//! # django-rs
//!
//! A full-featured Django equivalent web framework for Rust.
//!
//! This is the meta-crate that re-exports all sub-crates for convenient access.
//! You can depend on `django-rs` to get the entire framework, or depend on
//! individual crates for finer-grained control.
//!
//! ## Quick Start
//!
//! ```toml
//! [dependencies]
//! django-rs = { version = "0.1", features = ["postgres"] }
//! ```
//!
//! ## Feature Flags
//!
//! - **`full`** *(default)* — enables all modules (macros, db, http, views, forms,
//!   template, auth, admin, signals, cli)
//! - **`postgres`** / **`sqlite`** / **`mysql`** — database backend drivers
//! - **`testing`** — testing utilities (enables sqlite for test database)
//! - Individual module features: `macros`, `db`, `db-migrations`, `http`, `views`,
//!   `forms`, `template`, `auth`, `admin`, `signals`, `cli`

// ── Always-available crates ──────────────────────────────────────────

/// Core types, settings, app registry, and error types.
pub use django_rs_core as core;

/// Database backends: base traits (always available), plus driver implementations
/// behind `postgres`, `sqlite`, and `mysql` features.
pub use django_rs_db_backends as db_backends;

// ── Feature-gated crate re-exports ───────────────────────────────────

/// Procedural macros for models, views, forms, and more.
#[cfg(feature = "macros")]
pub use django_rs_macros as macros;

/// ORM: Model definitions, `QuerySet`, Manager, and expressions.
#[cfg(feature = "db")]
pub use django_rs_db as db;

/// Migration engine and auto-detection.
#[cfg(feature = "db-migrations")]
pub use django_rs_db_migrations as db_migrations;

/// HTTP layer: Request, Response, URL routing.
#[cfg(feature = "http")]
pub use django_rs_http as http;

/// Class-based views, generic views, and middleware.
#[cfg(feature = "views")]
pub use django_rs_views as views;

/// Forms, `ModelForms`, and widgets.
#[cfg(feature = "forms")]
pub use django_rs_forms as forms;

/// DTL-compatible template engine.
#[cfg(feature = "template")]
pub use django_rs_template as template;

/// Authentication: Users, Permissions, Groups, Sessions.
#[cfg(feature = "auth")]
pub use django_rs_auth as auth;

/// Auto-generated admin panel.
#[cfg(feature = "admin")]
pub use django_rs_admin as admin;

/// Signal dispatcher for decoupled event handling.
#[cfg(feature = "signals")]
pub use django_rs_signals as signals;

/// Management commands (CLI).
#[cfg(feature = "cli")]
pub use django_rs_cli as cli;

/// Testing framework and utilities.
#[cfg(feature = "testing")]
pub use django_rs_test as test;

// ── Third-party re-exports ───────────────────────────────────────────

pub use async_trait;
pub use axum;
pub use chrono;
pub use serde;
pub use serde_json;
pub use tokio;
pub use tower_http;
pub use tracing;
pub use tracing_subscriber;

// ── Prelude ──────────────────────────────────────────────────────────

/// Commonly used types re-exported for convenience.
///
/// ```rust
/// use django_rs::prelude::*;
/// ```
pub mod prelude {
    // ── Core ──
    pub use django_rs_core::{DjangoError, DjangoResult, Settings, ValidationError, SETTINGS};

    // ── Database backends (base traits, always available) ──
    pub use django_rs_db_backends::{DatabaseBackend, DatabaseConfig};

    #[cfg(feature = "mysql")]
    pub use django_rs_db_backends::MySqlBackend;
    #[cfg(feature = "postgres")]
    pub use django_rs_db_backends::PostgresBackend;
    #[cfg(feature = "sqlite")]
    pub use django_rs_db_backends::SqliteBackend;

    // ── ORM ──
    #[cfg(feature = "db")]
    pub use django_rs_db::{
        atomic, DbExecutor, FieldDef, FieldType, Manager, Model, ModelMeta, QuerySet, Row, Value, Q,
    };

    // ── HTTP ──
    #[cfg(feature = "http")]
    pub use django_rs_http::{HttpRequest, HttpResponse, JsonResponse, QueryDict};

    // ── Views ──
    #[cfg(feature = "views")]
    pub use django_rs_views::{
        CreateView, DeleteView, DetailView, DjangoApp, ListView, Middleware, MiddlewarePipeline,
        TemplateView, UpdateView, View,
    };

    // ── Forms ──
    #[cfg(feature = "forms")]
    pub use django_rs_forms::{BaseForm, Form, FormFieldDef, FormFieldType};

    // ── Templates ──
    #[cfg(feature = "template")]
    pub use django_rs_template::{Context, ContextValue, Engine};

    // ── Auth ──
    #[cfg(feature = "auth")]
    pub use django_rs_auth::{
        authenticate, check_password, login, logout, make_password, AbstractUser, CsrfMiddleware,
    };

    // ── Signals ──
    #[cfg(feature = "signals")]
    pub use django_rs_signals::{Signal, SIGNALS};

    // ── Macros (derive/proc macros) ──
    #[cfg(feature = "macros")]
    pub use django_rs_macros::{Admin, Form as FormDerive, Model as ModelDerive};

    // ── Third-party essentials ──
    pub use async_trait::async_trait;
    pub use serde::{Deserialize, Serialize};
}
