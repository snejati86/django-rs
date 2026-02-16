//! Form field definitions and type-level validation.
//!
//! Each [`FormFieldDef`] describes a single form field, including its type,
//! validators, widget, and metadata. The [`FormFieldType`] enum defines
//! the type-specific parsing and coercion logic through the [`clean_field_value`]
//! function.
//!
//! This mirrors Django's `django.forms.fields` module.

use std::collections::HashMap;

use django_rs_core::DjangoError;
use django_rs_db::validators::Validator;
use django_rs_db::value::Value;

use crate::widgets::WidgetType;

/// Defines the type of a form field, including type-specific parameters.
///
/// Each variant carries the parameters needed for parsing and validating
/// raw string input from form submissions. The [`clean_field_value`] function
/// dispatches on this enum to perform type coercion and built-in validation.
#[derive(Debug, Clone)]
pub enum FormFieldType {
    /// A character (string) field.
    Char {
        /// Minimum length (characters).
        min_length: Option<usize>,
        /// Maximum length (characters).
        max_length: Option<usize>,
        /// Whether to strip leading/trailing whitespace.
        strip: bool,
    },
    /// An integer field.
    Integer {
        /// Minimum allowed value.
        min_value: Option<i64>,
        /// Maximum allowed value.
        max_value: Option<i64>,
    },
    /// A floating-point field.
    Float {
        /// Minimum allowed value.
        min_value: Option<f64>,
        /// Maximum allowed value.
        max_value: Option<f64>,
    },
    /// A fixed-precision decimal field.
    Decimal {
        /// Maximum total number of digits.
        max_digits: u32,
        /// Number of digits after the decimal point.
        decimal_places: u32,
    },
    /// A boolean field (true/false).
    Boolean,
    /// A nullable boolean field (true/false/null).
    NullBoolean,
    /// A date field (YYYY-MM-DD).
    Date,
    /// A date-time field (YYYY-MM-DDTHH:MM:SS).
    DateTime,
    /// A time field (HH:MM:SS).
    Time,
    /// A duration field (e.g. "1 day, 2:03:04").
    Duration,
    /// An email address field.
    Email,
    /// A URL field.
    Url,
    /// A UUID field.
    Uuid,
    /// A slug field (letters, numbers, hyphens, underscores).
    Slug,
    /// An IP address field.
    IpAddress,
    /// A single-choice field.
    Choice {
        /// Available choices as `(value, display_label)` pairs.
        choices: Vec<(String, String)>,
    },
    /// A multiple-choice field.
    MultipleChoice {
        /// Available choices as `(value, display_label)` pairs.
        choices: Vec<(String, String)>,
    },
    /// A file upload field.
    File {
        /// Maximum file size in bytes.
        max_size: Option<usize>,
        /// Allowed file extensions (e.g. `["jpg", "png"]`).
        allowed_extensions: Vec<String>,
    },
    /// An image upload field.
    Image,
    /// A choice field with a coercion function.
    TypedChoice {
        /// Available choices as `(value, display_label)` pairs.
        choices: Vec<(String, String)>,
        /// A function to coerce the raw string value into a `Value`.
        coerce: fn(&str) -> Result<Value, DjangoError>,
    },
    /// A JSON field.
    Json,
    /// A field validated against a regular expression.
    Regex {
        /// The regex pattern string.
        regex: String,
    },
}

/// Complete definition of a form field.
///
/// A `FormFieldDef` captures everything needed to render, parse, and validate
/// a single form field. It is the form-layer analog of
/// [`FieldDef`](django_rs_db::fields::FieldDef) from the ORM.
#[derive(Debug)]
pub struct FormFieldDef {
    /// The field name (HTML name attribute).
    pub name: String,
    /// The field type, controlling parsing and coercion.
    pub field_type: FormFieldType,
    /// Whether this field is required.
    pub required: bool,
    /// Default/initial value.
    pub initial: Option<Value>,
    /// Help text displayed alongside the field.
    pub help_text: String,
    /// Human-readable label.
    pub label: String,
    /// The widget type used for rendering.
    pub widget: WidgetType,
    /// Additional validators applied after type coercion.
    pub validators: Vec<Box<dyn Validator>>,
    /// Custom error messages keyed by error code.
    pub error_messages: HashMap<String, String>,
    /// Whether the field is disabled (rendered but not editable).
    pub disabled: bool,
}

impl FormFieldDef {
    /// Creates a new `FormFieldDef` with sensible defaults.
    ///
    /// The field is required by default, uses the default widget for its type,
    /// and has no validators beyond the type-level validation.
    pub fn new(name: impl Into<String>, field_type: FormFieldType) -> Self {
        let name = name.into();
        let widget = default_widget_for_field_type(&field_type);
        let label = name.replace('_', " ");
        Self {
            name,
            field_type,
            required: true,
            initial: None,
            help_text: String::new(),
            label,
            widget,
            validators: Vec::new(),
            error_messages: HashMap::new(),
            disabled: false,
        }
    }

    /// Sets whether this field is required.
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// Sets the initial value.
    pub fn initial(mut self, value: Value) -> Self {
        self.initial = Some(value);
        self
    }

    /// Sets the help text.
    pub fn help_text(mut self, text: impl Into<String>) -> Self {
        self.help_text = text.into();
        self
    }

    /// Sets the label.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Sets the widget type.
    pub fn widget(mut self, widget: WidgetType) -> Self {
        self.widget = widget;
        self
    }

    /// Adds a validator.
    pub fn validator(mut self, validator: Box<dyn Validator>) -> Self {
        self.validators.push(validator);
        self
    }

    /// Sets a custom error message for a given code.
    pub fn error_message(mut self, code: impl Into<String>, msg: impl Into<String>) -> Self {
        self.error_messages.insert(code.into(), msg.into());
        self
    }

    /// Sets whether this field is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// Returns the default widget type for a given form field type.
pub fn default_widget_for_field_type(field_type: &FormFieldType) -> WidgetType {
    match field_type {
        FormFieldType::Char { .. } => WidgetType::TextInput,
        FormFieldType::Integer { .. } => WidgetType::NumberInput,
        FormFieldType::Float { .. } => WidgetType::NumberInput,
        FormFieldType::Decimal { .. } => WidgetType::NumberInput,
        FormFieldType::Boolean => WidgetType::CheckboxInput,
        FormFieldType::NullBoolean => WidgetType::Select,
        FormFieldType::Date => WidgetType::DateInput,
        FormFieldType::DateTime => WidgetType::DateTimeInput,
        FormFieldType::Time => WidgetType::TimeInput,
        FormFieldType::Duration => WidgetType::TextInput,
        FormFieldType::Email => WidgetType::EmailInput,
        FormFieldType::Url => WidgetType::UrlInput,
        FormFieldType::Uuid => WidgetType::TextInput,
        FormFieldType::Slug => WidgetType::TextInput,
        FormFieldType::IpAddress => WidgetType::TextInput,
        FormFieldType::Choice { .. } => WidgetType::Select,
        FormFieldType::MultipleChoice { .. } => WidgetType::SelectMultiple,
        FormFieldType::File { .. } => WidgetType::FileInput,
        FormFieldType::Image => WidgetType::FileInput,
        FormFieldType::TypedChoice { .. } => WidgetType::Select,
        FormFieldType::Json => WidgetType::Textarea,
        FormFieldType::Regex { .. } => WidgetType::TextInput,
    }
}

/// Cleans (validates and coerces) a raw form input string into a typed `Value`.
///
/// This performs type-level validation:
/// 1. Required check (if `required` and value is empty/None)
/// 2. Type coercion (string -> i64, date, etc.)
/// 3. Type-specific constraint validation (min/max, regex, choices)
/// 4. Custom validators
///
/// Returns the cleaned `Value` or a list of error messages.
pub fn clean_field_value(field: &FormFieldDef, raw: Option<&str>) -> Result<Value, Vec<String>> {
    let raw_str = raw.unwrap_or("");
    let is_empty = raw_str.is_empty() || raw.is_none();

    // Required check
    if field.required && is_empty {
        let msg = field
            .error_messages
            .get("required")
            .cloned()
            .unwrap_or_else(|| "This field is required.".to_string());
        return Err(vec![msg]);
    }

    // If not required and empty, return Null
    if is_empty {
        return Ok(field.initial.clone().unwrap_or(Value::Null));
    }

    let mut errors = Vec::new();

    // Type coercion and built-in validation
    let value = match &field.field_type {
        FormFieldType::Char {
            min_length,
            max_length,
            strip,
        } => {
            let s = if *strip { raw_str.trim() } else { raw_str };
            if let Some(min) = min_length {
                if s.len() < *min {
                    errors.push(format!(
                        "Ensure this value has at least {min} characters (it has {}).",
                        s.len()
                    ));
                }
            }
            if let Some(max) = max_length {
                if s.len() > *max {
                    errors.push(format!(
                        "Ensure this value has at most {max} characters (it has {}).",
                        s.len()
                    ));
                }
            }
            Value::String(s.to_string())
        }

        FormFieldType::Integer {
            min_value,
            max_value,
        } => match raw_str.parse::<i64>() {
            Ok(n) => {
                if let Some(min) = min_value {
                    if n < *min {
                        errors.push(format!(
                            "Ensure this value is greater than or equal to {min}."
                        ));
                    }
                }
                if let Some(max) = max_value {
                    if n > *max {
                        errors.push(format!("Ensure this value is less than or equal to {max}."));
                    }
                }
                Value::Int(n)
            }
            Err(_) => {
                errors.push("Enter a whole number.".to_string());
                Value::Null
            }
        },

        FormFieldType::Float {
            min_value,
            max_value,
        } => match raw_str.parse::<f64>() {
            Ok(n) => {
                if let Some(min) = min_value {
                    if n < *min {
                        errors.push(format!(
                            "Ensure this value is greater than or equal to {min}."
                        ));
                    }
                }
                if let Some(max) = max_value {
                    if n > *max {
                        errors.push(format!("Ensure this value is less than or equal to {max}."));
                    }
                }
                Value::Float(n)
            }
            Err(_) => {
                errors.push("Enter a number.".to_string());
                Value::Null
            }
        },

        FormFieldType::Decimal {
            max_digits,
            decimal_places,
        } => {
            match raw_str.parse::<f64>() {
                Ok(n) => {
                    // Validate digit counts
                    let parts: Vec<&str> = raw_str.trim_start_matches('-').split('.').collect();
                    let integer_digits = parts[0].len();
                    let actual_decimal_places = parts.get(1).map_or(0, |p| p.len());
                    let total_digits = integer_digits + actual_decimal_places;

                    if total_digits > *max_digits as usize {
                        errors.push(format!(
                            "Ensure that there are no more than {max_digits} digits in total."
                        ));
                    }
                    if actual_decimal_places > *decimal_places as usize {
                        errors.push(format!(
                            "Ensure that there are no more than {decimal_places} decimal places."
                        ));
                    }
                    Value::Float(n)
                }
                Err(_) => {
                    errors.push("Enter a number.".to_string());
                    Value::Null
                }
            }
        }

        FormFieldType::Boolean => {
            let val = matches!(raw_str.to_lowercase().as_str(), "true" | "1" | "yes" | "on");
            Value::Bool(val)
        }

        FormFieldType::NullBoolean => {
            let lower = raw_str.to_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Value::Bool(true),
                "false" | "0" | "no" | "off" => Value::Bool(false),
                "" | "null" | "none" | "unknown" => Value::Null,
                _ => {
                    errors.push("Select a valid choice.".to_string());
                    Value::Null
                }
            }
        }

        FormFieldType::Date => match chrono::NaiveDate::parse_from_str(raw_str, "%Y-%m-%d") {
            Ok(d) => Value::Date(d),
            Err(_) => {
                errors.push("Enter a valid date (YYYY-MM-DD).".to_string());
                Value::Null
            }
        },

        FormFieldType::DateTime => {
            // Try multiple formats
            let result = chrono::NaiveDateTime::parse_from_str(raw_str, "%Y-%m-%dT%H:%M:%S")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(raw_str, "%Y-%m-%dT%H:%M"))
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(raw_str, "%Y-%m-%d %H:%M:%S"))
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(raw_str, "%Y-%m-%d %H:%M"));
            match result {
                Ok(dt) => Value::DateTime(dt),
                Err(_) => {
                    errors.push("Enter a valid date/time.".to_string());
                    Value::Null
                }
            }
        }

        FormFieldType::Time => {
            let result = chrono::NaiveTime::parse_from_str(raw_str, "%H:%M:%S")
                .or_else(|_| chrono::NaiveTime::parse_from_str(raw_str, "%H:%M"));
            match result {
                Ok(t) => Value::Time(t),
                Err(_) => {
                    errors.push("Enter a valid time (HH:MM or HH:MM:SS).".to_string());
                    Value::Null
                }
            }
        }

        FormFieldType::Duration => {
            // Parse simple duration formats like "1:23:45" or seconds
            if let Some(dur) = parse_duration(raw_str) {
                Value::Duration(dur)
            } else {
                errors.push("Enter a valid duration.".to_string());
                Value::Null
            }
        }

        FormFieldType::Email => {
            // Basic email validation
            let email_re = regex::Regex::new(r"^[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}$")
                .expect("valid regex");
            if email_re.is_match(raw_str) {
                Value::String(raw_str.to_string())
            } else {
                errors.push("Enter a valid email address.".to_string());
                Value::String(raw_str.to_string())
            }
        }

        FormFieldType::Url => {
            // Basic URL validation
            let url_re = regex::Regex::new(r"^https?://[^\s/$.?#].[^\s]*$").expect("valid regex");
            if url_re.is_match(raw_str) {
                Value::String(raw_str.to_string())
            } else {
                errors.push("Enter a valid URL.".to_string());
                Value::String(raw_str.to_string())
            }
        }

        FormFieldType::Uuid => match uuid::Uuid::parse_str(raw_str) {
            Ok(u) => Value::Uuid(u),
            Err(_) => {
                errors.push("Enter a valid UUID.".to_string());
                Value::Null
            }
        },

        FormFieldType::Slug => {
            let slug_re = regex::Regex::new(r"^[-a-zA-Z0-9_]+$").expect("valid regex");
            if slug_re.is_match(raw_str) {
                Value::String(raw_str.to_string())
            } else {
                errors.push(
                    "Enter a valid \"slug\" consisting of letters, numbers, underscores or hyphens."
                        .to_string(),
                );
                Value::String(raw_str.to_string())
            }
        }

        FormFieldType::IpAddress => {
            // Accept IPv4 and IPv6
            if raw_str.parse::<std::net::IpAddr>().is_ok() {
                Value::String(raw_str.to_string())
            } else {
                errors.push("Enter a valid IP address.".to_string());
                Value::String(raw_str.to_string())
            }
        }

        FormFieldType::Choice { choices } => {
            let valid = choices.iter().any(|(v, _)| v == raw_str);
            if valid {
                Value::String(raw_str.to_string())
            } else {
                errors.push(format!(
                    "Select a valid choice. {raw_str} is not one of the available choices."
                ));
                Value::String(raw_str.to_string())
            }
        }

        FormFieldType::MultipleChoice { choices } => {
            let selected: Vec<&str> = raw_str.split(',').collect();
            let mut valid_values = Vec::new();
            for s in &selected {
                let s = s.trim();
                if choices.iter().any(|(v, _)| v == s) {
                    valid_values.push(Value::String(s.to_string()));
                } else {
                    errors.push(format!(
                        "Select a valid choice. {s} is not one of the available choices."
                    ));
                }
            }
            Value::List(valid_values)
        }

        FormFieldType::File {
            max_size,
            allowed_extensions,
        } => {
            // Basic file name validation
            if let Some(max) = max_size {
                // In practice, file size comes from the multipart data.
                // Here we check the string length as a placeholder.
                if raw_str.len() > *max {
                    errors.push(format!("File size exceeds maximum of {max} bytes."));
                }
            }
            if !allowed_extensions.is_empty() {
                let ext = raw_str
                    .rsplit('.')
                    .next()
                    .map(str::to_lowercase)
                    .unwrap_or_default();
                if !allowed_extensions.iter().any(|e| e.to_lowercase() == ext) {
                    errors.push(format!(
                        "File extension not allowed. Allowed extensions: {}.",
                        allowed_extensions.join(", ")
                    ));
                }
            }
            Value::String(raw_str.to_string())
        }

        FormFieldType::Image => {
            // Basic image validation (check extension)
            let ext = raw_str
                .rsplit('.')
                .next()
                .map(str::to_lowercase)
                .unwrap_or_default();
            let image_exts = ["jpg", "jpeg", "png", "gif", "bmp", "webp", "svg"];
            if !image_exts.contains(&ext.as_str()) {
                errors.push(
                    "Upload a valid image. The file must have an image extension.".to_string(),
                );
            }
            Value::String(raw_str.to_string())
        }

        FormFieldType::TypedChoice { choices, coerce } => {
            let valid = choices.iter().any(|(v, _)| v == raw_str);
            if !valid {
                errors.push(format!(
                    "Select a valid choice. {raw_str} is not one of the available choices."
                ));
                Value::Null
            } else {
                match coerce(raw_str) {
                    Ok(v) => v,
                    Err(_) => {
                        errors.push("Invalid value.".to_string());
                        Value::Null
                    }
                }
            }
        }

        FormFieldType::Json => match serde_json::from_str::<serde_json::Value>(raw_str) {
            Ok(j) => Value::Json(j),
            Err(_) => {
                errors.push("Enter valid JSON.".to_string());
                Value::Null
            }
        },

        FormFieldType::Regex { regex } => {
            let re = regex::Regex::new(regex).map_err(|e| vec![format!("Invalid regex: {e}")])?;
            if re.is_match(raw_str) {
                Value::String(raw_str.to_string())
            } else {
                errors.push("Enter a valid value.".to_string());
                Value::String(raw_str.to_string())
            }
        }
    };

    // Run custom validators on the cleaned value (only if no type errors so far)
    if errors.is_empty() {
        for validator in &field.validators {
            if let Err(e) = validator.validate(&value) {
                errors.push(e.to_string());
            }
        }
    }

    if errors.is_empty() {
        Ok(value)
    } else {
        Err(errors)
    }
}

/// Parses a simple duration string into a `chrono::Duration`.
///
/// Supports formats: `HH:MM:SS`, `MM:SS`, or a plain number of seconds.
fn parse_duration(s: &str) -> Option<chrono::Duration> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        1 => {
            // Plain seconds
            let secs: i64 = parts[0].parse().ok()?;
            Some(chrono::Duration::seconds(secs))
        }
        2 => {
            // MM:SS
            let mins: i64 = parts[0].parse().ok()?;
            let secs: i64 = parts[1].parse().ok()?;
            Some(chrono::Duration::seconds(mins * 60 + secs))
        }
        3 => {
            // HH:MM:SS
            let hours: i64 = parts[0].parse().ok()?;
            let mins: i64 = parts[1].parse().ok()?;
            let secs: i64 = parts[2].parse().ok()?;
            Some(chrono::Duration::seconds(hours * 3600 + mins * 60 + secs))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_char_field_clean() {
        let field = FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: Some(2),
                max_length: Some(50),
                strip: true,
            },
        );
        let result = clean_field_value(&field, Some("  Alice  "));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::String("Alice".to_string()));
    }

    #[test]
    fn test_char_field_too_short() {
        let field = FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: Some(5),
                max_length: None,
                strip: false,
            },
        );
        let result = clean_field_value(&field, Some("Hi"));
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("at least 5"));
    }

    #[test]
    fn test_char_field_too_long() {
        let field = FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: Some(3),
                strip: false,
            },
        );
        let result = clean_field_value(&field, Some("Hello"));
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("at most 3"));
    }

    #[test]
    fn test_integer_field_clean() {
        let field = FormFieldDef::new(
            "age",
            FormFieldType::Integer {
                min_value: Some(0),
                max_value: Some(150),
            },
        );
        let result = clean_field_value(&field, Some("25"));
        assert_eq!(result.unwrap(), Value::Int(25));
    }

    #[test]
    fn test_integer_field_invalid() {
        let field = FormFieldDef::new(
            "age",
            FormFieldType::Integer {
                min_value: None,
                max_value: None,
            },
        );
        let result = clean_field_value(&field, Some("abc"));
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("whole number"));
    }

    #[test]
    fn test_integer_field_min() {
        let field = FormFieldDef::new(
            "age",
            FormFieldType::Integer {
                min_value: Some(18),
                max_value: None,
            },
        );
        let result = clean_field_value(&field, Some("10"));
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("greater than or equal to 18"));
    }

    #[test]
    fn test_integer_field_max() {
        let field = FormFieldDef::new(
            "age",
            FormFieldType::Integer {
                min_value: None,
                max_value: Some(100),
            },
        );
        let result = clean_field_value(&field, Some("150"));
        assert!(result.is_err());
    }

    #[test]
    fn test_float_field_clean() {
        let field = FormFieldDef::new(
            "price",
            FormFieldType::Float {
                min_value: None,
                max_value: None,
            },
        );
        let result = clean_field_value(&field, Some("19.99"));
        assert_eq!(result.unwrap(), Value::Float(19.99));
    }

    #[test]
    fn test_float_field_invalid() {
        let field = FormFieldDef::new(
            "price",
            FormFieldType::Float {
                min_value: None,
                max_value: None,
            },
        );
        let result = clean_field_value(&field, Some("not-a-number"));
        assert!(result.is_err());
    }

    #[test]
    fn test_decimal_field_clean() {
        let field = FormFieldDef::new(
            "amount",
            FormFieldType::Decimal {
                max_digits: 5,
                decimal_places: 2,
            },
        );
        let result = clean_field_value(&field, Some("123.45"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_decimal_field_too_many_digits() {
        let field = FormFieldDef::new(
            "amount",
            FormFieldType::Decimal {
                max_digits: 4,
                decimal_places: 2,
            },
        );
        let result = clean_field_value(&field, Some("123.45"));
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("no more than 4 digits"));
    }

    #[test]
    fn test_decimal_field_too_many_decimal_places() {
        let field = FormFieldDef::new(
            "amount",
            FormFieldType::Decimal {
                max_digits: 10,
                decimal_places: 1,
            },
        );
        let result = clean_field_value(&field, Some("1.234"));
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("no more than 1 decimal places"));
    }

    #[test]
    fn test_boolean_field_clean() {
        let field = FormFieldDef::new("agree", FormFieldType::Boolean);
        assert_eq!(
            clean_field_value(&field, Some("true")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            clean_field_value(&field, Some("on")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            clean_field_value(&field, Some("1")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            clean_field_value(&field, Some("yes")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            clean_field_value(&field, Some("false")).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_null_boolean_field_clean() {
        let field = FormFieldDef::new("maybe", FormFieldType::NullBoolean).required(false);
        assert_eq!(
            clean_field_value(&field, Some("true")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            clean_field_value(&field, Some("false")).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            clean_field_value(&field, Some("unknown")).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn test_date_field_clean() {
        let field = FormFieldDef::new("birthday", FormFieldType::Date);
        let result = clean_field_value(&field, Some("2024-01-15"));
        assert!(result.is_ok());
        if let Value::Date(d) = result.unwrap() {
            assert_eq!(d.to_string(), "2024-01-15");
        } else {
            panic!("Expected Date value");
        }
    }

    #[test]
    fn test_date_field_invalid() {
        let field = FormFieldDef::new("birthday", FormFieldType::Date);
        let result = clean_field_value(&field, Some("not-a-date"));
        assert!(result.is_err());
    }

    #[test]
    fn test_datetime_field_clean() {
        let field = FormFieldDef::new("event", FormFieldType::DateTime);
        let result = clean_field_value(&field, Some("2024-01-15T10:30:00"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_datetime_field_short_format() {
        let field = FormFieldDef::new("event", FormFieldType::DateTime);
        let result = clean_field_value(&field, Some("2024-01-15T10:30"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_time_field_clean() {
        let field = FormFieldDef::new("start", FormFieldType::Time);
        let result = clean_field_value(&field, Some("14:30:00"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_time_field_short_format() {
        let field = FormFieldDef::new("start", FormFieldType::Time);
        let result = clean_field_value(&field, Some("14:30"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_duration_field_clean() {
        let field = FormFieldDef::new("length", FormFieldType::Duration);
        let result = clean_field_value(&field, Some("1:30:00"));
        assert!(result.is_ok());
        if let Value::Duration(d) = result.unwrap() {
            assert_eq!(d.num_seconds(), 5400);
        } else {
            panic!("Expected Duration value");
        }
    }

    #[test]
    fn test_email_field_valid() {
        let field = FormFieldDef::new("email", FormFieldType::Email);
        let result = clean_field_value(&field, Some("user@example.com"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_email_field_invalid() {
        let field = FormFieldDef::new("email", FormFieldType::Email);
        let result = clean_field_value(&field, Some("not-an-email"));
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("valid email"));
    }

    #[test]
    fn test_url_field_valid() {
        let field = FormFieldDef::new("website", FormFieldType::Url);
        let result = clean_field_value(&field, Some("https://example.com"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_url_field_invalid() {
        let field = FormFieldDef::new("website", FormFieldType::Url);
        let result = clean_field_value(&field, Some("not-a-url"));
        assert!(result.is_err());
    }

    #[test]
    fn test_uuid_field_valid() {
        let field = FormFieldDef::new("id", FormFieldType::Uuid);
        let result = clean_field_value(&field, Some("550e8400-e29b-41d4-a716-446655440000"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_uuid_field_invalid() {
        let field = FormFieldDef::new("id", FormFieldType::Uuid);
        let result = clean_field_value(&field, Some("not-a-uuid"));
        assert!(result.is_err());
    }

    #[test]
    fn test_slug_field_valid() {
        let field = FormFieldDef::new("slug", FormFieldType::Slug);
        let result = clean_field_value(&field, Some("my-cool-post"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_slug_field_invalid() {
        let field = FormFieldDef::new("slug", FormFieldType::Slug);
        let result = clean_field_value(&field, Some("not a slug!"));
        assert!(result.is_err());
    }

    #[test]
    fn test_ip_address_field_valid() {
        let field = FormFieldDef::new("ip", FormFieldType::IpAddress);
        assert!(clean_field_value(&field, Some("192.168.1.1")).is_ok());
        assert!(clean_field_value(&field, Some("::1")).is_ok());
    }

    #[test]
    fn test_ip_address_field_invalid() {
        let field = FormFieldDef::new("ip", FormFieldType::IpAddress);
        let result = clean_field_value(&field, Some("999.999.999.999"));
        assert!(result.is_err());
    }

    #[test]
    fn test_choice_field_valid() {
        let field = FormFieldDef::new(
            "color",
            FormFieldType::Choice {
                choices: vec![("red".into(), "Red".into()), ("blue".into(), "Blue".into())],
            },
        );
        let result = clean_field_value(&field, Some("red"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_choice_field_invalid() {
        let field = FormFieldDef::new(
            "color",
            FormFieldType::Choice {
                choices: vec![("red".into(), "Red".into())],
            },
        );
        let result = clean_field_value(&field, Some("green"));
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("valid choice"));
    }

    #[test]
    fn test_multiple_choice_field() {
        let field = FormFieldDef::new(
            "colors",
            FormFieldType::MultipleChoice {
                choices: vec![
                    ("red".into(), "Red".into()),
                    ("blue".into(), "Blue".into()),
                    ("green".into(), "Green".into()),
                ],
            },
        );
        let result = clean_field_value(&field, Some("red,blue"));
        assert!(result.is_ok());
        if let Value::List(vals) = result.unwrap() {
            assert_eq!(vals.len(), 2);
        } else {
            panic!("Expected List value");
        }
    }

    #[test]
    fn test_json_field_valid() {
        let field = FormFieldDef::new("data", FormFieldType::Json);
        let result = clean_field_value(&field, Some(r#"{"key": "value"}"#));
        assert!(result.is_ok());
    }

    #[test]
    fn test_json_field_invalid() {
        let field = FormFieldDef::new("data", FormFieldType::Json);
        let result = clean_field_value(&field, Some("not json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_regex_field_valid() {
        let field = FormFieldDef::new(
            "code",
            FormFieldType::Regex {
                regex: r"^[A-Z]{3}\d{3}$".to_string(),
            },
        );
        let result = clean_field_value(&field, Some("ABC123"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_regex_field_invalid() {
        let field = FormFieldDef::new(
            "code",
            FormFieldType::Regex {
                regex: r"^[A-Z]{3}\d{3}$".to_string(),
            },
        );
        let result = clean_field_value(&field, Some("abc"));
        assert!(result.is_err());
    }

    #[test]
    fn test_required_field_empty() {
        let field = FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        );
        let result = clean_field_value(&field, Some(""));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0], "This field is required.");
    }

    #[test]
    fn test_required_field_none() {
        let field = FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        );
        let result = clean_field_value(&field, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_optional_field_empty() {
        let field = FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        )
        .required(false);
        let result = clean_field_value(&field, Some(""));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Null);
    }

    #[test]
    fn test_custom_error_message() {
        let field = FormFieldDef::new(
            "name",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        )
        .error_message("required", "Please enter your name.");
        let result = clean_field_value(&field, Some(""));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0], "Please enter your name.");
    }

    #[test]
    fn test_field_builder_chain() {
        let field = FormFieldDef::new("email", FormFieldType::Email)
            .required(true)
            .label("Email Address")
            .help_text("Enter a valid email")
            .widget(WidgetType::EmailInput)
            .disabled(false);
        assert_eq!(field.label, "Email Address");
        assert_eq!(field.help_text, "Enter a valid email");
        assert_eq!(field.widget, WidgetType::EmailInput);
        assert!(field.required);
        assert!(!field.disabled);
    }

    #[test]
    fn test_default_widget_for_field_type() {
        assert_eq!(
            default_widget_for_field_type(&FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: true,
            }),
            WidgetType::TextInput
        );
        assert_eq!(
            default_widget_for_field_type(&FormFieldType::Integer {
                min_value: None,
                max_value: None,
            }),
            WidgetType::NumberInput
        );
        assert_eq!(
            default_widget_for_field_type(&FormFieldType::Boolean),
            WidgetType::CheckboxInput
        );
        assert_eq!(
            default_widget_for_field_type(&FormFieldType::Email),
            WidgetType::EmailInput
        );
        assert_eq!(
            default_widget_for_field_type(&FormFieldType::Date),
            WidgetType::DateInput
        );
        assert_eq!(
            default_widget_for_field_type(&FormFieldType::Json),
            WidgetType::Textarea
        );
    }

    #[test]
    fn test_file_field_extension_check() {
        let field = FormFieldDef::new(
            "upload",
            FormFieldType::File {
                max_size: None,
                allowed_extensions: vec!["pdf".into(), "doc".into()],
            },
        );
        assert!(clean_field_value(&field, Some("report.pdf")).is_ok());
        let result = clean_field_value(&field, Some("image.png"));
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("not allowed"));
    }

    #[test]
    fn test_image_field_valid() {
        let field = FormFieldDef::new("photo", FormFieldType::Image);
        assert!(clean_field_value(&field, Some("photo.jpg")).is_ok());
        assert!(clean_field_value(&field, Some("photo.png")).is_ok());
    }

    #[test]
    fn test_image_field_invalid() {
        let field = FormFieldDef::new("photo", FormFieldType::Image);
        let result = clean_field_value(&field, Some("document.pdf"));
        assert!(result.is_err());
    }

    #[test]
    fn test_typed_choice_field() {
        let field = FormFieldDef::new(
            "priority",
            FormFieldType::TypedChoice {
                choices: vec![
                    ("1".into(), "Low".into()),
                    ("2".into(), "Medium".into()),
                    ("3".into(), "High".into()),
                ],
                coerce: |s| {
                    s.parse::<i64>()
                        .map(Value::Int)
                        .map_err(|e| DjangoError::BadRequest(e.to_string()))
                },
            },
        );
        let result = clean_field_value(&field, Some("2"));
        assert_eq!(result.unwrap(), Value::Int(2));
    }

    #[test]
    fn test_optional_field_with_initial() {
        let field = FormFieldDef::new(
            "status",
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        )
        .required(false)
        .initial(Value::String("active".into()));
        let result = clean_field_value(&field, Some(""));
        assert_eq!(result.unwrap(), Value::String("active".to_string()));
    }
}
