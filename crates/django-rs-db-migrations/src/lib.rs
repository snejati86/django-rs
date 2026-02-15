//! # django-rs-db-migrations
//!
//! Migration engine for the django-rs framework. Handles schema migration generation,
//! auto-detection of model changes, migration application, and rollback operations.
//!
//! ## Architecture
//!
//! The migration system mirrors Django's migration framework:
//!
//! - [`Migration`] is a named set of [`Operation`]s belonging to an app.
//! - [`MigrationGraph`] resolves dependency ordering across apps.
//! - [`MigrationAutodetector`] diffs two [`ProjectState`]s to produce operations.
//! - [`SchemaEditor`] translates operations into backend-specific DDL.
//! - [`MigrationExecutor`] applies or reverts a plan of migrations.
//! - [`MigrationSquasher`] combines migrations into an optimized single migration.
//!
//! ## Module Overview
//!
//! - [`migration`] - `Migration`, `MigrationGraph`
//! - [`loader`] - `MigrationLoader` for filesystem discovery
//! - [`operations`] - `Operation` trait and all concrete operations
//! - [`schema_editor`] - `SchemaEditor` trait and PostgreSQL/SQLite/MySQL implementations
//! - [`executor`] - `MigrationExecutor`, `MigrationPlan`, `MigrationRecorder`
//! - [`autodetect`] - `MigrationAutodetector`, `ProjectState`, `ModelState`
//! - [`squash`] - `MigrationSquasher`

// Clippy overrides appropriate for a DDL generation / migration crate.
#![allow(clippy::too_many_lines)]
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
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::cognitive_complexity)]

pub mod autodetect;
pub mod executor;
pub mod loader;
pub mod migration;
pub mod operations;
pub mod schema_editor;
pub mod squash;

// Re-export key types at the crate root.
pub use autodetect::{MigrationAutodetector, ModelOptions, ModelState, ProjectState};
pub use executor::{MigrationExecutor, MigrationPlan, MigrationRecorder, MigrationStep};
pub use loader::MigrationLoader;
pub use migration::{Migration, MigrationGraph};
pub use operations::Operation;
pub use schema_editor::{
    MySqlSchemaEditor, PostgresSchemaEditor, SchemaEditor, SqliteSchemaEditor,
};
pub use squash::MigrationSquasher;
