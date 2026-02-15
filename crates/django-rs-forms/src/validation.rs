//! Validation pipeline for form processing.
//!
//! This module implements the Django-style validation pipeline:
//! 1. Field-level validation (type coercion + per-field validators)
//! 2. Form-level cross-field validation (async, can hit the database)
//!
//! Errors accumulate rather than short-circuiting, so all validation
//! issues are reported at once.
//!
//! This mirrors Django's `BaseForm._clean_fields()` and `BaseForm._clean_form()`.

use std::collections::HashMap;

use django_rs_db::value::Value;

use crate::fields::{clean_field_value, FormFieldDef};
use crate::form::Form;

/// Performs field-level validation for all fields.
///
/// For each field definition:
/// 1. Extracts the raw value from the data map
/// 2. Runs [`clean_field_value`] for type coercion and field-level validation
/// 3. Populates `cleaned_data` on success or `errors` on failure
///
/// Errors accumulate across all fields (no short-circuiting).
pub fn clean_fields(
    field_defs: &[FormFieldDef],
    raw_data: &HashMap<String, Option<String>>,
    cleaned_data: &mut HashMap<String, Value>,
    errors: &mut HashMap<String, Vec<String>>,
) {
    for field in field_defs {
        if field.disabled {
            // Disabled fields use their initial value and skip validation
            if let Some(initial) = &field.initial {
                cleaned_data.insert(field.name.clone(), initial.clone());
            }
            continue;
        }

        let raw = raw_data
            .get(&field.name)
            .and_then(|v| v.as_deref());

        match clean_field_value(field, raw) {
            Ok(value) => {
                cleaned_data.insert(field.name.clone(), value);
            }
            Err(field_errors) => {
                errors.insert(field.name.clone(), field_errors);
            }
        }
    }
}

/// Performs the full validation pipeline: field-level then form-level.
///
/// This is an async function because form-level cross-field validation
/// (via `form.clean()`) may require database access for uniqueness checks,
/// foreign key validation, or other I/O-bound operations.
///
/// # Returns
///
/// - `Ok(())` if all validation passes
/// - `Err(errors)` with a list of `(field_name, error_messages)` tuples
pub async fn full_clean(form: &mut dyn Form) -> Result<(), Vec<(String, Vec<String>)>> {
    // The form's is_valid() method handles the full pipeline internally.
    // This function provides an alternative entry point that returns
    // structured error data.
    if form.is_valid().await {
        Ok(())
    } else {
        let errors: Vec<(String, Vec<String>)> = form
            .errors()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::{FormFieldDef, FormFieldType};

    #[test]
    fn test_clean_fields_valid() {
        let fields = vec![
            FormFieldDef::new(
                "name",
                FormFieldType::Char {
                    min_length: None,
                    max_length: None,
                    strip: false,
                },
            ),
            FormFieldDef::new(
                "age",
                FormFieldType::Integer {
                    min_value: Some(0),
                    max_value: None,
                },
            ),
        ];
        let mut raw = HashMap::new();
        raw.insert("name".to_string(), Some("Alice".to_string()));
        raw.insert("age".to_string(), Some("30".to_string()));

        let mut cleaned = HashMap::new();
        let mut errors = HashMap::new();
        clean_fields(&fields, &raw, &mut cleaned, &mut errors);

        assert!(errors.is_empty());
        assert_eq!(cleaned.get("name"), Some(&Value::String("Alice".into())));
        assert_eq!(cleaned.get("age"), Some(&Value::Int(30)));
    }

    #[test]
    fn test_clean_fields_errors_accumulate() {
        let fields = vec![
            FormFieldDef::new(
                "name",
                FormFieldType::Char {
                    min_length: None,
                    max_length: None,
                    strip: false,
                },
            ),
            FormFieldDef::new(
                "email",
                FormFieldType::Email,
            ),
        ];
        let mut raw = HashMap::new();
        // Both fields missing (required)
        raw.insert("name".to_string(), None);
        raw.insert("email".to_string(), None);

        let mut cleaned = HashMap::new();
        let mut errors = HashMap::new();
        clean_fields(&fields, &raw, &mut cleaned, &mut errors);

        // Both fields should have errors
        assert!(errors.contains_key("name"));
        assert!(errors.contains_key("email"));
        assert!(cleaned.is_empty());
    }

    #[test]
    fn test_clean_fields_disabled_uses_initial() {
        let fields = vec![FormFieldDef::new(
            "status",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        )
        .disabled(true)
        .initial(Value::String("active".into()))];

        let raw = HashMap::new(); // No data submitted
        let mut cleaned = HashMap::new();
        let mut errors = HashMap::new();
        clean_fields(&fields, &raw, &mut cleaned, &mut errors);

        assert!(errors.is_empty());
        assert_eq!(
            cleaned.get("status"),
            Some(&Value::String("active".into()))
        );
    }

    #[test]
    fn test_clean_fields_partial_valid() {
        let fields = vec![
            FormFieldDef::new(
                "name",
                FormFieldType::Char {
                    min_length: None,
                    max_length: None,
                    strip: false,
                },
            ),
            FormFieldDef::new(
                "age",
                FormFieldType::Integer {
                    min_value: None,
                    max_value: None,
                },
            ),
        ];
        let mut raw = HashMap::new();
        raw.insert("name".to_string(), Some("Alice".to_string()));
        raw.insert("age".to_string(), Some("not-a-number".to_string()));

        let mut cleaned = HashMap::new();
        let mut errors = HashMap::new();
        clean_fields(&fields, &raw, &mut cleaned, &mut errors);

        // name is valid, age is not
        assert_eq!(cleaned.get("name"), Some(&Value::String("Alice".into())));
        assert!(errors.contains_key("age"));
        assert!(!errors.contains_key("name"));
    }

    #[tokio::test]
    async fn test_full_clean_valid() {
        use crate::form::BaseForm;
        use django_rs_http::QueryDict;

        let mut form = BaseForm::new(vec![FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        )]);
        let qd = QueryDict::parse("name=Alice");
        form.bind(&qd);

        let result = full_clean(&mut form).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_full_clean_invalid() {
        use crate::form::BaseForm;
        use django_rs_http::QueryDict;

        let mut form = BaseForm::new(vec![FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        )]);
        let qd = QueryDict::parse("");
        form.bind(&qd);

        let result = full_clean(&mut form).await;
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_clean_fields_missing_key_in_raw_data() {
        let fields = vec![FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        )];
        let raw = HashMap::new(); // Field not in raw data at all
        let mut cleaned = HashMap::new();
        let mut errors = HashMap::new();
        clean_fields(&fields, &raw, &mut cleaned, &mut errors);

        // Required field with no data should error
        assert!(errors.contains_key("name"));
    }

    #[test]
    fn test_clean_fields_optional_missing() {
        let fields = vec![FormFieldDef::new(
            "bio",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        )
        .required(false)];
        let raw = HashMap::new();
        let mut cleaned = HashMap::new();
        let mut errors = HashMap::new();
        clean_fields(&fields, &raw, &mut cleaned, &mut errors);

        assert!(errors.is_empty());
        assert!(cleaned.contains_key("bio"));
    }
}
