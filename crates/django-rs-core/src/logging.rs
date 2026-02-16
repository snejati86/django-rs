//! Logging integration for the django-rs framework.
//!
//! Provides helpers for configuring [`tracing`]-based logging from
//! [`Settings`](crate::settings::Settings) and for creating per-request spans.

use crate::settings::Settings;

/// Sets up the global tracing subscriber based on the given settings.
///
/// The log level is read from `settings.log_level` (e.g. "debug", "info", "warn",
/// "error"). In debug mode a pretty, human-readable format is used; in production
/// a structured JSON format is used.
///
/// # Panics
///
/// Panics if the subscriber cannot be set (e.g. if one was already installed).
pub fn setup_logging(settings: &Settings) {
    use tracing_subscriber::fmt;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_new(&settings.log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    if settings.debug {
        fmt::Subscriber::builder()
            .with_env_filter(filter)
            .with_target(true)
            .with_thread_ids(false)
            .with_file(true)
            .with_line_number(true)
            .pretty()
            .try_init()
            .ok();
    } else {
        fmt::Subscriber::builder()
            .with_env_filter(filter)
            .with_target(true)
            .json()
            .try_init()
            .ok();
    }
}

/// Creates a tracing span for an HTTP request.
///
/// Attach this span to the request processing pipeline so that all log
/// entries emitted during request handling include the request ID.
///
/// # Examples
///
/// ```
/// use django_rs_core::logging::request_span;
///
/// let span = request_span("abc-123");
/// let _guard = span.enter();
/// tracing::info!("handling request");
/// ```
pub fn request_span(request_id: &str) -> tracing::Span {
    tracing::info_span!("request", id = request_id)
}
