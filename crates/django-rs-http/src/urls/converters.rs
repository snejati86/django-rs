//! Path converters for URL pattern matching.
//!
//! This module provides the [`PathConverter`] trait and built-in converters
//! that mirror Django's `django.urls.converters` module. Converters define
//! how URL path segments are matched and converted to typed values.
//!
//! # Built-in converters
//!
//! | Name   | Regex                                  | Rust type |
//! |--------|----------------------------------------|-----------|
//! | `int`  | `[0-9]+`                               | `i64`     |
//! | `str`  | `[^/]+`                                | `String`  |
//! | `slug` | `[-a-zA-Z0-9_]+`                       | `String`  |
//! | `uuid` | `[0-9a-f]{8}-...-[0-9a-f]{12}`         | `Uuid`    |
//! | `path` | `.+`                                   | `String`  |

use std::fmt;

use django_rs_core::{DjangoError, DjangoResult};

/// A typed value extracted from a URL path segment.
///
/// Each variant corresponds to one of the built-in path converters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathValue {
    /// An integer value, produced by [`IntConverter`].
    Int(i64),
    /// A string value (no slashes), produced by [`StrConverter`].
    Str(String),
    /// A slug value (letters, digits, hyphens, underscores), produced by [`SlugConverter`].
    Slug(String),
    /// A UUID value, produced by [`UuidConverter`].
    Uuid(uuid::Uuid),
    /// A path value (may contain slashes), produced by [`PathSegmentConverter`].
    Path(String),
}

impl fmt::Display for PathValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Str(v) | Self::Slug(v) | Self::Path(v) => write!(f, "{v}"),
            Self::Uuid(v) => write!(f, "{v}"),
        }
    }
}

/// Trait for converting URL path segments to typed Rust values and back.
///
/// Implementations must provide a regex pattern for matching and methods
/// for converting between string representations and [`PathValue`] instances.
///
/// This mirrors Django's `django.urls.converters.StringConverter` and friends.
pub trait PathConverter: Send + Sync + fmt::Debug {
    /// Returns the regex pattern that matches valid values for this converter.
    fn regex(&self) -> &'static str;

    /// Converts a matched string segment into a typed [`PathValue`].
    ///
    /// # Errors
    ///
    /// Returns a [`DjangoError`] if the value cannot be parsed.
    fn to_rust(&self, value: &str) -> DjangoResult<PathValue>;

    /// Converts a [`PathValue`] back into a URL-safe string.
    ///
    /// # Errors
    ///
    /// Returns a [`DjangoError`] if the value cannot be serialized.
    fn to_url(&self, value: &PathValue) -> DjangoResult<String>;
}

/// Converter for integer path segments.
///
/// Matches one or more digits and converts them to `i64`.
///
/// Django equivalent: `django.urls.converters.IntConverter`
#[derive(Debug, Clone, Copy)]
pub struct IntConverter;

impl PathConverter for IntConverter {
    fn regex(&self) -> &'static str {
        "[0-9]+"
    }

    fn to_rust(&self, value: &str) -> DjangoResult<PathValue> {
        value.parse::<i64>().map(PathValue::Int).map_err(|_| {
            DjangoError::BadRequest(format!("Invalid integer value: {value}"))
        })
    }

    fn to_url(&self, value: &PathValue) -> DjangoResult<String> {
        match value {
            PathValue::Int(v) => Ok(v.to_string()),
            _ => Err(DjangoError::BadRequest(
                "IntConverter expects a PathValue::Int".to_string(),
            )),
        }
    }
}

/// Converter for string path segments (no slashes).
///
/// Matches any non-empty string that does not contain `/`.
///
/// Django equivalent: `django.urls.converters.StringConverter`
#[derive(Debug, Clone, Copy)]
pub struct StrConverter;

impl PathConverter for StrConverter {
    fn regex(&self) -> &'static str {
        "[^/]+"
    }

    fn to_rust(&self, value: &str) -> DjangoResult<PathValue> {
        if value.is_empty() {
            return Err(DjangoError::BadRequest(
                "String converter requires a non-empty value".to_string(),
            ));
        }
        Ok(PathValue::Str(value.to_string()))
    }

    fn to_url(&self, value: &PathValue) -> DjangoResult<String> {
        match value {
            PathValue::Str(v) => Ok(v.clone()),
            _ => Err(DjangoError::BadRequest(
                "StrConverter expects a PathValue::Str".to_string(),
            )),
        }
    }
}

/// Converter for slug path segments.
///
/// Matches strings containing only ASCII letters, digits, hyphens, and underscores.
///
/// Django equivalent: `django.urls.converters.SlugConverter`
#[derive(Debug, Clone, Copy)]
pub struct SlugConverter;

impl PathConverter for SlugConverter {
    fn regex(&self) -> &'static str {
        "[-a-zA-Z0-9_]+"
    }

    fn to_rust(&self, value: &str) -> DjangoResult<PathValue> {
        if value.is_empty() {
            return Err(DjangoError::BadRequest(
                "Slug converter requires a non-empty value".to_string(),
            ));
        }
        Ok(PathValue::Slug(value.to_string()))
    }

    fn to_url(&self, value: &PathValue) -> DjangoResult<String> {
        match value {
            PathValue::Slug(v) => Ok(v.clone()),
            _ => Err(DjangoError::BadRequest(
                "SlugConverter expects a PathValue::Slug".to_string(),
            )),
        }
    }
}

/// Converter for UUID path segments.
///
/// Matches standard UUID format (`8-4-4-4-12` hex digits).
///
/// Django equivalent: `django.urls.converters.UUIDConverter`
#[derive(Debug, Clone, Copy)]
pub struct UuidConverter;

impl PathConverter for UuidConverter {
    fn regex(&self) -> &'static str {
        "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
    }

    fn to_rust(&self, value: &str) -> DjangoResult<PathValue> {
        value
            .parse::<uuid::Uuid>()
            .map(PathValue::Uuid)
            .map_err(|_| DjangoError::BadRequest(format!("Invalid UUID: {value}")))
    }

    fn to_url(&self, value: &PathValue) -> DjangoResult<String> {
        match value {
            PathValue::Uuid(v) => Ok(v.to_string()),
            _ => Err(DjangoError::BadRequest(
                "UuidConverter expects a PathValue::Uuid".to_string(),
            )),
        }
    }
}

/// Converter for path segments that may contain slashes.
///
/// Matches any non-empty string, including forward slashes.
/// This is useful for capturing the remainder of a URL path.
///
/// Django equivalent: `django.urls.converters.PathConverter`
#[derive(Debug, Clone, Copy)]
pub struct PathSegmentConverter;

impl PathConverter for PathSegmentConverter {
    fn regex(&self) -> &'static str {
        ".+"
    }

    fn to_rust(&self, value: &str) -> DjangoResult<PathValue> {
        if value.is_empty() {
            return Err(DjangoError::BadRequest(
                "Path converter requires a non-empty value".to_string(),
            ));
        }
        Ok(PathValue::Path(value.to_string()))
    }

    fn to_url(&self, value: &PathValue) -> DjangoResult<String> {
        match value {
            PathValue::Path(v) => Ok(v.clone()),
            _ => Err(DjangoError::BadRequest(
                "PathSegmentConverter expects a PathValue::Path".to_string(),
            )),
        }
    }
}

/// Creates a boxed path converter for the given type name.
///
/// # Supported types
///
/// - `"int"` -> [`IntConverter`]
/// - `"str"` -> [`StrConverter`]
/// - `"slug"` -> [`SlugConverter`]
/// - `"uuid"` -> [`UuidConverter`]
/// - `"path"` -> [`PathSegmentConverter`]
///
/// # Errors
///
/// Returns a [`DjangoError::ImproperlyConfigured`] if the type name is not recognized.
pub fn get_converter(type_name: &str) -> DjangoResult<Box<dyn PathConverter>> {
    match type_name {
        "int" => Ok(Box::new(IntConverter)),
        "str" => Ok(Box::new(StrConverter)),
        "slug" => Ok(Box::new(SlugConverter)),
        "uuid" => Ok(Box::new(UuidConverter)),
        "path" => Ok(Box::new(PathSegmentConverter)),
        _ => Err(DjangoError::ImproperlyConfigured(format!(
            "Unknown path converter type: {type_name}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int_converter_to_rust() {
        let conv = IntConverter;
        assert_eq!(conv.to_rust("42").unwrap(), PathValue::Int(42));
        assert_eq!(conv.to_rust("0").unwrap(), PathValue::Int(0));
        assert!(conv.to_rust("abc").is_err());
        assert!(conv.to_rust("").is_err());
    }

    #[test]
    fn test_int_converter_to_url() {
        let conv = IntConverter;
        assert_eq!(conv.to_url(&PathValue::Int(42)).unwrap(), "42");
        assert!(conv.to_url(&PathValue::Str("x".into())).is_err());
    }

    #[test]
    fn test_int_converter_regex() {
        let conv = IntConverter;
        let re = regex::Regex::new(conv.regex()).unwrap();
        assert!(re.is_match("123"));
        assert!(!re.is_match("abc"));
    }

    #[test]
    fn test_str_converter_to_rust() {
        let conv = StrConverter;
        assert_eq!(
            conv.to_rust("hello").unwrap(),
            PathValue::Str("hello".to_string())
        );
        assert!(conv.to_rust("").is_err());
    }

    #[test]
    fn test_str_converter_to_url() {
        let conv = StrConverter;
        assert_eq!(
            conv.to_url(&PathValue::Str("hello".into())).unwrap(),
            "hello"
        );
        assert!(conv.to_url(&PathValue::Int(1)).is_err());
    }

    #[test]
    fn test_str_converter_regex() {
        let conv = StrConverter;
        let re = regex::Regex::new(conv.regex()).unwrap();
        assert!(re.is_match("hello"));
        assert!(!re.is_match(""));
        // The regex itself matches any non-slash char; slash is excluded
        assert!(!re.is_match("/"));
    }

    #[test]
    fn test_slug_converter_to_rust() {
        let conv = SlugConverter;
        assert_eq!(
            conv.to_rust("my-slug_1").unwrap(),
            PathValue::Slug("my-slug_1".to_string())
        );
        assert!(conv.to_rust("").is_err());
    }

    #[test]
    fn test_slug_converter_to_url() {
        let conv = SlugConverter;
        assert_eq!(
            conv.to_url(&PathValue::Slug("my-slug".into())).unwrap(),
            "my-slug"
        );
        assert!(conv.to_url(&PathValue::Int(1)).is_err());
    }

    #[test]
    fn test_slug_converter_regex() {
        let conv = SlugConverter;
        let re = regex::Regex::new(conv.regex()).unwrap();
        assert!(re.is_match("hello-world"));
        assert!(re.is_match("hello_world"));
        assert!(!re.is_match(""));
    }

    #[test]
    fn test_uuid_converter_to_rust() {
        let conv = UuidConverter;
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let result = conv.to_rust(uuid_str).unwrap();
        assert_eq!(
            result,
            PathValue::Uuid(uuid_str.parse::<uuid::Uuid>().unwrap())
        );
        assert!(conv.to_rust("not-a-uuid").is_err());
    }

    #[test]
    fn test_uuid_converter_to_url() {
        let conv = UuidConverter;
        let uuid = "550e8400-e29b-41d4-a716-446655440000"
            .parse::<uuid::Uuid>()
            .unwrap();
        assert_eq!(
            conv.to_url(&PathValue::Uuid(uuid)).unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert!(conv.to_url(&PathValue::Int(1)).is_err());
    }

    #[test]
    fn test_uuid_converter_regex() {
        let conv = UuidConverter;
        let re = regex::Regex::new(conv.regex()).unwrap();
        assert!(re.is_match("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!re.is_match("not-a-uuid"));
    }

    #[test]
    fn test_path_converter_to_rust() {
        let conv = PathSegmentConverter;
        assert_eq!(
            conv.to_rust("a/b/c").unwrap(),
            PathValue::Path("a/b/c".to_string())
        );
        assert!(conv.to_rust("").is_err());
    }

    #[test]
    fn test_path_converter_to_url() {
        let conv = PathSegmentConverter;
        assert_eq!(
            conv.to_url(&PathValue::Path("a/b/c".into())).unwrap(),
            "a/b/c"
        );
        assert!(conv.to_url(&PathValue::Int(1)).is_err());
    }

    #[test]
    fn test_path_converter_regex() {
        let conv = PathSegmentConverter;
        let re = regex::Regex::new(conv.regex()).unwrap();
        assert!(re.is_match("a/b/c"));
        assert!(re.is_match("single"));
        assert!(!re.is_match(""));
    }

    #[test]
    fn test_get_converter_known_types() {
        assert!(get_converter("int").is_ok());
        assert!(get_converter("str").is_ok());
        assert!(get_converter("slug").is_ok());
        assert!(get_converter("uuid").is_ok());
        assert!(get_converter("path").is_ok());
    }

    #[test]
    fn test_get_converter_unknown_type() {
        let result = get_converter("unknown");
        assert!(result.is_err());
    }

    #[test]
    fn test_path_value_display() {
        assert_eq!(PathValue::Int(42).to_string(), "42");
        assert_eq!(PathValue::Str("hello".into()).to_string(), "hello");
        assert_eq!(PathValue::Slug("my-slug".into()).to_string(), "my-slug");
        assert_eq!(PathValue::Path("a/b".into()).to_string(), "a/b");
        let uuid = "550e8400-e29b-41d4-a716-446655440000"
            .parse::<uuid::Uuid>()
            .unwrap();
        assert_eq!(
            PathValue::Uuid(uuid).to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_int_converter_negative_not_matched_by_regex() {
        let conv = IntConverter;
        let re = regex::Regex::new(&format!("^{}$", conv.regex())).unwrap();
        // Regex only matches digits, so negative numbers fail at regex level
        assert!(!re.is_match("-5"));
    }

    #[test]
    fn test_int_converter_large_number() {
        let conv = IntConverter;
        assert_eq!(
            conv.to_rust("9999999999").unwrap(),
            PathValue::Int(9_999_999_999)
        );
    }
}
