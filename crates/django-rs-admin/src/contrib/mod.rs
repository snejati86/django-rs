//! Contrib modules providing common web application utilities.
//!
//! This module contains reusable components that mirror Django's `contrib` packages:
//!
//! - [`contenttypes`] - Content type registry for generic model references
//! - [`messages`] - One-time notification message framework
//! - [`humanize`] - Human-friendly formatting for numbers, dates, and sizes
//! - [`sitemaps`] - XML sitemap generation
//! - [`staticfiles`] - Static file finder and collector

pub mod contenttypes;
pub mod humanize;
pub mod messages;
pub mod sitemaps;
pub mod staticfiles;
