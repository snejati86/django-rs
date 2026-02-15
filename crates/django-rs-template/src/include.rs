//! Template inclusion and fragment caching.
//!
//! This module documents the `{% include %}` tag behavior. The actual
//! implementation lives in [`crate::parser`] (parsing) and [`crate::engine`]
//! (rendering).
//!
//! ## `{% include %}` Tag
//!
//! Includes another template, rendering it with the current context:
//!
//! ```text
//! {% include "header.html" %}
//! ```
//!
//! ### With Extra Context
//!
//! Pass additional variables to the included template:
//!
//! ```text
//! {% include "header.html" with title="Home" %}
//! ```
//!
//! ### Only Mode
//!
//! Include with *only* the specified variables (no parent context):
//!
//! ```text
//! {% include "header.html" with title="Home" only %}
//! ```
//!
//! ## Fragment Caching
//!
//! The [`FragmentCache`] struct provides a simple in-memory cache for
//! rendered template fragments.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// A simple in-memory cache for rendered template fragments.
///
/// Useful for caching expensive template sections that don't change often.
pub struct FragmentCache {
    entries: RwLock<HashMap<String, CacheEntry>>,
    default_timeout: Duration,
}

struct CacheEntry {
    content: String,
    expires_at: Instant,
}

impl FragmentCache {
    /// Creates a new fragment cache with the given default timeout.
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            default_timeout,
        }
    }

    /// Gets a cached fragment by key, or `None` if expired or missing.
    pub fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.read().unwrap();
        entries.get(key).and_then(|entry| {
            if Instant::now() < entry.expires_at {
                Some(entry.content.clone())
            } else {
                None
            }
        })
    }

    /// Sets a cached fragment with the default timeout.
    pub fn set(&self, key: impl Into<String>, content: impl Into<String>) {
        self.set_with_timeout(key, content, self.default_timeout);
    }

    /// Sets a cached fragment with a custom timeout.
    pub fn set_with_timeout(
        &self,
        key: impl Into<String>,
        content: impl Into<String>,
        timeout: Duration,
    ) {
        let mut entries = self.entries.write().unwrap();
        entries.insert(
            key.into(),
            CacheEntry {
                content: content.into(),
                expires_at: Instant::now() + timeout,
            },
        );
    }

    /// Removes a cached fragment.
    pub fn invalidate(&self, key: &str) {
        let mut entries = self.entries.write().unwrap();
        entries.remove(key);
    }

    /// Clears all cached fragments.
    pub fn clear(&self) {
        let mut entries = self.entries.write().unwrap();
        entries.clear();
    }
}

impl Default for FragmentCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(300))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fragment_cache_set_get() {
        let cache = FragmentCache::default();
        cache.set("key1", "content1");
        assert_eq!(cache.get("key1"), Some("content1".to_string()));
    }

    #[test]
    fn test_fragment_cache_missing_key() {
        let cache = FragmentCache::default();
        assert_eq!(cache.get("missing"), None);
    }

    #[test]
    fn test_fragment_cache_invalidate() {
        let cache = FragmentCache::default();
        cache.set("key1", "content1");
        cache.invalidate("key1");
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_fragment_cache_clear() {
        let cache = FragmentCache::default();
        cache.set("key1", "content1");
        cache.set("key2", "content2");
        cache.clear();
        assert_eq!(cache.get("key1"), None);
        assert_eq!(cache.get("key2"), None);
    }

    #[test]
    fn test_fragment_cache_expiry() {
        let cache = FragmentCache::new(Duration::from_millis(1));
        cache.set("key1", "content1");
        // Sleep to let it expire
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_fragment_cache_custom_timeout() {
        let cache = FragmentCache::default();
        cache.set_with_timeout("key1", "content1", Duration::from_secs(3600));
        assert_eq!(cache.get("key1"), Some("content1".to_string()));
    }

    #[test]
    fn test_fragment_cache_overwrite() {
        let cache = FragmentCache::default();
        cache.set("key1", "v1");
        cache.set("key1", "v2");
        assert_eq!(cache.get("key1"), Some("v2".to_string()));
    }
}
