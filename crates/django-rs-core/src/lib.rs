//! # django-rs-core
//!
//! Core types, settings, app registry, and error types for the django-rs framework.
//! This crate has zero framework dependencies and provides the foundation for all other crates.
//!
//! ## Modules
//!
//! - [`error`] - Error types and result aliases
//! - [`utils`] - Utility types (`MultiValueDict`, `LazyObject`, text helpers)
//! - [`settings`] - Framework settings and global configuration
//! - [`apps`] - Application registry and lifecycle management
//! - [`logging`] - Tracing-based logging integration

pub mod apps;
pub mod error;
pub mod logging;
pub mod settings;
pub mod utils;

// Re-export the most commonly used types at the crate root.
pub use error::{DjangoError, DjangoResult, ValidationError};
pub use settings::{Settings, SETTINGS};
