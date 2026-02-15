//! URL routing and resolution.
//!
//! This module provides Django-style URL routing including:
//!
//! - [`pattern`]: URL pattern definitions via `path()` and `re_path()`
//! - [`converters`]: Path type converters (`int`, `str`, `slug`, `uuid`, `path`)
//! - [`resolver`]: Hierarchical URL resolution with namespace support
//! - [`reverse`]: Reverse URL generation from named patterns
//!
//! # Examples
//!
//! ```
//! use django_rs_http::urls::pattern::path;
//! use django_rs_http::urls::resolver::{root, include, URLEntry};
//! use django_rs_http::urls::reverse::reverse;
//! use django_rs_http::{HttpRequest, HttpResponse};
//! use std::collections::HashMap;
//! use std::sync::Arc;
//!
//! let handler = Arc::new(|_req: HttpRequest| -> django_rs_http::BoxFuture {
//!     Box::pin(async { HttpResponse::ok("ok") })
//! });
//!
//! let patterns = vec![
//!     URLEntry::Pattern(path("articles/<int:year>/", handler, Some("article-year")).unwrap()),
//! ];
//! let resolver = root(patterns).unwrap();
//!
//! // Forward resolution
//! let m = resolver.resolve("articles/2024/").unwrap();
//! assert_eq!(m.kwargs.get("year").unwrap(), "2024");
//!
//! // Reverse resolution
//! let mut kwargs = HashMap::new();
//! kwargs.insert("year", "2024");
//! let url = reverse("article-year", &[], &kwargs, &resolver).unwrap();
//! assert_eq!(url, "/articles/2024/");
//! ```

pub mod converters;
pub mod pattern;
pub mod resolver;
pub mod reverse;
