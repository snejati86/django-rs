//! # django-rs-cli
//!
//! Management commands CLI and developer tooling for the django-rs framework.
//!
//! This crate provides:
//!
//! - **Management commands** - A framework for defining and registering CLI commands,
//!   plus built-in commands (`runserver`, `migrate`, `check`, etc.)
//! - **Caching** - Async cache backends (in-memory, database, filesystem, dummy)
//! - **Email** - Async email sending with multiple backends (SMTP, console, file, in-memory)
//! - **File storage** - Async file storage abstraction with filesystem backend
//! - **Serialization** - JSON serialization for data import/export
//!
//! ## Design Principles
//!
//! All I/O operations are async to avoid blocking the tokio runtime. All traits
//! require `Send + Sync` to support concurrent execution from multiple tasks.
//!
//! ## Quick Start
//!
//! ```rust
//! use django_rs_cli::command::CommandRegistry;
//! use django_rs_cli::commands::register_builtin_commands;
//!
//! let mut registry = CommandRegistry::new();
//! register_builtin_commands(&mut registry);
//!
//! let names = registry.list_commands();
//! assert!(names.contains(&"runserver"));
//! assert!(names.contains(&"migrate"));
//! assert!(names.contains(&"check"));
//! ```

// These clippy lints are intentionally allowed:
// - result_large_err: DjangoError is the framework-wide error type
// - doc_markdown: backtick requirements for documentation items are too strict
// - missing_const_for_fn: some functions may gain runtime logic later
// - module_name_repetitions: re-exports make module-prefixed names redundant
// - significant_drop_tightening: RwLock guards must be held for the operation duration
// - unused_async: command handlers maintain consistent async signatures
#![allow(clippy::result_large_err)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::unused_async)]

pub mod cache;
pub mod command;
pub mod commands;
pub mod email;
pub mod files;
pub mod serialization;

// Re-export primary types at the crate root for convenience.
pub use cache::{CacheBackend, CacheValue, DatabaseCache, DummyCache, FileCache, InMemoryCache};
pub use command::{CommandRegistry, ManagementCommand};
pub use email::{
    Attachment, ConsoleBackend, EmailBackend, EmailMessage, FileBackend, InMemoryBackend,
    SmtpBackend, get_connection, send_mail, send_mass_mail,
};
pub use files::{FileSystemStorage, Storage, UploadedFile};
pub use serialization::{JsonSerializer, PrettyJsonSerializer, Serializer};
