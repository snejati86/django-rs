//! Field validators for the ORM.
//!
//! Validators are used to enforce constraints on field values before they
//! are persisted to the database. This mirrors Django's validator system.

use crate::value::Value;
use django_rs_core::DjangoError;
use std::fmt;

/// A trait for validating field values.
///
/// Validators are attached to [`FieldDef`](crate::fields::FieldDef) instances and
/// called during model validation. Each validator checks a single constraint and
/// returns an error if the value does not satisfy it.
///
/// # Examples
///
/// ```
/// use django_rs_db::validators::{Validator, MaxLengthValidator};
/// use django_rs_db::value::Value;
///
/// let v = MaxLengthValidator::new(5);
/// assert!(v.validate(&Value::String("hi".into())).is_ok());
/// assert!(v.validate(&Value::String("toolong".into())).is_err());
/// ```
pub trait Validator: Send + Sync + fmt::Debug {
    /// Validates the given value, returning an error if invalid.
    fn validate(&self, value: &Value) -> Result<(), DjangoError>;

    /// Returns a human-readable name for this validator.
    fn name(&self) -> &str;
}

/// Validates that a string value does not exceed a maximum length.
#[derive(Debug, Clone)]
pub struct MaxLengthValidator {
    /// The maximum allowed length.
    pub max_length: usize,
}

impl MaxLengthValidator {
    /// Creates a new `MaxLengthValidator` with the given maximum length.
    pub const fn new(max_length: usize) -> Self {
        Self { max_length }
    }
}

impl Validator for MaxLengthValidator {
    fn validate(&self, value: &Value) -> Result<(), DjangoError> {
        if let Value::String(s) = value {
            if s.len() > self.max_length {
                return Err(DjangoError::ValidationError(
                    django_rs_core::ValidationError::new(
                        format!(
                            "Ensure this value has at most {} characters (it has {}).",
                            self.max_length,
                            s.len()
                        ),
                        "max_length",
                    ),
                ));
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "MaxLengthValidator"
    }
}

/// Validates that a string value meets a minimum length requirement.
#[derive(Debug, Clone)]
pub struct MinLengthValidator {
    /// The minimum required length.
    pub min_length: usize,
}

impl MinLengthValidator {
    /// Creates a new `MinLengthValidator` with the given minimum length.
    pub const fn new(min_length: usize) -> Self {
        Self { min_length }
    }
}

impl Validator for MinLengthValidator {
    fn validate(&self, value: &Value) -> Result<(), DjangoError> {
        if let Value::String(s) = value {
            if s.len() < self.min_length {
                return Err(DjangoError::ValidationError(
                    django_rs_core::ValidationError::new(
                        format!(
                            "Ensure this value has at least {} characters (it has {}).",
                            self.min_length,
                            s.len()
                        ),
                        "min_length",
                    ),
                ));
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "MinLengthValidator"
    }
}

/// Validates that a numeric value does not exceed a maximum.
#[derive(Debug, Clone)]
pub struct MaxValueValidator {
    /// The maximum allowed value.
    pub max_value: f64,
}

impl MaxValueValidator {
    /// Creates a new `MaxValueValidator` with the given maximum.
    pub fn new(max_value: f64) -> Self {
        Self { max_value }
    }
}

impl Validator for MaxValueValidator {
    fn validate(&self, value: &Value) -> Result<(), DjangoError> {
        let numeric = match value {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        };
        if let Some(n) = numeric {
            if n > self.max_value {
                return Err(DjangoError::ValidationError(
                    django_rs_core::ValidationError::new(
                        format!(
                            "Ensure this value is less than or equal to {}.",
                            self.max_value
                        ),
                        "max_value",
                    ),
                ));
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "MaxValueValidator"
    }
}

/// Validates that a numeric value meets a minimum requirement.
#[derive(Debug, Clone)]
pub struct MinValueValidator {
    /// The minimum required value.
    pub min_value: f64,
}

impl MinValueValidator {
    /// Creates a new `MinValueValidator` with the given minimum.
    pub fn new(min_value: f64) -> Self {
        Self { min_value }
    }
}

impl Validator for MinValueValidator {
    fn validate(&self, value: &Value) -> Result<(), DjangoError> {
        let numeric = match value {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        };
        if let Some(n) = numeric {
            if n < self.min_value {
                return Err(DjangoError::ValidationError(
                    django_rs_core::ValidationError::new(
                        format!(
                            "Ensure this value is greater than or equal to {}.",
                            self.min_value
                        ),
                        "min_value",
                    ),
                ));
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "MinValueValidator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_length_valid() {
        let v = MaxLengthValidator::new(5);
        assert!(v.validate(&Value::String("hello".into())).is_ok());
    }

    #[test]
    fn test_max_length_invalid() {
        let v = MaxLengthValidator::new(3);
        assert!(v.validate(&Value::String("toolong".into())).is_err());
    }

    #[test]
    fn test_max_length_non_string() {
        let v = MaxLengthValidator::new(3);
        assert!(v.validate(&Value::Int(12345)).is_ok());
    }

    #[test]
    fn test_min_length_valid() {
        let v = MinLengthValidator::new(3);
        assert!(v.validate(&Value::String("hello".into())).is_ok());
    }

    #[test]
    fn test_min_length_invalid() {
        let v = MinLengthValidator::new(5);
        assert!(v.validate(&Value::String("hi".into())).is_err());
    }

    #[test]
    fn test_max_value_valid() {
        let v = MaxValueValidator::new(100.0);
        assert!(v.validate(&Value::Int(50)).is_ok());
        assert!(v.validate(&Value::Float(99.9)).is_ok());
    }

    #[test]
    fn test_max_value_invalid() {
        let v = MaxValueValidator::new(100.0);
        assert!(v.validate(&Value::Int(101)).is_err());
        assert!(v.validate(&Value::Float(100.1)).is_err());
    }

    #[test]
    fn test_min_value_valid() {
        let v = MinValueValidator::new(0.0);
        assert!(v.validate(&Value::Int(1)).is_ok());
        assert!(v.validate(&Value::Float(0.0)).is_ok());
    }

    #[test]
    fn test_min_value_invalid() {
        let v = MinValueValidator::new(0.0);
        assert!(v.validate(&Value::Int(-1)).is_err());
        assert!(v.validate(&Value::Float(-0.1)).is_err());
    }

    #[test]
    fn test_validator_names() {
        assert_eq!(MaxLengthValidator::new(5).name(), "MaxLengthValidator");
        assert_eq!(MinLengthValidator::new(5).name(), "MinLengthValidator");
        assert_eq!(MaxValueValidator::new(5.0).name(), "MaxValueValidator");
        assert_eq!(MinValueValidator::new(5.0).name(), "MinValueValidator");
    }
}
