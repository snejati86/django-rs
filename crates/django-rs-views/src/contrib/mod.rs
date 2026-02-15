//! Django contrib-equivalent packages.
//!
//! This module provides implementations of Django's "contrib" frameworks:
//!
//! - [`sites`] - Sites framework for multi-site support
//! - [`redirects`] - URL redirect management with fallback middleware
//! - [`flatpages`] - Simple flat page serving with fallback middleware
//! - [`syndication`] - RSS/Atom feed generation

pub mod flatpages;
pub mod redirects;
pub mod sites;
pub mod syndication;
