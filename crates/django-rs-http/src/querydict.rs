//! Query string dictionary for HTTP request parameters.
//!
//! [`QueryDict`] wraps [`MultiValueDict`](django_rs_core::utils::MultiValueDict) to provide
//! an immutable-by-default dictionary for GET and POST parameters, mirroring
//! Django's `django.http.QueryDict`.

use django_rs_core::utils::MultiValueDict;
use django_rs_core::{DjangoError, DjangoResult};

/// An immutable-by-default dictionary for query string and form data.
///
/// Like Django's `QueryDict`, this type is immutable by default. The
/// [`copy`](QueryDict::copy) method returns a mutable clone.
///
/// # Examples
///
/// ```
/// use django_rs_http::QueryDict;
///
/// let qd = QueryDict::parse("color=red&color=blue&size=large");
/// assert_eq!(qd.get("color"), Some("blue"));
/// assert_eq!(qd.get_list("color"), Some(&vec!["red".to_string(), "blue".to_string()]));
///
/// let mut mutable = qd.copy();
/// mutable.set("color", "green").unwrap();
/// assert_eq!(mutable.get("color"), Some("green"));
/// ```
#[derive(Debug, Clone)]
pub struct QueryDict {
    data: MultiValueDict<String, String>,
    mutable: bool,
    encoding: String,
}

impl Default for QueryDict {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryDict {
    /// Creates a new, empty, immutable `QueryDict`.
    pub fn new() -> Self {
        Self {
            data: MultiValueDict::new(),
            mutable: false,
            encoding: "utf-8".to_string(),
        }
    }

    /// Creates a new, empty, mutable `QueryDict`.
    pub fn new_mutable() -> Self {
        Self {
            data: MultiValueDict::new(),
            mutable: true,
            encoding: "utf-8".to_string(),
        }
    }

    /// Parses a URL query string (e.g., `"key1=val1&key2=val2"`) into an immutable `QueryDict`.
    ///
    /// Handles percent-encoding and supports multiple values per key.
    pub fn parse(query_string: &str) -> Self {
        let mut data = MultiValueDict::new();

        if !query_string.is_empty() {
            for pair in query_string.split('&') {
                if pair.is_empty() {
                    continue;
                }

                let (key, value) = pair
                    .find('=')
                    .map_or((pair, ""), |eq_pos| (&pair[..eq_pos], &pair[eq_pos + 1..]));

                let decoded_key = percent_decode(key);
                let decoded_value = percent_decode(value);
                data.append(decoded_key, decoded_value);
            }
        }

        Self {
            data,
            mutable: false,
            encoding: "utf-8".to_string(),
        }
    }

    /// Returns the last value for the given key, or `None` if not present.
    ///
    /// This mirrors Django's `QueryDict.__getitem__` / `QueryDict.get()`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.data.get(&key.to_string()).map(String::as_str)
    }

    /// Returns all values for the given key, or `None` if not present.
    ///
    /// This mirrors Django's `QueryDict.getlist()`.
    pub fn get_list(&self, key: &str) -> Option<&Vec<String>> {
        self.data.get_list(&key.to_string())
    }

    /// Sets a single value for the given key, replacing any existing values.
    ///
    /// # Errors
    ///
    /// Returns [`DjangoError::SuspiciousOperation`] if this `QueryDict` is immutable.
    pub fn set(&mut self, key: &str, value: &str) -> DjangoResult<()> {
        if !self.mutable {
            return Err(DjangoError::SuspiciousOperation(
                "This QueryDict instance is immutable".to_string(),
            ));
        }
        self.data.set(key.to_string(), value.to_string());
        Ok(())
    }

    /// Appends a value to the list for the given key.
    ///
    /// # Errors
    ///
    /// Returns [`DjangoError::SuspiciousOperation`] if this `QueryDict` is immutable.
    pub fn append(&mut self, key: &str, value: &str) -> DjangoResult<()> {
        if !self.mutable {
            return Err(DjangoError::SuspiciousOperation(
                "This QueryDict instance is immutable".to_string(),
            ));
        }
        self.data.append(key.to_string(), value.to_string());
        Ok(())
    }

    /// Returns a mutable copy of this `QueryDict`.
    ///
    /// This mirrors Django's `QueryDict.copy()`.
    #[must_use]
    pub fn copy(&self) -> Self {
        Self {
            data: self.data.clone(),
            mutable: true,
            encoding: self.encoding.clone(),
        }
    }

    /// Encodes this `QueryDict` as a URL query string.
    ///
    /// All keys and values are percent-encoded.
    pub fn urlencode(&self) -> String {
        let mut parts = Vec::new();

        for (key, values) in &self.data {
            for value in values {
                let encoded_key = percent_encode(key);
                let encoded_value = percent_encode(value);
                parts.push(format!("{encoded_key}={encoded_value}"));
            }
        }

        parts.sort();
        parts.join("&")
    }

    /// Returns `true` if this `QueryDict` is mutable.
    pub const fn is_mutable(&self) -> bool {
        self.mutable
    }

    /// Returns the encoding used for this `QueryDict`.
    pub fn encoding(&self) -> &str {
        &self.encoding
    }

    /// Returns the number of distinct keys.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the `QueryDict` contains no keys.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Returns `true` if the specified key is present.
    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(&key.to_string())
    }

    /// Returns an iterator over the keys.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data.keys()
    }

    /// Returns a reference to the underlying `MultiValueDict`.
    pub const fn data(&self) -> &MultiValueDict<String, String> {
        &self.data
    }
}

/// Decodes a percent-encoded string.
fn percent_decode(input: &str) -> String {
    // Replace + with space (form encoding), then decode percent sequences
    let plus_decoded = input.replace('+', " ");
    percent_encoding::percent_decode_str(&plus_decoded)
        .decode_utf8_lossy()
        .into_owned()
}

/// Percent-encodes a string for use in a URL query.
fn percent_encode(input: &str) -> String {
    // Encode using the query encoding set (allows some chars unencoded)
    percent_encoding::utf8_percent_encode(input, percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let qd = QueryDict::new();
        assert!(qd.is_empty());
        assert_eq!(qd.len(), 0);
    }

    #[test]
    fn test_parse_simple() {
        let qd = QueryDict::parse("key=value");
        assert_eq!(qd.get("key"), Some("value"));
        assert_eq!(qd.len(), 1);
    }

    #[test]
    fn test_parse_multiple_keys() {
        let qd = QueryDict::parse("a=1&b=2&c=3");
        assert_eq!(qd.get("a"), Some("1"));
        assert_eq!(qd.get("b"), Some("2"));
        assert_eq!(qd.get("c"), Some("3"));
        assert_eq!(qd.len(), 3);
    }

    #[test]
    fn test_parse_multiple_values() {
        let qd = QueryDict::parse("color=red&color=blue&color=green");
        // get() returns the last value
        assert_eq!(qd.get("color"), Some("green"));
        assert_eq!(
            qd.get_list("color"),
            Some(&vec![
                "red".to_string(),
                "blue".to_string(),
                "green".to_string()
            ])
        );
    }

    #[test]
    fn test_parse_empty_string() {
        let qd = QueryDict::parse("");
        assert!(qd.is_empty());
    }

    #[test]
    fn test_parse_no_value() {
        let qd = QueryDict::parse("key");
        assert_eq!(qd.get("key"), Some(""));
    }

    #[test]
    fn test_parse_empty_value() {
        let qd = QueryDict::parse("key=");
        assert_eq!(qd.get("key"), Some(""));
    }

    #[test]
    fn test_parse_percent_encoded() {
        let qd = QueryDict::parse("name=hello%20world&city=New%20York");
        assert_eq!(qd.get("name"), Some("hello world"));
        assert_eq!(qd.get("city"), Some("New York"));
    }

    #[test]
    fn test_parse_plus_as_space() {
        let qd = QueryDict::parse("name=hello+world");
        assert_eq!(qd.get("name"), Some("hello world"));
    }

    #[test]
    fn test_immutable_set_fails() {
        let mut qd = QueryDict::parse("key=value");
        assert!(!qd.is_mutable());
        assert!(qd.set("key", "new_value").is_err());
    }

    #[test]
    fn test_immutable_append_fails() {
        let mut qd = QueryDict::parse("key=value");
        assert!(qd.append("key", "extra").is_err());
    }

    #[test]
    fn test_copy_returns_mutable() {
        let qd = QueryDict::parse("key=value");
        let mut mutable = qd.copy();
        assert!(mutable.is_mutable());
        assert!(mutable.set("key", "new").is_ok());
        assert_eq!(mutable.get("key"), Some("new"));
        // Original is unchanged
        assert_eq!(qd.get("key"), Some("value"));
    }

    #[test]
    fn test_mutable_set() {
        let mut qd = QueryDict::new_mutable();
        qd.set("key", "value").unwrap();
        assert_eq!(qd.get("key"), Some("value"));
    }

    #[test]
    fn test_mutable_append() {
        let mut qd = QueryDict::new_mutable();
        qd.append("key", "a").unwrap();
        qd.append("key", "b").unwrap();
        assert_eq!(qd.get("key"), Some("b"));
        assert_eq!(
            qd.get_list("key"),
            Some(&vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn test_mutable_set_replaces() {
        let mut qd = QueryDict::new_mutable();
        qd.append("key", "a").unwrap();
        qd.append("key", "b").unwrap();
        qd.set("key", "c").unwrap();
        assert_eq!(qd.get_list("key"), Some(&vec!["c".to_string()]));
    }

    #[test]
    fn test_urlencode() {
        let qd = QueryDict::parse("a=1&b=2");
        let encoded = qd.urlencode();
        // Sorted order, so a=1&b=2
        assert!(encoded.contains("a=1"));
        assert!(encoded.contains("b=2"));
    }

    #[test]
    fn test_urlencode_special_chars() {
        let mut qd = QueryDict::new_mutable();
        qd.set("name", "hello world").unwrap();
        let encoded = qd.urlencode();
        assert!(encoded.contains("hello%20world"));
    }

    #[test]
    fn test_urlencode_multiple_values() {
        let qd = QueryDict::parse("c=1&c=2");
        let encoded = qd.urlencode();
        assert!(encoded.contains("c=1"));
        assert!(encoded.contains("c=2"));
    }

    #[test]
    fn test_contains_key() {
        let qd = QueryDict::parse("key=value");
        assert!(qd.contains_key("key"));
        assert!(!qd.contains_key("missing"));
    }

    #[test]
    fn test_get_missing_key() {
        let qd = QueryDict::new();
        assert_eq!(qd.get("missing"), None);
        assert_eq!(qd.get_list("missing"), None);
    }

    #[test]
    fn test_encoding() {
        let qd = QueryDict::new();
        assert_eq!(qd.encoding(), "utf-8");
    }

    #[test]
    fn test_default() {
        let qd = QueryDict::default();
        assert!(qd.is_empty());
        assert!(!qd.is_mutable());
    }

    #[test]
    fn test_keys() {
        let qd = QueryDict::parse("a=1&b=2&c=3");
        let mut keys: Vec<_> = qd.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_skips_empty_pairs() {
        let qd = QueryDict::parse("a=1&&b=2&");
        assert_eq!(qd.get("a"), Some("1"));
        assert_eq!(qd.get("b"), Some("2"));
        assert_eq!(qd.len(), 2);
    }

    #[test]
    fn test_data_accessor() {
        let qd = QueryDict::parse("x=1");
        let data = qd.data();
        assert_eq!(data.get(&"x".to_string()), Some(&"1".to_string()));
    }
}
