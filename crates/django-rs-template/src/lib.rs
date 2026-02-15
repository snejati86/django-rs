//! # django-rs-template
//!
//! A Django Template Language (DTL) compatible template engine for the django-rs
//! framework. This crate provides a standalone template engine with no dependency
//! on Tera or other external template engines.
//!
//! ## Features
//!
//! - **DTL-compatible syntax**: `{{ variables }}`, `{% tags %}`, `{# comments #}`
//! - **Template inheritance**: `{% extends %}`, `{% block %}`, `{{ block.super }}`
//! - **40+ built-in filters**: `lower`, `upper`, `truncatechars`, `date`, etc.
//! - **20+ built-in tags**: `if`, `for`, `with`, `include`, `csrf_token`, etc.
//! - **Auto-escaping**: HTML entities escaped by default, `safe` filter to bypass
//! - **Context processors**: Automatically inject variables from request data
//! - **Template loaders**: Load from filesystem, app directories, or strings
//! - **Fragment caching**: Cache rendered template fragments
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_template::engine::Engine;
//! use django_rs_template::context::{Context, ContextValue};
//!
//! let engine = Engine::new();
//! engine.add_string_template("hello.html", "Hello {{ name|upper }}!");
//!
//! let mut ctx = Context::new();
//! ctx.set("name", ContextValue::from("world"));
//!
//! let result = engine.render_to_string("hello.html", &mut ctx).unwrap();
//! assert_eq!(result, "Hello WORLD!");
//! ```
//!
//! ## Template Inheritance
//!
//! ```
//! use django_rs_template::engine::Engine;
//! use django_rs_template::context::{Context, ContextValue};
//!
//! let engine = Engine::new();
//! engine.add_string_template("base.html",
//!     "<html>{% block content %}default{% endblock %}</html>");
//! engine.add_string_template("page.html",
//!     r#"{% extends "base.html" %}{% block content %}Hello!{% endblock %}"#);
//!
//! let mut ctx = Context::new();
//! let result = engine.render_to_string("page.html", &mut ctx).unwrap();
//! assert_eq!(result, "<html>Hello!</html>");
//! ```

// Allow large error type (DjangoError is shared across the project).
#![allow(clippy::result_large_err)]
// These clippy lints are intentionally suppressed for this crate:
// - needless_pass_by_value: Filter/Tag traits require owned values in many places
// - cast_possible_truncation/wrap/sign_loss: Template values bridge between i64 and usize
// - cast_precision_loss: i64 -> f64 is acceptable for template numeric values
// - redundant_closure_for_method_calls: Clarity is preferred in some filter lambdas
// - unnested_or_patterns: Pattern clarity preferred over nesting
// - option_if_let_else: if-let is often clearer than map_or
// - needless_raw_string_hashes: Raw strings used for template content in tests
#![allow(
    clippy::needless_pass_by_value,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::redundant_closure_for_method_calls,
    clippy::unnested_or_patterns,
    clippy::option_if_let_else,
    clippy::needless_raw_string_hashes,
    clippy::missing_const_for_fn,
    clippy::use_self,
    clippy::map_unwrap_or,
    clippy::format_push_string,
    clippy::float_cmp,
    clippy::assigning_clones,
    clippy::if_same_then_else,
    clippy::match_same_arms,
    clippy::unnecessary_wraps,
    clippy::similar_names,
    clippy::doc_markdown,
    clippy::approx_constant,
    clippy::if_not_else,
    clippy::must_use_candidate,
    clippy::branches_sharing_code,
    clippy::unused_peekable
)]

pub mod context;
pub mod context_processors;
pub mod engine;
pub mod filters;
pub mod include;
pub mod inheritance;
pub mod lexer;
pub mod library;
pub mod loaders;
pub mod parser;
pub mod tags;

// Re-export the most commonly used types.
pub use context::{Context, ContextValue};
pub use engine::Engine;
pub use library::{Library, LibraryRegistry};
