//! # django-rs-db
//!
//! ORM layer for the django-rs framework. Provides the [`Model`](model::Model) trait
//! for defining database models, [`QuerySet`](query::QuerySet) for building and executing
//! database queries, [`Manager`](query::Manager) for model-level operations, and
//! expression types for constructing complex queries.
//!
//! ## Architecture
//!
//! The ORM is designed around lazy evaluation. A [`QuerySet`](query::QuerySet) builds
//! a [`Query`](query::Query) AST through method chaining without touching the database.
//! SQL is only generated when a terminal method (`.get()`, `.count()`, `.first()`, etc.)
//! is called, at which point the [`SqlCompiler`](query::SqlCompiler) translates the AST
//! into parameterized SQL appropriate for the target backend.
//!
//! ## Module Overview
//!
//! - [`model`] - The [`Model`](model::Model) trait and [`ModelMeta`](model::ModelMeta)
//! - [`fields`] - Field definitions ([`FieldDef`](fields::FieldDef)) and types
//! - [`value`] - The backend-agnostic [`Value`](value::Value) enum
//! - [`query`] - Query building, lookups, expressions, and compilation
//! - [`validators`] - Field validators

// These clippy lints are intentionally allowed for the ORM crate:
// - struct_excessive_bools: FieldDef mirrors Django's field API which uses many booleans
// - too_many_lines: The SQL compiler methods are inherently large due to many match arms
// - cast_precision_loss: i64-to-f64 casts are acceptable for validator comparisons
// - result_large_err: DjangoError is the framework error type and should be used consistently
// - format_push_string: format! with push_str is clearer than write! for SQL generation
// - doc_markdown: backtick requirements for documentation items are too strict
// - needless_pass_by_value: some API signatures match Django's patterns
// - return_self_not_must_use: builder pattern methods are self-documenting
// - use_self: explicit type names are clearer in some contexts
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::result_large_err)]
#![allow(clippy::format_push_string)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::use_self)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::unnecessary_literal_bound)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::single_char_pattern)]

pub mod fields;
pub mod model;
pub mod query;
pub mod validators;
pub mod value;

// Re-export the most commonly used types at the crate root.
pub use fields::{FieldDef, FieldType, OnDelete};
pub use model::{Model, ModelMeta};
pub use query::{
    AggregateFunc, DatabaseBackendType, Expression, Lookup, Manager, OrderBy, Q, Query, QuerySet,
    Row, SelectColumn, SqlCompiler, When, WhereNode,
};
pub use validators::Validator;
pub use value::Value;
