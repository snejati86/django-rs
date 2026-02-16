//! # django-rs-test
//!
//! Testing framework for the django-rs framework. Provides a test client for
//! simulating HTTP requests, assertion helpers for views and responses, and
//! utilities for structuring tests.
//!
//! ## Modules
//!
//! - [`client`] - HTTP test client wrapping Axum Router
//! - [`framework`] - Test case structure and assertion helpers
//! - [`test_database`] - In-memory SQLite database for ORM tests
//! - [`request_factory`] - Build `HttpRequest` objects without routing
//! - [`override_settings`] - Temporarily swap settings in tests
//! - [`mail_outbox`] - Capture emails sent during tests
//! - [`assert_queries`] - Assert the number of SQL queries executed
//! - [`live_server`] - Spawn a real HTTP server for integration tests
//!
//! ## Design Principles
//!
//! The test framework is built to support parallel test execution. The test client
//! is not shared across tests, and all operations are async.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use django_rs_test::client::TestClient;
//! use django_rs_test::framework::{assert_contains, assert_status};
//! use axum::Router;
//! use axum::routing::get;
//!
//! async fn example() {
//!     let app = Router::new().route("/", get(|| async { "Hello" }));
//!     let mut client = TestClient::new(app);
//!
//!     let response = client.get("/").await;
//!     assert_status(&response, 200);
//!     assert_contains(&response, "Hello");
//! }
//! ```

// These clippy lints are intentionally allowed:
// - result_large_err: DjangoError is the framework-wide error type
// - doc_markdown: backtick requirements for documentation items are too strict
// - missing_const_for_fn: some functions may gain runtime logic later
#![allow(clippy::result_large_err)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_const_for_fn)]

pub mod assert_queries;
pub mod client;
pub mod framework;
pub mod live_server;
pub mod mail_outbox;
pub mod override_settings;
pub mod request_factory;
pub mod test_database;

// Re-export primary types at the crate root for convenience.
pub use client::{TestClient, TestResponse};
pub use framework::{
    TestCase, assert_contains, assert_form_error, assert_has_header, assert_not_contains,
    assert_not_has_header, assert_redirects, assert_status, assert_template_used,
};

// Re-export new infrastructure types.
pub use assert_queries::{assert_max_queries, assert_num_queries};
pub use live_server::LiveServerTestCase;
pub use mail_outbox::{EmailMessage, MailOutbox};
pub use override_settings::{SettingsOverride, get_settings, override_settings};
pub use request_factory::RequestFactory;
pub use test_database::TestDatabase;
