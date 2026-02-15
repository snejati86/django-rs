//! Query building, compilation, and execution.
//!
//! This module contains the complete query pipeline:
//!
//! - [`lookups`] - Q objects and lookup types for filtering
//! - [`expressions`] - F-objects, aggregates, and computed expressions
//! - [`compiler`] - Query AST and SQL compilation
//! - [`queryset`] - QuerySet and Manager for lazy query building
//! - [`raw`] - Raw SQL query support
//! - [`bulk`] - Bulk create, bulk update, get_or_create, update_or_create
//! - [`custom_lookups`] - Custom lookup and transform registry

pub mod bulk;
pub mod compiler;
pub mod custom_lookups;
pub mod expressions;
pub mod lookups;
pub mod queryset;
pub mod raw;

pub use compiler::{DatabaseBackendType, OrderBy, Query, Row, SelectColumn, SqlCompiler, WhereNode};
pub use expressions::{AggregateFunc, Expression, When};
pub use lookups::{Lookup, Q};
pub use queryset::{Manager, QuerySet};
