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
//! - [`constraints`] - Database constraints (CHECK, UNIQUE)
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
// literal_string_with_formatting_args: template strings using {column}/{value} are intentional
#![allow(clippy::literal_string_with_formatting_args)]
// significant_drop_tightening: false positives with async Mutex guards
#![allow(clippy::significant_drop_tightening)]

pub mod constraints;
pub mod executor;
pub mod fields;
pub mod model;
pub mod query;
pub mod router;
pub mod transactions;
pub mod validators;
pub mod value;

// Re-export the most commonly used types at the crate root.
pub use constraints::{CheckConstraint, Constraint, UniqueConstraint};
pub use executor::{
    create_model, create_model_with_hooks, delete_model, delete_model_with_hooks, refresh_model,
    save_model, save_model_with_hooks, DbExecutor, ModelLifecycleHooks,
};
pub use fields::{FieldDef, FieldType, OnDelete};
pub use model::{BloomIndex, BrinIndex, GinIndex, GistIndex, Index, IndexType, SpGistIndex};
pub use model::{Model, ModelMeta};
pub use query::expressions::search::{
    SearchQuery, SearchQueryType, SearchRank, SearchVector, TrigramSimilarity,
};
pub use query::{
    AggregateFunc, CompoundQuery, CompoundType, DatabaseBackendType, Exists, Expression,
    InheritanceType, Lookup, Manager, OrderBy, OuterRef, PrefetchRelatedField, PrefetchResult,
    Query, QuerySet, Row, SelectColumn, SelectRelatedField, SqlCompiler, SubqueryExpression, When,
    WhereNode, WindowExpression, WindowFrame, WindowFrameBound, WindowFrameType, WindowFunction, Q,
};
pub use router::{DatabaseEntry, DatabaseRouter, DatabasesConfig, RouterChain};
pub use validators::Validator;
pub use value::Value;

// Re-export new modules at the crate root for convenience.
pub use query::bulk::{
    bulk_create, bulk_update, get_or_create, update_or_create, BulkCreateOptions, BulkUpdateOptions,
};
pub use query::custom_lookups::{CustomLookup, LookupRegistry, Transform, TransformOutput};
pub use query::raw::{RawQuerySet, RawSql};
pub use transactions::{
    atomic, atomic_with_isolation, IsolationLevel, Savepoint, TransactionManager,
};
