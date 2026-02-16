//! Serialization framework for django-rs.
//!
//! This module provides the [`Serializer`] trait and built-in implementations
//! for data import/export. It mirrors Django's `django.core.serializers` module.
//!
//! ## Serializers
//!
//! - [`JsonSerializer`] - Compact JSON serialization
//! - [`PrettyJsonSerializer`] - Pretty-printed JSON serialization

use django_rs_core::DjangoError;

/// A serializer for converting data to and from string representations.
///
/// Used by management commands like `dumpdata` and `loaddata` for
/// data import/export. All implementations must be `Send + Sync`.
pub trait Serializer: Send + Sync {
    /// Serializes a slice of JSON objects into a string.
    fn serialize(&self, objects: &[serde_json::Value]) -> Result<String, DjangoError>;

    /// Deserializes a string into a vector of JSON objects.
    fn deserialize(&self, data: &str) -> Result<Vec<serde_json::Value>, DjangoError>;
}

/// Compact JSON serializer.
///
/// Produces minimal JSON without extra whitespace.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonSerializer;

impl Serializer for JsonSerializer {
    fn serialize(&self, objects: &[serde_json::Value]) -> Result<String, DjangoError> {
        serde_json::to_string(objects).map_err(|e| DjangoError::SerializationError(e.to_string()))
    }

    fn deserialize(&self, data: &str) -> Result<Vec<serde_json::Value>, DjangoError> {
        serde_json::from_str(data).map_err(|e| DjangoError::SerializationError(e.to_string()))
    }
}

/// Pretty-printed JSON serializer.
///
/// Produces human-readable JSON with indentation.
#[derive(Debug, Clone, Copy, Default)]
pub struct PrettyJsonSerializer;

impl Serializer for PrettyJsonSerializer {
    fn serialize(&self, objects: &[serde_json::Value]) -> Result<String, DjangoError> {
        serde_json::to_string_pretty(objects)
            .map_err(|e| DjangoError::SerializationError(e.to_string()))
    }

    fn deserialize(&self, data: &str) -> Result<Vec<serde_json::Value>, DjangoError> {
        serde_json::from_str(data).map_err(|e| DjangoError::SerializationError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_serializer_serialize() {
        let serializer = JsonSerializer;
        let objects = vec![
            json!({"model": "auth.user", "pk": 1, "fields": {"username": "admin"}}),
            json!({"model": "auth.user", "pk": 2, "fields": {"username": "alice"}}),
        ];

        let result = serializer.serialize(&objects).unwrap();
        assert!(result.contains("\"username\":\"admin\""));
        assert!(result.contains("\"pk\":2"));
    }

    #[test]
    fn test_json_serializer_deserialize() {
        let serializer = JsonSerializer;
        let data = r#"[{"pk":1,"name":"test"},{"pk":2,"name":"other"}]"#;

        let objects = serializer.deserialize(data).unwrap();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0]["pk"], 1);
        assert_eq!(objects[1]["name"], "other");
    }

    #[test]
    fn test_json_serializer_roundtrip() {
        let s = JsonSerializer;
        let objects = vec![
            json!({"id": 1, "value": "hello"}),
            json!({"id": 2, "value": "world"}),
        ];

        let encoded = s.serialize(&objects).unwrap();
        let decoded = s.deserialize(&encoded).unwrap();
        assert_eq!(objects, decoded);
    }

    #[test]
    fn test_json_serializer_empty_array() {
        let serializer = JsonSerializer;
        let result = serializer.serialize(&[]).unwrap();
        assert_eq!(result, "[]");

        let objects = serializer.deserialize("[]").unwrap();
        assert!(objects.is_empty());
    }

    #[test]
    fn test_json_serializer_deserialize_invalid() {
        let serializer = JsonSerializer;
        let result = serializer.deserialize("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_pretty_json_serializer_serialize() {
        let serializer = PrettyJsonSerializer;
        let objects = vec![json!({"key": "value"})];

        let result = serializer.serialize(&objects).unwrap();
        assert!(result.contains('\n'));
        assert!(result.contains("  "));
        assert!(result.contains("\"key\": \"value\""));
    }

    #[test]
    fn test_pretty_json_serializer_deserialize() {
        let serializer = PrettyJsonSerializer;
        let data = "[\n  {\"pk\": 1}\n]";

        let objects = serializer.deserialize(data).unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0]["pk"], 1);
    }

    #[test]
    fn test_pretty_json_serializer_roundtrip() {
        let s = PrettyJsonSerializer;
        let objects = vec![json!({"a": 1}), json!({"b": 2})];

        let encoded = s.serialize(&objects).unwrap();
        let decoded = s.deserialize(&encoded).unwrap();
        assert_eq!(objects, decoded);
    }

    #[test]
    fn test_pretty_json_serializer_deserialize_invalid() {
        let serializer = PrettyJsonSerializer;
        let result = serializer.deserialize("{not an array}");
        assert!(result.is_err());
    }

    #[test]
    fn test_json_serializer_default() {
        let serializer = JsonSerializer;
        assert!(serializer.serialize(&[]).is_ok());
    }

    #[test]
    fn test_pretty_json_serializer_default() {
        let serializer = PrettyJsonSerializer;
        assert!(serializer.serialize(&[]).is_ok());
    }
}
