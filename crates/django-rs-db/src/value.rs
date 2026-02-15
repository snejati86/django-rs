//! ORM value types for representing database values in a backend-agnostic way.
//!
//! The [`Value`] enum is the core type used throughout the ORM to represent
//! field values, query parameters, and results. It supports all common SQL
//! types and provides conversions from standard Rust types.

use std::fmt;

/// A backend-agnostic representation of a database value.
///
/// `Value` is the universal type used to pass data between the ORM layer
/// and database backends. It covers all standard SQL data types and maps
/// to the appropriate native types for each backend.
///
/// # Examples
///
/// ```
/// use django_rs_db::value::Value;
///
/// let v = Value::from(42_i64);
/// assert_eq!(v, Value::Int(42));
///
/// let v = Value::from("hello");
/// assert_eq!(v, Value::String("hello".to_string()));
/// ```
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    /// SQL NULL.
    Null,
    /// A boolean value.
    Bool(bool),
    /// A 64-bit signed integer.
    Int(i64),
    /// A 64-bit floating-point number.
    Float(f64),
    /// A UTF-8 string.
    String(String),
    /// Raw binary data.
    Bytes(Vec<u8>),
    /// A date without time.
    Date(chrono::NaiveDate),
    /// A date and time without timezone.
    DateTime(chrono::NaiveDateTime),
    /// A date and time with UTC timezone.
    DateTimeTz(chrono::DateTime<chrono::Utc>),
    /// A time without date.
    Time(chrono::NaiveTime),
    /// A duration / interval.
    Duration(chrono::Duration),
    /// A UUID value.
    Uuid(uuid::Uuid),
    /// A JSON value.
    Json(serde_json::Value),
    /// A list of values (for IN clauses, array fields, etc.).
    List(Vec<Value>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "NULL"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::String(s) => write!(f, "{s}"),
            Self::Bytes(b) => write!(f, "<{} bytes>", b.len()),
            Self::Date(d) => write!(f, "{d}"),
            Self::DateTime(dt) => write!(f, "{dt}"),
            Self::DateTimeTz(dt) => write!(f, "{dt}"),
            Self::Time(t) => write!(f, "{t}"),
            Self::Duration(d) => write!(f, "{d}"),
            Self::Uuid(u) => write!(f, "{u}"),
            Self::Json(j) => write!(f, "{j}"),
            Self::List(vals) => {
                write!(f, "[")?;
                for (i, v) in vals.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
        }
    }
}

// ── From implementations ───────────────────────────────────────────────

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Self::Int(i64::from(v))
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<i16> for Value {
    fn from(v: i16) -> Self {
        Self::Int(i64::from(v))
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Self::Float(f64::from(v))
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(v)
    }
}

impl From<chrono::NaiveDate> for Value {
    fn from(v: chrono::NaiveDate) -> Self {
        Self::Date(v)
    }
}

impl From<chrono::NaiveDateTime> for Value {
    fn from(v: chrono::NaiveDateTime) -> Self {
        Self::DateTime(v)
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Value {
    fn from(v: chrono::DateTime<chrono::Utc>) -> Self {
        Self::DateTimeTz(v)
    }
}

impl From<chrono::NaiveTime> for Value {
    fn from(v: chrono::NaiveTime) -> Self {
        Self::Time(v)
    }
}

impl From<chrono::Duration> for Value {
    fn from(v: chrono::Duration) -> Self {
        Self::Duration(v)
    }
}

impl From<uuid::Uuid> for Value {
    fn from(v: uuid::Uuid) -> Self {
        Self::Uuid(v)
    }
}

impl From<serde_json::Value> for Value {
    fn from(v: serde_json::Value) -> Self {
        Self::Json(v)
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Self::List(v)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(inner) => inner.into(),
            None => Self::Null,
        }
    }
}

impl Value {
    /// Returns `true` if this value is `Null`.
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Attempts to extract a boolean value.
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Attempts to extract an integer value.
    pub const fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Attempts to extract a float value.
    pub const fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Attempts to extract a string reference.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_bool() {
        assert_eq!(Value::from(true), Value::Bool(true));
        assert_eq!(Value::from(false), Value::Bool(false));
    }

    #[test]
    fn test_from_integers() {
        assert_eq!(Value::from(42_i32), Value::Int(42));
        assert_eq!(Value::from(42_i64), Value::Int(42));
        assert_eq!(Value::from(42_i16), Value::Int(42));
    }

    #[test]
    fn test_from_floats() {
        assert_eq!(Value::from(1.23_f64), Value::Float(1.23));
        assert_eq!(Value::from(1.23_f32), Value::Float(f64::from(1.23_f32)));
    }

    #[test]
    fn test_from_string() {
        assert_eq!(
            Value::from("hello"),
            Value::String("hello".to_string())
        );
        assert_eq!(
            Value::from("hello".to_string()),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn test_from_bytes() {
        assert_eq!(
            Value::from(vec![1_u8, 2, 3]),
            Value::Bytes(vec![1, 2, 3])
        );
    }

    #[test]
    fn test_from_option() {
        let some_val: Option<i64> = Some(42);
        assert_eq!(Value::from(some_val), Value::Int(42));

        let none_val: Option<i64> = None;
        assert_eq!(Value::from(none_val), Value::Null);
    }

    #[test]
    fn test_from_uuid() {
        let u = uuid::Uuid::new_v4();
        assert_eq!(Value::from(u), Value::Uuid(u));
    }

    #[test]
    fn test_from_json() {
        let j = serde_json::json!({"key": "value"});
        assert_eq!(Value::from(j.clone()), Value::Json(j));
    }

    #[test]
    fn test_from_chrono_date() {
        let d = chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        assert_eq!(Value::from(d), Value::Date(d));
    }

    #[test]
    fn test_from_chrono_datetime() {
        let dt = chrono::NaiveDate::from_ymd_opt(2024, 1, 15)
            .unwrap()
            .and_hms_opt(12, 30, 0)
            .unwrap();
        assert_eq!(Value::from(dt), Value::DateTime(dt));
    }

    #[test]
    fn test_from_chrono_time() {
        let t = chrono::NaiveTime::from_hms_opt(12, 30, 0).unwrap();
        assert_eq!(Value::from(t), Value::Time(t));
    }

    #[test]
    fn test_display_null() {
        assert_eq!(Value::Null.to_string(), "NULL");
    }

    #[test]
    fn test_display_bool() {
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Bool(false).to_string(), "false");
    }

    #[test]
    fn test_display_int() {
        assert_eq!(Value::Int(42).to_string(), "42");
    }

    #[test]
    fn test_display_float() {
        assert_eq!(Value::Float(1.23).to_string(), "1.23");
    }

    #[test]
    fn test_display_string() {
        assert_eq!(Value::String("hello".into()).to_string(), "hello");
    }

    #[test]
    fn test_display_bytes() {
        assert_eq!(Value::Bytes(vec![1, 2, 3]).to_string(), "<3 bytes>");
    }

    #[test]
    fn test_display_list() {
        let list = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(list.to_string(), "[1, 2, 3]");
    }

    #[test]
    fn test_is_null() {
        assert!(Value::Null.is_null());
        assert!(!Value::Int(0).is_null());
    }

    #[test]
    fn test_as_bool() {
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::Int(1).as_bool(), None);
    }

    #[test]
    fn test_as_int() {
        assert_eq!(Value::Int(42).as_int(), Some(42));
        assert_eq!(Value::Bool(true).as_int(), None);
    }

    #[test]
    fn test_as_float() {
        assert_eq!(Value::Float(1.23).as_float(), Some(1.23));
        assert_eq!(Value::Int(3).as_float(), None);
    }

    #[test]
    fn test_as_str() {
        assert_eq!(Value::String("hello".into()).as_str(), Some("hello"));
        assert_eq!(Value::Int(1).as_str(), None);
    }

    #[test]
    fn test_from_list() {
        let vals = vec![Value::Int(1), Value::Int(2)];
        assert_eq!(Value::from(vals.clone()), Value::List(vals));
    }

    #[test]
    fn test_display_uuid() {
        let u = uuid::Uuid::nil();
        assert_eq!(
            Value::Uuid(u).to_string(),
            "00000000-0000-0000-0000-000000000000"
        );
    }
}
