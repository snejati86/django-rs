//! Query expressions, aggregates, F-objects, subqueries, window functions, and database functions.
//!
//! This module provides the [`Expression`] enum for building computed values,
//! annotations, and aggregates in queries. It mirrors Django's
//! `django.db.models.expressions` module.
//!
//! # Submodules
//!
//! - [`core`] - Core expression types: F, Value, Func, Aggregate, Case/When, arithmetic
//! - [`subquery`] - Subquery, OuterRef, Exists expressions for correlated subqueries
//! - [`window`] - Window expressions and window functions (ROW_NUMBER, RANK, etc.)
//! - [`functions`] - Database functions (Coalesce, Upper, Lower, Round, Now, Cast, etc.)
//! - [`search`] - PostgreSQL full-text search (SearchVector, SearchQuery, SearchRank, TrigramSimilarity)

pub mod core;
pub mod functions;
pub mod search;
pub mod subquery;
pub mod window;

// Re-export core types at the expressions level for backward compatibility.
pub use self::core::{AggregateFunc, Expression, When};
pub use self::functions::*;
pub use self::search::{
    SearchQuery, SearchQueryType, SearchRank, SearchVector, TrigramSimilarity,
};
pub use self::subquery::{Exists, OuterRef, SubqueryExpression};
pub use self::window::{
    WindowExpression, WindowFrame, WindowFrameBound, WindowFrameType, WindowFunction,
};
