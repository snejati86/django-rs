//! # django-rs-admin
//!
//! Admin panel and contrib utilities for the django-rs framework.
//!
//! This crate provides:
//!
//! - **Admin site** ([`site::AdminSite`]) - Central registry for model administration
//! - **Model admin** ([`model_admin::ModelAdmin`]) - Configuration for how models appear
//!   in the admin panel, with a builder pattern API
//! - **REST API** ([`api`]) - JSON endpoints consumed by the React admin dashboard,
//!   including paginated list views, schema introspection, and CRUD operations
//! - **Actions** ([`actions`]) - Bulk operations on selected model objects
//! - **Filters** ([`filters`]) - List view filtering and searching
//! - **Contrib modules** ([`contrib`]) - Reusable utilities including content types,
//!   messages, humanize formatting, sitemaps, and static files management
//!
//! ## Architecture
//!
//! The admin panel uses a **React frontend** (built separately) with this crate
//! providing the backend REST/JSON API. All endpoints are fully async, leveraging
//! Rust's concurrency model for parallel database queries and bulk operations.
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_admin::site::AdminSite;
//! use django_rs_admin::model_admin::ModelAdmin;
//!
//! let mut site = AdminSite::new("admin");
//! site.register(
//!     "blog.article",
//!     ModelAdmin::new("blog", "article")
//!         .list_display(vec!["title", "author", "date"])
//!         .search_fields(vec!["title", "body"])
//!         .list_per_page(25),
//! );
//! let router = site.into_axum_router();
//! ```

pub mod actions;
pub mod api;
pub mod contrib;
pub mod filters;
pub mod model_admin;
pub mod site;
