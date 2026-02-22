//! # django-rs-db-backends
//!
//! Database backend implementations for the django-rs framework. Provides connection
//! pooling, query execution, and transaction management for multiple database engines.
//!
//! ## Supported backends
//!
//! - **PostgreSQL** via [`postgresql::PostgresBackend`] (`tokio-postgres` + `deadpool-postgres`)
//! - **SQLite** via [`sqlite::SqliteBackend`] (`rusqlite` with async wrappers)
//! - **MySQL** via [`mysql::MySqlBackend`] (`mysql_async`)
//!
//! ## Architecture
//!
//! All backends implement the [`DatabaseBackend`](base::DatabaseBackend) trait, which
//! provides a uniform interface for executing SQL, managing transactions, and obtaining
//! a SQL compiler for the backend's dialect.

// These clippy lints are intentionally allowed for the backends crate:
// - result_large_err: DjangoError is the framework error type
// - cast_precision_loss: numeric conversions between DB types are acceptable
// - cast_possible_wrap: u64 to i64 conversions for MySQL row counts
// - cast_sign_loss: signed-unsigned conversions for MySQL compatibility
// - doc_markdown: backtick requirements for doc items are too strict
// - needless_pass_by_value: some API signatures are dictated by the trait
// - return_self_not_must_use: builder methods
// - use_self: explicit names in some contexts
#![allow(clippy::result_large_err)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::use_self)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::unnecessary_literal_bound)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::assigning_clones)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::cast_lossless)]

pub mod base;
#[cfg(feature = "mysql")]
pub mod mysql;
#[cfg(feature = "postgres")]
pub mod postgresql;
#[cfg(feature = "sqlite")]
pub mod sqlite;

pub use base::{DatabaseBackend, DatabaseConfig, Transaction};
pub use django_rs_db::DbExecutor;
#[cfg(feature = "mysql")]
pub use mysql::MySqlBackend;
#[cfg(feature = "postgres")]
pub use postgresql::PostgresBackend;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteBackend;
