//! Core error types for the django-rs framework.
//!
//! This module provides a comprehensive error enum [`DjangoError`] that covers
//! HTTP errors, ORM errors, validation errors, configuration errors, template errors,
//! and more. It mirrors the exception hierarchy found in Django.

use std::collections::HashMap;
use std::fmt;

use thiserror::Error;

/// Represents a validation error with optional field-level errors.
///
/// Validation errors can be either simple (a single message) or compound
/// (containing per-field error lists), mirroring Django's `ValidationError`.
///
/// # Examples
///
/// ```
/// use django_rs_core::error::ValidationError;
///
/// // Simple validation error
/// let err = ValidationError::new("This field is required.", "required");
///
/// // Field-level validation errors
/// let mut field_errors = std::collections::HashMap::new();
/// field_errors.insert(
///     "email".to_string(),
///     vec![ValidationError::new("Invalid email address.", "invalid")],
/// );
/// let err = ValidationError::with_field_errors(field_errors);
/// ```
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// The primary error message.
    pub message: String,
    /// A short code identifying the type of validation failure (e.g. "required", "invalid").
    pub code: String,
    /// Additional parameters providing context for the error message.
    pub params: HashMap<String, String>,
    /// Per-field validation errors, keyed by field name.
    pub field_errors: HashMap<String, Vec<Self>>,
}

impl ValidationError {
    /// Creates a new `ValidationError` with a message and code.
    pub fn new(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: code.into(),
            params: HashMap::new(),
            field_errors: HashMap::new(),
        }
    }

    /// Creates a `ValidationError` containing per-field errors.
    pub fn with_field_errors(field_errors: HashMap<String, Vec<Self>>) -> Self {
        Self {
            message: String::new(),
            code: String::new(),
            params: HashMap::new(),
            field_errors,
        }
    }

    /// Adds a parameter to this validation error.
    #[must_use]
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.message.is_empty() {
            write!(f, "{}", self.message)?;
        } else if !self.field_errors.is_empty() {
            let mut first = true;
            for (field, errors) in &self.field_errors {
                for error in errors {
                    if !first {
                        write!(f, "; ")?;
                    }
                    write!(f, "{field}: {error}")?;
                    first = false;
                }
            }
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

/// The primary error type for the django-rs framework.
///
/// This enum covers all categories of errors that can occur within the framework,
/// including HTTP errors, ORM/database errors, validation errors, configuration
/// errors, template errors, serialization errors, IO errors, and security errors.
///
/// Each variant maps to an appropriate HTTP status code via [`DjangoError::status_code`].
#[derive(Error, Debug)]
pub enum DjangoError {
    // ── HTTP errors ──────────────────────────────────────────────────

    /// HTTP 400 Bad Request.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// HTTP 401 Unauthorized.
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// HTTP 403 Forbidden / Permission Denied.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// HTTP 404 Not Found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// HTTP 405 Method Not Allowed.
    #[error("Method not allowed: {0}")]
    MethodNotAllowed(String),

    /// HTTP 409 Conflict.
    #[error("Conflict: {0}")]
    Conflict(String),

    /// HTTP 410 Gone.
    #[error("Gone")]
    Gone,

    /// HTTP 500 Internal Server Error.
    #[error("Internal server error: {0}")]
    InternalServerError(String),

    // ── ORM errors ───────────────────────────────────────────────────

    /// Raised when a query expected exactly one result but found none.
    #[error("Object does not exist: {0}")]
    DoesNotExist(String),

    /// Raised when a query expected exactly one result but found multiple.
    #[error("Multiple objects returned when one expected: {0}")]
    MultipleObjectsReturned(String),

    /// A generic database error.
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// A database integrity constraint was violated.
    #[error("Integrity error: {0}")]
    IntegrityError(String),

    /// An operational database error (connection failure, etc.).
    #[error("Operational error: {0}")]
    OperationalError(String),

    // ── Validation ───────────────────────────────────────────────────

    /// One or more fields failed validation.
    #[error("Validation error: {0}")]
    ValidationError(ValidationError),

    // ── Configuration ────────────────────────────────────────────────

    /// A configuration value is missing or invalid.
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    /// The framework is improperly configured.
    #[error("Improperly configured: {0}")]
    ImproperlyConfigured(String),

    // ── Templates ────────────────────────────────────────────────────

    /// A template contains invalid syntax.
    #[error("Template syntax error: {0}")]
    TemplateSyntaxError(String),

    /// The requested template file was not found.
    #[error("Template does not exist: {0}")]
    TemplateDoesNotExist(String),

    // ── Serialization ────────────────────────────────────────────────

    /// An error occurred during serialization or deserialization.
    #[error("Serialization error: {0}")]
    SerializationError(String),

    // ── IO ───────────────────────────────────────────────────────────

    /// An I/O error occurred.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    // ── Security ─────────────────────────────────────────────────────

    /// A potentially malicious operation was detected.
    #[error("Suspicious operation: {0}")]
    SuspiciousOperation(String),
}

impl DjangoError {
    /// Returns the HTTP status code associated with this error.
    ///
    /// This mapping follows Django's conventions where applicable:
    ///
    /// - `BadRequest`, `ValidationError` -> 400
    /// - `Unauthorized` -> 401
    /// - `PermissionDenied`, `SuspiciousOperation` -> 403
    /// - `NotFound`, `DoesNotExist` -> 404
    /// - `MethodNotAllowed` -> 405
    /// - `Conflict` -> 409
    /// - `Gone` -> 410
    /// - Everything else -> 500
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::BadRequest(_) | Self::ValidationError(_) => 400,
            Self::Unauthorized(_) => 401,
            Self::PermissionDenied(_) | Self::SuspiciousOperation(_) => 403,
            Self::NotFound(_) | Self::DoesNotExist(_) => 404,
            Self::MethodNotAllowed(_) => 405,
            Self::Conflict(_) => 409,
            Self::Gone => 410,
            Self::InternalServerError(_)
            | Self::MultipleObjectsReturned(_)
            | Self::DatabaseError(_)
            | Self::IntegrityError(_)
            | Self::OperationalError(_)
            | Self::ConfigurationError(_)
            | Self::ImproperlyConfigured(_)
            | Self::TemplateSyntaxError(_)
            | Self::TemplateDoesNotExist(_)
            | Self::SerializationError(_)
            | Self::IoError(_) => 500,
        }
    }
}

/// A convenience type alias for `Result<T, DjangoError>`.
pub type DjangoResult<T> = Result<T, DjangoError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_display_simple() {
        let err = ValidationError::new("This field is required.", "required");
        assert_eq!(err.to_string(), "This field is required.");
    }

    #[test]
    fn test_validation_error_display_field_errors() {
        let mut field_errors = HashMap::new();
        field_errors.insert(
            "email".to_string(),
            vec![ValidationError::new("Invalid email.", "invalid")],
        );
        let err = ValidationError::with_field_errors(field_errors);
        assert!(err.to_string().contains("email: Invalid email."));
    }

    #[test]
    fn test_validation_error_with_param() {
        let err = ValidationError::new("Too short.", "min_length")
            .with_param("min", "8");
        assert_eq!(err.params.get("min").unwrap(), "8");
    }

    #[test]
    fn test_django_error_status_codes() {
        assert_eq!(DjangoError::BadRequest("x".into()).status_code(), 400);
        assert_eq!(DjangoError::Unauthorized("x".into()).status_code(), 401);
        assert_eq!(DjangoError::PermissionDenied("x".into()).status_code(), 403);
        assert_eq!(DjangoError::NotFound("x".into()).status_code(), 404);
        assert_eq!(DjangoError::MethodNotAllowed("x".into()).status_code(), 405);
        assert_eq!(DjangoError::Conflict("x".into()).status_code(), 409);
        assert_eq!(DjangoError::Gone.status_code(), 410);
        assert_eq!(DjangoError::InternalServerError("x".into()).status_code(), 500);
        assert_eq!(DjangoError::DoesNotExist("x".into()).status_code(), 404);
        assert_eq!(DjangoError::DatabaseError("x".into()).status_code(), 500);
        assert_eq!(DjangoError::IntegrityError("x".into()).status_code(), 500);
        assert_eq!(
            DjangoError::ValidationError(ValidationError::new("x", "y")).status_code(),
            400
        );
        assert_eq!(DjangoError::SuspiciousOperation("x".into()).status_code(), 403);
        assert_eq!(DjangoError::TemplateSyntaxError("x".into()).status_code(), 500);
        assert_eq!(DjangoError::TemplateDoesNotExist("x".into()).status_code(), 500);
    }

    #[test]
    fn test_django_error_display() {
        let err = DjangoError::NotFound("page".into());
        assert_eq!(err.to_string(), "Not found: page");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let django_err: DjangoError = io_err.into();
        assert_eq!(django_err.status_code(), 500);
        assert!(django_err.to_string().contains("file missing"));
    }
}
