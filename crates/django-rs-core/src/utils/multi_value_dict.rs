//! A dictionary that can hold multiple values per key.
//!
//! [`MultiValueDict`] mirrors Django's `MultiValueDict`, which is used
//! extensively for query parameters and form data where a single key
//! may appear multiple times.

use std::collections::hash_map;
use std::collections::HashMap;
use std::hash::Hash;

/// A dictionary that maps keys to lists of values.
///
/// By default, [`get`](MultiValueDict::get) returns the **last** value for a key
/// (matching Django's behavior), while [`get_list`](MultiValueDict::get_list)
/// returns all values.
///
/// # Examples
///
/// ```
/// use django_rs_core::utils::MultiValueDict;
///
/// let mut d = MultiValueDict::new();
/// d.append("color".to_string(), "red");
/// d.append("color".to_string(), "blue");
///
/// assert_eq!(d.get(&"color".to_string()), Some(&"blue"));
/// assert_eq!(d.get_list(&"color".to_string()), Some(&vec!["red", "blue"]));
/// ```
#[derive(Debug, Clone)]
pub struct MultiValueDict<K: Eq + Hash, V> {
    inner: HashMap<K, Vec<V>>,
}

impl<K: Eq + Hash, V> Default for MultiValueDict<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash, V> MultiValueDict<K, V> {
    /// Creates an empty `MultiValueDict`.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Returns a reference to the **last** value associated with the key,
    /// or `None` if the key is not present.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key).and_then(|v| v.last())
    }

    /// Returns a reference to all values associated with the key,
    /// or `None` if the key is not present.
    pub fn get_list(&self, key: &K) -> Option<&Vec<V>> {
        self.inner.get(key)
    }

    /// Sets the value for a key, replacing any existing values.
    pub fn set(&mut self, key: K, value: V) {
        self.inner.insert(key, vec![value]);
    }

    /// Appends a value to the list for the given key.
    pub fn append(&mut self, key: K, value: V) {
        self.inner.entry(key).or_default().push(value);
    }

    /// Returns an iterator over the keys.
    pub fn keys(&self) -> hash_map::Keys<'_, K, Vec<V>> {
        self.inner.keys()
    }

    /// Returns an iterator over all value lists.
    pub fn values(&self) -> hash_map::Values<'_, K, Vec<V>> {
        self.inner.values()
    }

    /// Returns an iterator over (key, value-list) pairs.
    pub fn items(&self) -> hash_map::Iter<'_, K, Vec<V>> {
        self.inner.iter()
    }

    /// Returns the number of distinct keys.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the dictionary contains no keys.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns `true` if the dictionary contains the specified key.
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    /// Returns an iterator over (key, value-list) pairs.
    pub fn iter(&self) -> hash_map::Iter<'_, K, Vec<V>> {
        self.inner.iter()
    }
}

impl<K: Eq + Hash, V> IntoIterator for MultiValueDict<K, V> {
    type Item = (K, Vec<V>);
    type IntoIter = hash_map::IntoIter<K, Vec<V>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, K: Eq + Hash, V> IntoIterator for &'a MultiValueDict<K, V> {
    type Item = (&'a K, &'a Vec<V>);
    type IntoIter = hash_map::Iter<'a, K, Vec<V>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let d: MultiValueDict<String, String> = MultiValueDict::new();
        assert!(d.is_empty());
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn test_set_and_get() {
        let mut d = MultiValueDict::new();
        d.set("key", "value");
        assert_eq!(d.get(&"key"), Some(&"value"));
        assert_eq!(d.get_list(&"key"), Some(&vec!["value"]));
    }

    #[test]
    fn test_append_and_get_returns_last() {
        let mut d = MultiValueDict::new();
        d.append("color", "red");
        d.append("color", "blue");
        d.append("color", "green");

        assert_eq!(d.get(&"color"), Some(&"green"));
        assert_eq!(d.get_list(&"color"), Some(&vec!["red", "blue", "green"]));
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn test_set_replaces_existing() {
        let mut d = MultiValueDict::new();
        d.append("k", "a");
        d.append("k", "b");
        d.set("k", "c");
        assert_eq!(d.get_list(&"k"), Some(&vec!["c"]));
    }

    #[test]
    fn test_get_missing_key() {
        let d: MultiValueDict<&str, &str> = MultiValueDict::new();
        assert_eq!(d.get(&"missing"), None);
        assert_eq!(d.get_list(&"missing"), None);
    }

    #[test]
    fn test_contains_key() {
        let mut d = MultiValueDict::new();
        d.set("a", 1);
        assert!(d.contains_key(&"a"));
        assert!(!d.contains_key(&"b"));
    }

    #[test]
    fn test_keys_and_values() {
        let mut d = MultiValueDict::new();
        d.set("x", 10);
        d.set("y", 20);

        let keys: Vec<_> = d.keys().collect();
        assert_eq!(keys.len(), 2);

        let values: Vec<_> = d.values().collect();
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn test_iter() {
        let mut d = MultiValueDict::new();
        d.append("a", 1);
        d.append("a", 2);
        d.append("b", 3);

        let items: HashMap<_, _> = d.iter().map(|(k, v)| (*k, v.clone())).collect();
        assert_eq!(items.get("a"), Some(&vec![1, 2]));
        assert_eq!(items.get("b"), Some(&vec![3]));
    }

    #[test]
    fn test_default() {
        let d: MultiValueDict<String, i32> = MultiValueDict::default();
        assert!(d.is_empty());
    }
}
