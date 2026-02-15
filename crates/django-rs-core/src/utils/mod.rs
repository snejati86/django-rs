//! Utility types and functions for the django-rs framework.
//!
//! This module provides:
//! - [`MultiValueDict`]: A dictionary that can hold multiple values per key.
//! - [`LazyObject`]: A lazily-initialized value wrapper.
//! - [`text`]: String utility functions (slugify, truncate, etc.).

mod lazy;
mod multi_value_dict;
pub mod text;

pub use lazy::LazyObject;
pub use multi_value_dict::MultiValueDict;
