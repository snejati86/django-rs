//! Template context for variable resolution and rendering.
//!
//! Provides [`Context`] for holding template variables in a stack-based scope,
//! and [`ContextValue`] for representing dynamic template values.

use std::collections::HashMap;
use std::fmt;

/// Represents a dynamic value in a template context.
///
/// This enum covers all value types that can appear in Django templates,
/// including strings, numbers, booleans, lists, dictionaries, and `None`.
#[derive(Debug, Clone)]
pub enum ContextValue {
    /// A string value. If `safe` is true, auto-escaping is bypassed.
    String(String),
    /// A 64-bit integer.
    Integer(i64),
    /// A 64-bit floating point number.
    Float(f64),
    /// A boolean value.
    Bool(bool),
    /// An ordered list of values.
    List(Vec<ContextValue>),
    /// A key-value mapping.
    Dict(HashMap<String, ContextValue>),
    /// The absence of a value (Python's `None`).
    None,
    /// A string marked as safe — auto-escaping will not be applied.
    SafeString(String),
}

impl ContextValue {
    /// Returns `true` if this value is considered "truthy" in Django template logic.
    ///
    /// - `None` is falsy
    /// - Empty strings, empty lists, empty dicts are falsy
    /// - `Bool(false)` is falsy
    /// - `Integer(0)` and `Float(0.0)` are falsy
    /// - Everything else is truthy
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(b) => *b,
            Self::Integer(i) => *i != 0,
            Self::Float(f) => *f != 0.0,
            Self::String(s) | Self::SafeString(s) => !s.is_empty(),
            Self::List(l) => !l.is_empty(),
            Self::Dict(d) => !d.is_empty(),
        }
    }

    /// Converts this value to a display string (without HTML escaping).
    pub fn to_display_string(&self) -> String {
        match self {
            Self::String(s) | Self::SafeString(s) => s.clone(),
            Self::Integer(i) => i.to_string(),
            Self::Float(f) => {
                // Format like Python: if integer-valued, still show decimal
                if f.fract() == 0.0 {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            Self::Bool(b) => {
                if *b {
                    "True".to_string()
                } else {
                    "False".to_string()
                }
            }
            Self::List(items) => {
                let inner: Vec<String> = items.iter().map(|v| v.to_repr()).collect();
                format!("[{}]", inner.join(", "))
            }
            Self::Dict(map) => {
                let inner: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("'{}': {}", k, v.to_repr()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            Self::None => String::new(),
        }
    }

    /// Returns a Python-like repr string.
    fn to_repr(&self) -> String {
        match self {
            Self::String(s) | Self::SafeString(s) => format!("'{s}'"),
            Self::Integer(i) => i.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Bool(b) => {
                if *b {
                    "True".to_string()
                } else {
                    "False".to_string()
                }
            }
            Self::None => "None".to_string(),
            Self::List(items) => {
                let inner: Vec<String> = items.iter().map(|v| v.to_repr()).collect();
                format!("[{}]", inner.join(", "))
            }
            Self::Dict(map) => {
                let inner: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("'{}': {}", k, v.to_repr()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
        }
    }

    /// Returns `true` if this value is a safe string (auto-escaping bypassed).
    pub fn is_safe(&self) -> bool {
        matches!(self, Self::SafeString(_))
    }

    /// Marks a string value as safe, bypassing auto-escaping.
    #[must_use]
    pub fn mark_safe(self) -> Self {
        match self {
            Self::String(s) => Self::SafeString(s),
            other => other,
        }
    }

    /// Resolves a dot-separated path on this value (e.g., `user.name`, `items.0`).
    pub fn resolve_path(&self, key: &str) -> Option<&ContextValue> {
        match self {
            Self::Dict(map) => map.get(key),
            Self::List(list) => {
                if let Ok(idx) = key.parse::<usize>() {
                    list.get(idx)
                } else {
                    // Support 'length' on lists
                    None
                }
            }
            _ => None,
        }
    }

    /// Returns the length of a list, string, or dict.
    pub fn len(&self) -> Option<usize> {
        match self {
            Self::String(s) | Self::SafeString(s) => Some(s.len()),
            Self::List(l) => Some(l.len()),
            Self::Dict(d) => Some(d.len()),
            _ => None,
        }
    }

    /// Returns `true` if this is an empty collection or empty string.
    pub fn is_empty(&self) -> Option<bool> {
        self.len().map(|l| l == 0)
    }

    /// Attempts to convert this value to an i64.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            Self::Float(f) => Some(*f as i64),
            Self::String(s) | Self::SafeString(s) => s.parse::<i64>().ok(),
            Self::Bool(b) => Some(i64::from(*b)),
            _ => None,
        }
    }

    /// Attempts to convert this value to an f64.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Integer(i) => Some(*i as f64),
            Self::String(s) | Self::SafeString(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    /// Returns the string contents if this is a String or SafeString.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) | Self::SafeString(s) => Some(s),
            _ => None,
        }
    }
}

impl fmt::Display for ContextValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

impl PartialEq for ContextValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::String(a), Self::String(b))
            | (Self::SafeString(a), Self::SafeString(b))
            | (Self::String(a), Self::SafeString(b))
            | (Self::SafeString(a), Self::String(b)) => a == b,
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Integer(a), Self::Float(b)) | (Self::Float(b), Self::Integer(a)) => {
                (*a as f64) == *b
            }
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::None, Self::None) => true,
            (Self::List(a), Self::List(b)) => a == b,
            (Self::Dict(a), Self::Dict(b)) => a == b,
            _ => false,
        }
    }
}

// -- From implementations --

impl From<&str> for ContextValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for ContextValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<i32> for ContextValue {
    fn from(i: i32) -> Self {
        Self::Integer(i64::from(i))
    }
}

impl From<i64> for ContextValue {
    fn from(i: i64) -> Self {
        Self::Integer(i)
    }
}

impl From<u32> for ContextValue {
    fn from(i: u32) -> Self {
        Self::Integer(i64::from(i))
    }
}

impl From<u64> for ContextValue {
    fn from(i: u64) -> Self {
        Self::Integer(i as i64)
    }
}

impl From<usize> for ContextValue {
    fn from(i: usize) -> Self {
        Self::Integer(i as i64)
    }
}

impl From<f32> for ContextValue {
    fn from(f: f32) -> Self {
        Self::Float(f64::from(f))
    }
}

impl From<f64> for ContextValue {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

impl From<bool> for ContextValue {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl<T: Into<ContextValue>> From<Vec<T>> for ContextValue {
    fn from(v: Vec<T>) -> Self {
        Self::List(v.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<ContextValue>> From<HashMap<String, T>> for ContextValue {
    fn from(m: HashMap<String, T>) -> Self {
        Self::Dict(m.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}

impl<T: Into<ContextValue>> From<Option<T>> for ContextValue {
    fn from(o: Option<T>) -> Self {
        match o {
            Some(v) => v.into(),
            None => Self::None,
        }
    }
}

impl From<serde_json::Value> for ContextValue {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Self::None,
            serde_json::Value::Bool(b) => Self::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Self::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    Self::Float(f)
                } else {
                    Self::None
                }
            }
            serde_json::Value::String(s) => Self::String(s),
            serde_json::Value::Array(arr) => {
                Self::List(arr.into_iter().map(ContextValue::from).collect())
            }
            serde_json::Value::Object(map) => Self::Dict(
                map.into_iter()
                    .map(|(k, v)| (k, ContextValue::from(v)))
                    .collect(),
            ),
        }
    }
}

/// A template context that holds variables in a stack of scopes.
///
/// The context supports push/pop semantics for nested scopes (e.g., inside
/// `{% for %}` or `{% with %}` blocks). Variable lookup searches from the
/// top of the stack downward.
///
/// # Examples
///
/// ```
/// use django_rs_template::context::{Context, ContextValue};
///
/// let mut ctx = Context::new();
/// ctx.set("name", ContextValue::from("Django"));
/// assert_eq!(ctx.get("name").unwrap().to_display_string(), "Django");
///
/// ctx.push();
/// ctx.set("name", ContextValue::from("Overridden"));
/// assert_eq!(ctx.get("name").unwrap().to_display_string(), "Overridden");
///
/// ctx.pop();
/// assert_eq!(ctx.get("name").unwrap().to_display_string(), "Django");
/// ```
pub struct Context {
    stack: Vec<HashMap<String, ContextValue>>,
    auto_escape: bool,
}

impl Context {
    /// Creates a new empty context with a single scope.
    pub fn new() -> Self {
        Self {
            stack: vec![HashMap::new()],
            auto_escape: true,
        }
    }

    /// Pushes a new scope onto the context stack.
    pub fn push(&mut self) {
        self.stack.push(HashMap::new());
    }

    /// Pops the top scope from the context stack.
    ///
    /// If only one scope remains, this is a no-op.
    pub fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    /// Sets a variable in the current (top) scope.
    pub fn set(&mut self, key: impl Into<String>, value: ContextValue) {
        if let Some(top) = self.stack.last_mut() {
            top.insert(key.into(), value);
        }
    }

    /// Looks up a variable by name, searching from the top scope downward.
    ///
    /// Supports dot-separated paths like `user.name` or `items.0.title`.
    pub fn get(&self, key: &str) -> Option<&ContextValue> {
        // Split on dots for path resolution
        let parts: Vec<&str> = key.split('.').collect();
        let root_key = parts[0];

        // Find root value in scope stack
        let mut value = None;
        for scope in self.stack.iter().rev() {
            if let Some(v) = scope.get(root_key) {
                value = Some(v);
                break;
            }
        }

        // Resolve remaining path segments
        let mut current = value?;
        for part in &parts[1..] {
            match current.resolve_path(part) {
                Some(v) => current = v,
                None => return None,
            }
        }

        Some(current)
    }

    /// Returns whether auto-escaping is enabled.
    pub fn auto_escape(&self) -> bool {
        self.auto_escape
    }

    /// Sets whether auto-escaping is enabled.
    pub fn set_auto_escape(&mut self, enabled: bool) {
        self.auto_escape = enabled;
    }

    /// Flattens all scopes into a single map, with later scopes overriding earlier ones.
    pub fn flatten(&self) -> HashMap<String, ContextValue> {
        let mut result = HashMap::new();
        for scope in &self.stack {
            for (k, v) in scope {
                result.insert(k.clone(), v.clone());
            }
        }
        result
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

/// Marks a string value as safe, bypassing HTML auto-escaping.
///
/// This is the equivalent of Django's `mark_safe()`.
pub fn mark_safe(value: ContextValue) -> ContextValue {
    value.mark_safe()
}

/// Escapes HTML special characters in a string.
///
/// Replaces `&`, `<`, `>`, `"`, and `'` with their HTML entity equivalents.
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_value_from_str() {
        let v: ContextValue = "hello".into();
        assert_eq!(v.to_display_string(), "hello");
    }

    #[test]
    fn test_context_value_from_string() {
        let v: ContextValue = String::from("world").into();
        assert_eq!(v.to_display_string(), "world");
    }

    #[test]
    fn test_context_value_from_i32() {
        let v: ContextValue = 42i32.into();
        assert_eq!(v.to_display_string(), "42");
    }

    #[test]
    fn test_context_value_from_i64() {
        let v: ContextValue = 100i64.into();
        assert_eq!(v.to_display_string(), "100");
    }

    #[test]
    fn test_context_value_from_f64() {
        let v: ContextValue = 3.14f64.into();
        assert_eq!(v.to_display_string(), "3.14");
    }

    #[test]
    fn test_context_value_from_bool() {
        let v: ContextValue = true.into();
        assert_eq!(v.to_display_string(), "True");
        let v: ContextValue = false.into();
        assert_eq!(v.to_display_string(), "False");
    }

    #[test]
    fn test_context_value_from_vec() {
        let v: ContextValue = vec![1i32, 2, 3].into();
        assert_eq!(v.to_display_string(), "[1, 2, 3]");
    }

    #[test]
    fn test_context_value_from_none() {
        let v: ContextValue = ContextValue::None;
        assert_eq!(v.to_display_string(), "");
    }

    #[test]
    fn test_context_value_from_option_some() {
        let v: ContextValue = Some(42i32).into();
        assert_eq!(v.to_display_string(), "42");
    }

    #[test]
    fn test_context_value_from_option_none() {
        let v: ContextValue = Option::<i32>::None.into();
        assert!(matches!(v, ContextValue::None));
    }

    #[test]
    fn test_context_value_from_json() {
        let json = serde_json::json!({
            "name": "Django",
            "version": 4,
            "active": true,
            "tags": ["web", "python"],
            "meta": null
        });
        let v: ContextValue = json.into();
        if let ContextValue::Dict(map) = &v {
            assert!(matches!(map.get("name"), Some(ContextValue::String(s)) if s == "Django"));
            assert!(matches!(map.get("version"), Some(ContextValue::Integer(4))));
            assert!(matches!(map.get("active"), Some(ContextValue::Bool(true))));
            assert!(matches!(map.get("meta"), Some(ContextValue::None)));
        } else {
            panic!("Expected Dict");
        }
    }

    #[test]
    fn test_context_value_truthiness() {
        assert!(ContextValue::Bool(true).is_truthy());
        assert!(!ContextValue::Bool(false).is_truthy());
        assert!(ContextValue::Integer(1).is_truthy());
        assert!(!ContextValue::Integer(0).is_truthy());
        assert!(ContextValue::String("hello".to_string()).is_truthy());
        assert!(!ContextValue::String(String::new()).is_truthy());
        assert!(!ContextValue::None.is_truthy());
        assert!(ContextValue::List(vec![ContextValue::Integer(1)]).is_truthy());
        assert!(!ContextValue::List(vec![]).is_truthy());
    }

    #[test]
    fn test_context_value_equality() {
        assert_eq!(ContextValue::Integer(1), ContextValue::Integer(1));
        assert_eq!(
            ContextValue::String("a".to_string()),
            ContextValue::String("a".to_string())
        );
        assert_eq!(ContextValue::None, ContextValue::None);
        assert_ne!(ContextValue::Integer(1), ContextValue::Integer(2));
    }

    #[test]
    fn test_context_value_safe_string() {
        let v = ContextValue::String("<b>bold</b>".to_string());
        assert!(!v.is_safe());
        let v = v.mark_safe();
        assert!(v.is_safe());
        assert_eq!(v.to_display_string(), "<b>bold</b>");
    }

    #[test]
    fn test_context_value_resolve_path_dict() {
        let mut inner = HashMap::new();
        inner.insert("name".to_string(), ContextValue::from("Alice"));
        let v = ContextValue::Dict(inner);
        assert_eq!(v.resolve_path("name").unwrap().to_display_string(), "Alice");
    }

    #[test]
    fn test_context_value_resolve_path_list() {
        let v = ContextValue::List(vec![
            ContextValue::from("a"),
            ContextValue::from("b"),
            ContextValue::from("c"),
        ]);
        assert_eq!(v.resolve_path("1").unwrap().to_display_string(), "b");
    }

    #[test]
    fn test_context_value_as_integer() {
        assert_eq!(ContextValue::Integer(42).as_integer(), Some(42));
        assert_eq!(ContextValue::Float(3.7).as_integer(), Some(3));
        assert_eq!(
            ContextValue::String("10".to_string()).as_integer(),
            Some(10)
        );
        assert_eq!(ContextValue::Bool(true).as_integer(), Some(1));
        assert_eq!(ContextValue::None.as_integer(), None);
    }

    #[test]
    fn test_context_value_as_float() {
        assert_eq!(ContextValue::Float(3.14).as_float(), Some(3.14));
        assert_eq!(ContextValue::Integer(42).as_float(), Some(42.0));
    }

    #[test]
    fn test_context_push_pop() {
        let mut ctx = Context::new();
        ctx.set("x", ContextValue::from(1i32));
        assert_eq!(ctx.get("x").unwrap().to_display_string(), "1");

        ctx.push();
        ctx.set("x", ContextValue::from(2i32));
        assert_eq!(ctx.get("x").unwrap().to_display_string(), "2");

        ctx.pop();
        assert_eq!(ctx.get("x").unwrap().to_display_string(), "1");
    }

    #[test]
    fn test_context_pop_minimum_scope() {
        let mut ctx = Context::new();
        ctx.set("x", ContextValue::from(1i32));
        ctx.pop(); // Should not pop the last scope
        assert_eq!(ctx.get("x").unwrap().to_display_string(), "1");
    }

    #[test]
    fn test_context_get_missing() {
        let ctx = Context::new();
        assert!(ctx.get("nonexistent").is_none());
    }

    #[test]
    fn test_context_dot_notation() {
        let mut ctx = Context::new();
        let mut user = HashMap::new();
        user.insert("name".to_string(), ContextValue::from("Alice"));
        user.insert("age".to_string(), ContextValue::from(30i32));
        ctx.set("user", ContextValue::Dict(user));

        assert_eq!(ctx.get("user.name").unwrap().to_display_string(), "Alice");
        assert_eq!(ctx.get("user.age").unwrap().to_display_string(), "30");
        assert!(ctx.get("user.email").is_none());
    }

    #[test]
    fn test_context_flatten() {
        let mut ctx = Context::new();
        ctx.set("a", ContextValue::from(1i32));
        ctx.push();
        ctx.set("b", ContextValue::from(2i32));
        ctx.set("a", ContextValue::from(10i32));

        let flat = ctx.flatten();
        assert_eq!(flat.get("a").unwrap().to_display_string(), "10");
        assert_eq!(flat.get("b").unwrap().to_display_string(), "2");
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<b>bold</b>"), "&lt;b&gt;bold&lt;/b&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html("\"quotes\""), "&quot;quotes&quot;");
        assert_eq!(escape_html("it's"), "it&#x27;s");
    }

    #[test]
    fn test_mark_safe() {
        let v = ContextValue::from("<b>safe</b>");
        let safe = mark_safe(v);
        assert!(safe.is_safe());
    }

    #[test]
    fn test_context_auto_escape_default() {
        let ctx = Context::new();
        assert!(ctx.auto_escape());
    }

    #[test]
    fn test_context_set_auto_escape() {
        let mut ctx = Context::new();
        ctx.set_auto_escape(false);
        assert!(!ctx.auto_escape());
    }

    #[test]
    fn test_context_value_len() {
        assert_eq!(ContextValue::String("hello".into()).len(), Some(5));
        assert_eq!(
            ContextValue::List(vec![ContextValue::Integer(1)]).len(),
            Some(1)
        );
        assert_eq!(ContextValue::Integer(42).len(), None);
    }

    #[test]
    fn test_context_nested_dot_notation() {
        let mut ctx = Context::new();
        let mut address = HashMap::new();
        address.insert("city".to_string(), ContextValue::from("NYC"));
        let mut user = HashMap::new();
        user.insert("address".to_string(), ContextValue::Dict(address));
        ctx.set("user", ContextValue::Dict(user));

        assert_eq!(
            ctx.get("user.address.city").unwrap().to_display_string(),
            "NYC"
        );
    }

    #[test]
    fn test_context_list_index_dot_notation() {
        let mut ctx = Context::new();
        ctx.set(
            "items",
            ContextValue::List(vec![
                ContextValue::from("first"),
                ContextValue::from("second"),
            ]),
        );
        assert_eq!(ctx.get("items.0").unwrap().to_display_string(), "first");
        assert_eq!(ctx.get("items.1").unwrap().to_display_string(), "second");
        assert!(ctx.get("items.5").is_none());
    }

    #[test]
    fn test_context_value_display() {
        assert_eq!(format!("{}", ContextValue::Integer(42)), "42");
        assert_eq!(format!("{}", ContextValue::String("hi".into())), "hi");
        assert_eq!(format!("{}", ContextValue::None), "");
    }

    #[test]
    fn test_float_display_integer_valued() {
        let v = ContextValue::Float(3.0);
        assert_eq!(v.to_display_string(), "3.0");
    }
}
