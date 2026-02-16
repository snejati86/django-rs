//! Caching framework for django-rs.
//!
//! This module provides the [`CacheBackend`] trait and several built-in implementations
//! for server-side caching. It mirrors Django's `django.core.cache` module.
//!
//! ## Backends
//!
//! - [`InMemoryCache`] - Thread-safe in-memory cache with TTL support
//! - [`DatabaseCache`] - Cache stored in a database table (async)
//! - [`FileCache`] - Filesystem-based cache with async I/O
//! - [`DummyCache`] - No-op cache for testing
//!
//! ## Usage
//!
//! ```rust,no_run
//! use django_rs_cli::cache::{CacheBackend, CacheValue, InMemoryCache};
//! use std::time::Duration;
//!
//! async fn example() {
//!     let cache = InMemoryCache::new();
//!     cache.set("key", CacheValue::String("hello".to_string()), None).await.unwrap();
//!     let val = cache.get("key").await.unwrap();
//!     assert!(val.is_some());
//! }
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use django_rs_core::DjangoError;

/// A value that can be stored in a cache backend.
///
/// Supports common types: strings, integers, floats, raw bytes, and JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CacheValue {
    /// A string value.
    String(String),
    /// A 64-bit integer value.
    Integer(i64),
    /// A 64-bit floating-point value.
    Float(f64),
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// A JSON value.
    Json(serde_json::Value),
}

impl CacheValue {
    /// Returns the value as a string, if it is a `String` variant.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the value as an i64, if it is an `Integer` variant.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the value as an f64, if it is a `Float` variant.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }
}

/// A backend for storing and retrieving cached values.
///
/// All methods are async and the trait requires `Send + Sync` to support
/// concurrent access from multiple tokio tasks. This mirrors Django's
/// cache backend interface.
#[async_trait]
pub trait CacheBackend: Send + Sync {
    /// Retrieves a value from the cache by key.
    ///
    /// Returns `None` if the key does not exist or has expired.
    async fn get(&self, key: &str) -> Result<Option<CacheValue>, DjangoError>;

    /// Stores a value in the cache with an optional TTL.
    ///
    /// If `ttl` is `None`, the value does not expire (or uses the backend default).
    async fn set(
        &self,
        key: &str,
        value: CacheValue,
        ttl: Option<Duration>,
    ) -> Result<(), DjangoError>;

    /// Deletes a value from the cache.
    ///
    /// Returns `true` if the key existed and was deleted.
    async fn delete(&self, key: &str) -> Result<bool, DjangoError>;

    /// Removes all entries from the cache.
    async fn clear(&self) -> Result<(), DjangoError>;

    /// Retrieves multiple values from the cache at once.
    ///
    /// Returns a map of key-value pairs for keys that exist and have not expired.
    async fn get_many(&self, keys: &[&str]) -> Result<HashMap<String, CacheValue>, DjangoError>;

    /// Stores multiple values in the cache at once.
    async fn set_many(
        &self,
        values: &HashMap<String, CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), DjangoError>;

    /// Checks whether a key exists in the cache.
    async fn has_key(&self, key: &str) -> Result<bool, DjangoError>;

    /// Increments an integer value by `delta`.
    ///
    /// Returns the new value after incrementing. If the key does not exist
    /// or is not an integer, returns an error.
    async fn incr(&self, key: &str, delta: i64) -> Result<i64, DjangoError>;
}

/// An entry in the in-memory cache, wrapping a value with its expiration time.
#[derive(Debug, Clone)]
struct CacheEntry {
    value: CacheValue,
    expires_at: Option<Instant>,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| Instant::now() > exp)
    }
}

/// A thread-safe in-memory cache backend with TTL support.
///
/// Uses `RwLock<HashMap>` for concurrent read access. Expired entries
/// are lazily cleaned up during read operations. This is suitable for
/// single-process deployments and testing.
#[derive(Debug, Clone)]
pub struct InMemoryCache {
    store: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryCache {
    /// Creates a new empty in-memory cache.
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl CacheBackend for InMemoryCache {
    async fn get(&self, key: &str) -> Result<Option<CacheValue>, DjangoError> {
        let store = self.store.read().await;
        match store.get(key) {
            Some(entry) if !entry.is_expired() => Ok(Some(entry.value.clone())),
            Some(_) => {
                // Entry is expired; clean up lazily on next write
                Ok(None)
            }
            None => Ok(None),
        }
    }

    async fn set(
        &self,
        key: &str,
        value: CacheValue,
        ttl: Option<Duration>,
    ) -> Result<(), DjangoError> {
        let mut store = self.store.write().await;
        let expires_at = ttl.map(|d| Instant::now() + d);
        store.insert(key.to_string(), CacheEntry { value, expires_at });
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool, DjangoError> {
        let mut store = self.store.write().await;
        Ok(store.remove(key).is_some())
    }

    async fn clear(&self) -> Result<(), DjangoError> {
        let mut store = self.store.write().await;
        store.clear();
        Ok(())
    }

    async fn get_many(&self, keys: &[&str]) -> Result<HashMap<String, CacheValue>, DjangoError> {
        let store = self.store.read().await;
        let mut result = HashMap::new();

        for &key in keys {
            if let Some(entry) = store.get(key) {
                if !entry.is_expired() {
                    result.insert(key.to_string(), entry.value.clone());
                }
            }
        }

        Ok(result)
    }

    async fn set_many(
        &self,
        values: &HashMap<String, CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), DjangoError> {
        let mut store = self.store.write().await;
        let expires_at = ttl.map(|d| Instant::now() + d);

        for (key, value) in values {
            store.insert(
                key.clone(),
                CacheEntry {
                    value: value.clone(),
                    expires_at,
                },
            );
        }

        Ok(())
    }

    async fn has_key(&self, key: &str) -> Result<bool, DjangoError> {
        let store = self.store.read().await;
        Ok(store.get(key).is_some_and(|entry| !entry.is_expired()))
    }

    async fn incr(&self, key: &str, delta: i64) -> Result<i64, DjangoError> {
        let mut store = self.store.write().await;
        let entry = store
            .get_mut(key)
            .ok_or_else(|| DjangoError::NotFound(format!("Cache key '{key}' does not exist")))?;

        if entry.is_expired() {
            return Err(DjangoError::NotFound(format!(
                "Cache key '{key}' has expired"
            )));
        }

        match &entry.value {
            CacheValue::Integer(current) => {
                let new_value = current + delta;
                entry.value = CacheValue::Integer(new_value);
                Ok(new_value)
            }
            _ => Err(DjangoError::BadRequest(format!(
                "Cache key '{key}' is not an integer"
            ))),
        }
    }
}

/// An async database-backed cache.
///
/// Stores cache entries in a database table. In a full implementation,
/// this would use the django-rs-db connection pool. Currently a placeholder
/// backed by in-memory storage.
#[derive(Debug, Clone)]
pub struct DatabaseCache {
    /// The database table name for cache entries.
    pub table_name: String,
    inner: InMemoryCache,
}

impl DatabaseCache {
    /// Creates a new database cache with the given table name.
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            inner: InMemoryCache::new(),
        }
    }
}

#[async_trait]
impl CacheBackend for DatabaseCache {
    async fn get(&self, key: &str) -> Result<Option<CacheValue>, DjangoError> {
        self.inner.get(key).await
    }

    async fn set(
        &self,
        key: &str,
        value: CacheValue,
        ttl: Option<Duration>,
    ) -> Result<(), DjangoError> {
        self.inner.set(key, value, ttl).await
    }

    async fn delete(&self, key: &str) -> Result<bool, DjangoError> {
        self.inner.delete(key).await
    }

    async fn clear(&self) -> Result<(), DjangoError> {
        self.inner.clear().await
    }

    async fn get_many(&self, keys: &[&str]) -> Result<HashMap<String, CacheValue>, DjangoError> {
        self.inner.get_many(keys).await
    }

    async fn set_many(
        &self,
        values: &HashMap<String, CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), DjangoError> {
        self.inner.set_many(values, ttl).await
    }

    async fn has_key(&self, key: &str) -> Result<bool, DjangoError> {
        self.inner.has_key(key).await
    }

    async fn incr(&self, key: &str, delta: i64) -> Result<i64, DjangoError> {
        self.inner.incr(key, delta).await
    }
}

/// A filesystem-based cache backend using async I/O.
///
/// Stores each cache entry as a file in a directory. Suitable for
/// shared cache between processes on the same machine.
#[derive(Debug, Clone)]
pub struct FileCache {
    /// The directory where cache files are stored.
    pub dir: PathBuf,
}

impl FileCache {
    /// Creates a new file cache that stores entries in the given directory.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Returns the filesystem path for a given cache key.
    fn key_path(&self, key: &str) -> PathBuf {
        // Simple hash-based filename to avoid filesystem issues
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        self.dir.join(format!("{hash:016x}.cache"))
    }
}

/// Serialized representation of a file cache entry.
#[derive(Serialize, Deserialize)]
struct FileCacheEntry {
    value: CacheValue,
    expires_at_ms: Option<u128>,
}

#[async_trait]
impl CacheBackend for FileCache {
    async fn get(&self, key: &str) -> Result<Option<CacheValue>, DjangoError> {
        let path = self.key_path(key);
        match tokio::fs::read(&path).await {
            Ok(data) => {
                let entry: FileCacheEntry = serde_json::from_slice(&data)
                    .map_err(|e| DjangoError::SerializationError(e.to_string()))?;

                if let Some(expires_at_ms) = entry.expires_at_ms {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    if now_ms > expires_at_ms {
                        // Expired: delete the file
                        let _ = tokio::fs::remove_file(&path).await;
                        return Ok(None);
                    }
                }

                Ok(Some(entry.value))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(DjangoError::IoError(e)),
        }
    }

    async fn set(
        &self,
        key: &str,
        value: CacheValue,
        ttl: Option<Duration>,
    ) -> Result<(), DjangoError> {
        tokio::fs::create_dir_all(&self.dir).await?;

        let expires_at_ms = ttl.map(|d| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                + d.as_millis()
        });

        let entry = FileCacheEntry {
            value,
            expires_at_ms,
        };

        let data = serde_json::to_vec(&entry)
            .map_err(|e| DjangoError::SerializationError(e.to_string()))?;

        let path = self.key_path(key);
        tokio::fs::write(&path, &data).await?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool, DjangoError> {
        let path = self.key_path(key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(DjangoError::IoError(e)),
        }
    }

    async fn clear(&self) -> Result<(), DjangoError> {
        if self.dir.exists() {
            let mut entries = tokio::fs::read_dir(&self.dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "cache") {
                    let _ = tokio::fs::remove_file(&path).await;
                }
            }
        }
        Ok(())
    }

    async fn get_many(&self, keys: &[&str]) -> Result<HashMap<String, CacheValue>, DjangoError> {
        let mut result = HashMap::new();
        for &key in keys {
            if let Some(value) = self.get(key).await? {
                result.insert(key.to_string(), value);
            }
        }
        Ok(result)
    }

    async fn set_many(
        &self,
        values: &HashMap<String, CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), DjangoError> {
        for (key, value) in values {
            self.set(key, value.clone(), ttl).await?;
        }
        Ok(())
    }

    async fn has_key(&self, key: &str) -> Result<bool, DjangoError> {
        Ok(self.get(key).await?.is_some())
    }

    async fn incr(&self, key: &str, delta: i64) -> Result<i64, DjangoError> {
        let value = self
            .get(key)
            .await?
            .ok_or_else(|| DjangoError::NotFound(format!("Cache key '{key}' does not exist")))?;

        match value {
            CacheValue::Integer(current) => {
                let new_value = current + delta;
                self.set(key, CacheValue::Integer(new_value), None).await?;
                Ok(new_value)
            }
            _ => Err(DjangoError::BadRequest(format!(
                "Cache key '{key}' is not an integer"
            ))),
        }
    }
}

/// A no-op cache backend that never stores anything.
///
/// Useful for disabling caching in tests or development. All operations
/// succeed but no data is persisted. This mirrors Django's `DummyCache`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DummyCache;

#[async_trait]
impl CacheBackend for DummyCache {
    async fn get(&self, _key: &str) -> Result<Option<CacheValue>, DjangoError> {
        Ok(None)
    }

    async fn set(
        &self,
        _key: &str,
        _value: CacheValue,
        _ttl: Option<Duration>,
    ) -> Result<(), DjangoError> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<bool, DjangoError> {
        Ok(false)
    }

    async fn clear(&self) -> Result<(), DjangoError> {
        Ok(())
    }

    async fn get_many(&self, _keys: &[&str]) -> Result<HashMap<String, CacheValue>, DjangoError> {
        Ok(HashMap::new())
    }

    async fn set_many(
        &self,
        _values: &HashMap<String, CacheValue>,
        _ttl: Option<Duration>,
    ) -> Result<(), DjangoError> {
        Ok(())
    }

    async fn has_key(&self, _key: &str) -> Result<bool, DjangoError> {
        Ok(false)
    }

    async fn incr(&self, key: &str, _delta: i64) -> Result<i64, DjangoError> {
        Err(DjangoError::NotFound(format!(
            "Cache key '{key}' does not exist (DummyCache)"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── CacheValue tests ──────────────────────────────────────────────

    #[test]
    fn test_cache_value_string() {
        let val = CacheValue::String("hello".to_string());
        assert_eq!(val.as_str(), Some("hello"));
        assert_eq!(val.as_integer(), None);
        assert_eq!(val.as_float(), None);
    }

    #[test]
    fn test_cache_value_integer() {
        let val = CacheValue::Integer(42);
        assert_eq!(val.as_integer(), Some(42));
        assert_eq!(val.as_str(), None);
        assert_eq!(val.as_float(), None);
    }

    #[test]
    fn test_cache_value_float() {
        let val = CacheValue::Float(1.5);
        assert_eq!(val.as_float(), Some(1.5));
        assert_eq!(val.as_str(), None);
        assert_eq!(val.as_integer(), None);
    }

    #[test]
    fn test_cache_value_equality() {
        assert_eq!(
            CacheValue::String("a".to_string()),
            CacheValue::String("a".to_string())
        );
        assert_ne!(CacheValue::Integer(1), CacheValue::Integer(2));
    }

    // ── InMemoryCache tests ───────────────────────────────────────────

    #[tokio::test]
    async fn test_inmemory_get_set() {
        let cache = InMemoryCache::new();
        cache
            .set("key", CacheValue::String("value".to_string()), None)
            .await
            .unwrap();

        let result = cache.get("key").await.unwrap();
        assert_eq!(result, Some(CacheValue::String("value".to_string())));
    }

    #[tokio::test]
    async fn test_inmemory_get_missing() {
        let cache = InMemoryCache::new();
        let result = cache.get("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_inmemory_delete() {
        let cache = InMemoryCache::new();
        cache
            .set("key", CacheValue::Integer(42), None)
            .await
            .unwrap();

        let deleted = cache.delete("key").await.unwrap();
        assert!(deleted);

        let result = cache.get("key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_inmemory_delete_nonexistent() {
        let cache = InMemoryCache::new();
        let deleted = cache.delete("nonexistent").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_inmemory_ttl_expired() {
        let cache = InMemoryCache::new();
        cache
            .set(
                "key",
                CacheValue::String("temporary".to_string()),
                Some(Duration::from_millis(1)),
            )
            .await
            .unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(10)).await;

        let result = cache.get("key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_inmemory_ttl_not_expired() {
        let cache = InMemoryCache::new();
        cache
            .set(
                "key",
                CacheValue::String("persistent".to_string()),
                Some(Duration::from_secs(60)),
            )
            .await
            .unwrap();

        let result = cache.get("key").await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_inmemory_clear() {
        let cache = InMemoryCache::new();
        cache.set("a", CacheValue::Integer(1), None).await.unwrap();
        cache.set("b", CacheValue::Integer(2), None).await.unwrap();

        cache.clear().await.unwrap();

        assert!(cache.get("a").await.unwrap().is_none());
        assert!(cache.get("b").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_inmemory_get_many() {
        let cache = InMemoryCache::new();
        cache.set("a", CacheValue::Integer(1), None).await.unwrap();
        cache.set("b", CacheValue::Integer(2), None).await.unwrap();

        let result = cache.get_many(&["a", "b", "c"]).await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("a"), Some(&CacheValue::Integer(1)));
        assert_eq!(result.get("b"), Some(&CacheValue::Integer(2)));
        assert!(!result.contains_key("c"));
    }

    #[tokio::test]
    async fn test_inmemory_set_many() {
        let cache = InMemoryCache::new();
        let mut values = HashMap::new();
        values.insert("x".to_string(), CacheValue::String("X".to_string()));
        values.insert("y".to_string(), CacheValue::String("Y".to_string()));

        cache.set_many(&values, None).await.unwrap();

        assert_eq!(
            cache.get("x").await.unwrap(),
            Some(CacheValue::String("X".to_string()))
        );
        assert_eq!(
            cache.get("y").await.unwrap(),
            Some(CacheValue::String("Y".to_string()))
        );
    }

    #[tokio::test]
    async fn test_inmemory_has_key() {
        let cache = InMemoryCache::new();
        assert!(!cache.has_key("key").await.unwrap());

        cache
            .set("key", CacheValue::Integer(1), None)
            .await
            .unwrap();
        assert!(cache.has_key("key").await.unwrap());
    }

    #[tokio::test]
    async fn test_inmemory_has_key_expired() {
        let cache = InMemoryCache::new();
        cache
            .set(
                "key",
                CacheValue::Integer(1),
                Some(Duration::from_millis(1)),
            )
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!cache.has_key("key").await.unwrap());
    }

    #[tokio::test]
    async fn test_inmemory_incr() {
        let cache = InMemoryCache::new();
        cache
            .set("counter", CacheValue::Integer(10), None)
            .await
            .unwrap();

        let new_val = cache.incr("counter", 5).await.unwrap();
        assert_eq!(new_val, 15);

        let new_val = cache.incr("counter", -3).await.unwrap();
        assert_eq!(new_val, 12);
    }

    #[tokio::test]
    async fn test_inmemory_incr_nonexistent() {
        let cache = InMemoryCache::new();
        let result = cache.incr("nonexistent", 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_inmemory_incr_non_integer() {
        let cache = InMemoryCache::new();
        cache
            .set("key", CacheValue::String("not a number".to_string()), None)
            .await
            .unwrap();

        let result = cache.incr("key", 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_inmemory_overwrite() {
        let cache = InMemoryCache::new();
        cache
            .set("key", CacheValue::Integer(1), None)
            .await
            .unwrap();
        cache
            .set("key", CacheValue::Integer(2), None)
            .await
            .unwrap();

        assert_eq!(
            cache.get("key").await.unwrap(),
            Some(CacheValue::Integer(2))
        );
    }

    // ── DatabaseCache tests ───────────────────────────────────────────

    #[tokio::test]
    async fn test_database_cache_basic() {
        let cache = DatabaseCache::new("cache_table");
        assert_eq!(cache.table_name, "cache_table");

        cache
            .set("key", CacheValue::String("db-value".to_string()), None)
            .await
            .unwrap();
        let result = cache.get("key").await.unwrap();
        assert_eq!(result, Some(CacheValue::String("db-value".to_string())));
    }

    #[tokio::test]
    async fn test_database_cache_delete() {
        let cache = DatabaseCache::new("cache_table");
        cache
            .set("key", CacheValue::Integer(100), None)
            .await
            .unwrap();
        assert!(cache.delete("key").await.unwrap());
        assert!(cache.get("key").await.unwrap().is_none());
    }

    // ── FileCache tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_file_cache_get_set() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FileCache::new(dir.path().to_path_buf());

        cache
            .set(
                "file-key",
                CacheValue::String("file-value".to_string()),
                None,
            )
            .await
            .unwrap();

        let result = cache.get("file-key").await.unwrap();
        assert_eq!(result, Some(CacheValue::String("file-value".to_string())));
    }

    #[tokio::test]
    async fn test_file_cache_get_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FileCache::new(dir.path().to_path_buf());

        let result = cache.get("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_file_cache_delete() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FileCache::new(dir.path().to_path_buf());

        cache
            .set("key", CacheValue::Integer(42), None)
            .await
            .unwrap();
        assert!(cache.delete("key").await.unwrap());
        assert!(cache.get("key").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_file_cache_delete_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FileCache::new(dir.path().to_path_buf());
        assert!(!cache.delete("nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_file_cache_clear() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FileCache::new(dir.path().to_path_buf());

        cache.set("a", CacheValue::Integer(1), None).await.unwrap();
        cache.set("b", CacheValue::Integer(2), None).await.unwrap();

        cache.clear().await.unwrap();

        assert!(cache.get("a").await.unwrap().is_none());
        assert!(cache.get("b").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_file_cache_has_key() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FileCache::new(dir.path().to_path_buf());

        assert!(!cache.has_key("key").await.unwrap());

        cache
            .set("key", CacheValue::Integer(1), None)
            .await
            .unwrap();
        assert!(cache.has_key("key").await.unwrap());
    }

    #[tokio::test]
    async fn test_file_cache_incr() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FileCache::new(dir.path().to_path_buf());

        cache
            .set("counter", CacheValue::Integer(10), None)
            .await
            .unwrap();
        let new_val = cache.incr("counter", 5).await.unwrap();
        assert_eq!(new_val, 15);
    }

    #[tokio::test]
    async fn test_file_cache_ttl_expired() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FileCache::new(dir.path().to_path_buf());

        cache
            .set(
                "key",
                CacheValue::String("temp".to_string()),
                Some(Duration::from_millis(1)),
            )
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;

        let result = cache.get("key").await.unwrap();
        assert!(result.is_none());
    }

    // ── DummyCache tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_dummy_cache_get_returns_none() {
        let cache = DummyCache;
        assert!(cache.get("any").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_dummy_cache_set_succeeds() {
        let cache = DummyCache;
        cache
            .set("key", CacheValue::String("val".to_string()), None)
            .await
            .unwrap();
        // Value is not stored
        assert!(cache.get("key").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_dummy_cache_delete_returns_false() {
        let cache = DummyCache;
        assert!(!cache.delete("any").await.unwrap());
    }

    #[tokio::test]
    async fn test_dummy_cache_has_key_false() {
        let cache = DummyCache;
        assert!(!cache.has_key("any").await.unwrap());
    }

    #[tokio::test]
    async fn test_dummy_cache_incr_fails() {
        let cache = DummyCache;
        assert!(cache.incr("any", 1).await.is_err());
    }

    #[tokio::test]
    async fn test_dummy_cache_get_many_empty() {
        let cache = DummyCache;
        let result = cache.get_many(&["a", "b"]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_dummy_cache_clear() {
        let cache = DummyCache;
        cache.clear().await.unwrap();
    }

    // ── Default trait tests ───────────────────────────────────────────

    #[tokio::test]
    async fn test_inmemory_cache_default() {
        let cache = InMemoryCache::default();
        assert!(cache.get("key").await.unwrap().is_none());
    }
}
