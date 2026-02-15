//! Field definitions and types for the ORM.
//!
//! This module provides the [`FieldDef`] struct and [`FieldType`] enum that
//! describe model fields and their database column mappings. These mirror
//! Django's `django.db.models.fields` module.

pub mod types;

pub use types::{FieldDef, FieldType, OnDelete};
