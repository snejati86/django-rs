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
//! - [`settings_loader`] - Load settings from TOML, JSON, and environment variables
//! - [`apps`] - Application registry and lifecycle management
//! - [`logging`] - Tracing-based logging integration
//! - [`signing`] - Cryptographic signing (HMAC-SHA256, timestamps, serialization)
//! - [`checks`] - System check framework for configuration validation
//! - [`i18n`] - Internationalization and localization (translation catalogs, timezone)

// Allow large error type (DjangoError is shared across the project).
#![allow(clippy::result_large_err)]

pub mod apps;
pub mod checks;
pub mod error;
pub mod i18n;
pub mod logging;
pub mod settings;
pub mod settings_loader;
pub mod signing;
pub mod utils;

// Re-export the most commonly used types at the crate root.
pub use error::{DjangoError, DjangoResult, ValidationError};
pub use settings::{Settings, SETTINGS};
