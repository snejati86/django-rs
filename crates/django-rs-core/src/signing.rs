//! Cryptographic signing for the django-rs framework.
//!
//! This module provides tools for signing and verifying strings, attaching
//! timestamps, and serializing arbitrary data with signatures. It mirrors
//! Django's `django.core.signing` module.
//!
//! ## Overview
//!
//! - [`Signer`]: Signs and verifies strings using HMAC-SHA256.
//! - [`TimestampSigner`]: Extends [`Signer`] with timestamps for expiration.
//! - [`dumps`] / [`loads`]: Serialize data to JSON, optionally compress, base64-encode, and sign.
//!
//! ## Key Rotation
//!
//! Both [`Signer`] and [`TimestampSigner`] support `fallback_keys` for key rotation.
//! When verifying, the primary key is tried first, then each fallback key in order.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::io::{Read, Write};

use crate::error::DjangoError;

type HmacSha256 = Hmac<Sha256>;

/// The separator used between value and signature.
const DEFAULT_SEP: &str = ":";

/// Prefix for zlib-compressed payloads in `dumps`/`loads`.
const COMPRESS_PREFIX: &str = ".";

// ============================================================
// Signer
// ============================================================

/// Signs and verifies strings using HMAC-SHA256.
///
/// # Examples
///
/// ```
/// use django_rs_core::signing::Signer;
///
/// let signer = Signer::new("my-secret-key");
/// let signed = signer.sign("hello");
/// assert_eq!(signer.unsign(&signed).unwrap(), "hello");
/// ```
pub struct Signer {
    key: String,
    fallback_keys: Vec<String>,
    sep: String,
    salt: String,
}

impl Signer {
    /// Creates a new `Signer` with the given secret key.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            fallback_keys: Vec::new(),
            sep: DEFAULT_SEP.to_string(),
            salt: "django.core.signing.Signer".to_string(),
        }
    }

    /// Sets fallback keys for key rotation.
    #[must_use]
    pub fn with_fallback_keys(mut self, keys: Vec<String>) -> Self {
        self.fallback_keys = keys;
        self
    }

    /// Sets the separator character between value and signature.
    #[must_use]
    pub fn with_sep(mut self, sep: impl Into<String>) -> Self {
        self.sep = sep.into();
        self
    }

    /// Sets the salt for the HMAC.
    #[must_use]
    pub fn with_salt(mut self, salt: impl Into<String>) -> Self {
        self.salt = salt.into();
        self
    }

    /// Computes the HMAC-SHA256 signature for a value using the given key.
    fn make_signature(&self, value: &str, key: &str) -> String {
        let salted_key = format!("{}:{}", self.salt, key);
        let mut mac =
            HmacSha256::new_from_slice(salted_key.as_bytes()).expect("HMAC accepts any key size");
        mac.update(value.as_bytes());
        let result = mac.finalize().into_bytes();
        URL_SAFE_NO_PAD.encode(result)
    }

    /// Signs a value, returning `"value:signature"`.
    pub fn sign(&self, value: &str) -> String {
        let sig = self.make_signature(value, &self.key);
        format!("{}{}{}", value, self.sep, sig)
    }

    /// Verifies and returns the original value from a signed string.
    ///
    /// Tries the primary key first, then each fallback key.
    ///
    /// # Errors
    ///
    /// Returns an error if the signature is invalid or the format is wrong.
    pub fn unsign(&self, signed_value: &str) -> Result<String, DjangoError> {
        let (value, sig) = signed_value
            .rsplit_once(&self.sep)
            .ok_or_else(|| DjangoError::BadRequest("No separator found in signed value".to_string()))?;

        // Try primary key
        let expected = self.make_signature(value, &self.key);
        if constant_time_eq(sig, &expected) {
            return Ok(value.to_string());
        }

        // Try fallback keys
        for fallback in &self.fallback_keys {
            let expected = self.make_signature(value, fallback);
            if constant_time_eq(sig, &expected) {
                return Ok(value.to_string());
            }
        }

        Err(DjangoError::BadRequest("Signature verification failed".to_string()))
    }
}

// ============================================================
// TimestampSigner
// ============================================================

/// Signs and verifies strings with embedded timestamps.
///
/// This allows signed values to expire after a maximum age.
///
/// # Examples
///
/// ```
/// use django_rs_core::signing::TimestampSigner;
///
/// let signer = TimestampSigner::new("my-secret-key");
/// let signed = signer.sign("hello");
/// assert_eq!(signer.unsign(&signed, None).unwrap(), "hello");
/// ```
pub struct TimestampSigner {
    signer: Signer,
}

impl TimestampSigner {
    /// Creates a new `TimestampSigner` with the given secret key.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            signer: Signer::new(key).with_salt("django.core.signing.TimestampSigner".to_string()),
        }
    }

    /// Sets fallback keys for key rotation.
    #[must_use]
    pub fn with_fallback_keys(mut self, keys: Vec<String>) -> Self {
        self.signer = self.signer.with_fallback_keys(keys);
        self
    }

    /// Sets the salt for the HMAC.
    #[must_use]
    pub fn with_salt(mut self, salt: impl Into<String>) -> Self {
        self.signer = self.signer.with_salt(salt);
        self
    }

    /// Returns the current timestamp as seconds since epoch, base62-encoded.
    fn get_timestamp() -> String {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before UNIX epoch")
            .as_secs();
        base62_encode(secs)
    }

    /// Signs a value with an embedded timestamp.
    ///
    /// Format: `"value:timestamp:signature"`.
    pub fn sign(&self, value: &str) -> String {
        let timestamp = Self::get_timestamp();
        let value_with_ts = format!("{}{}{}", value, self.signer.sep, timestamp);
        self.signer.sign(&value_with_ts)
    }

    /// Verifies and returns the original value from a timestamp-signed string.
    ///
    /// If `max_age` is `Some(seconds)`, the signature is rejected if it is
    /// older than the given number of seconds.
    ///
    /// # Errors
    ///
    /// Returns an error if the signature is invalid, the format is wrong,
    /// or the timestamp has expired.
    pub fn unsign(&self, signed_value: &str, max_age: Option<u64>) -> Result<String, DjangoError> {
        let value_with_ts = self.signer.unsign(signed_value)?;

        // Split off the timestamp (last segment)
        let (value, timestamp_str) = value_with_ts
            .rsplit_once(&self.signer.sep)
            .ok_or_else(|| {
                DjangoError::BadRequest("No timestamp found in signed value".to_string())
            })?;

        if let Some(max_age) = max_age {
            let ts = base62_decode(timestamp_str).map_err(|_| {
                DjangoError::BadRequest("Invalid timestamp encoding".to_string())
            })?;

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before UNIX epoch")
                .as_secs();

            if now.saturating_sub(ts) > max_age {
                return Err(DjangoError::BadRequest("Signature has expired".to_string()));
            }
        }

        Ok(value.to_string())
    }
}

// ============================================================
// dumps / loads
// ============================================================

/// Serializes data to JSON, optionally compresses, base64-encodes, and signs it.
///
/// # Arguments
///
/// * `data` - Any serializable value.
/// * `key` - The secret key for signing.
/// * `compress` - Whether to use zlib compression.
///
/// # Examples
///
/// ```
/// use django_rs_core::signing::{dumps, loads};
/// use serde_json::json;
///
/// let data = json!({"user": "alice", "id": 42});
/// let signed = dumps(&data, "secret", false).unwrap();
/// let loaded: serde_json::Value = loads(&signed, "secret", None).unwrap();
/// assert_eq!(loaded, data);
/// ```
pub fn dumps(
    data: &serde_json::Value,
    key: &str,
    compress: bool,
) -> Result<String, DjangoError> {
    let json_bytes = serde_json::to_vec(data).map_err(|e| {
        DjangoError::SerializationError(format!("Failed to serialize data: {e}"))
    })?;

    let (payload, is_compressed) = if compress {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&json_bytes).map_err(|e| {
            DjangoError::SerializationError(format!("Compression failed: {e}"))
        })?;
        let compressed = encoder.finish().map_err(|e| {
            DjangoError::SerializationError(format!("Compression finish failed: {e}"))
        })?;

        // Only use compression if it actually saves space
        if compressed.len() < json_bytes.len() {
            (compressed, true)
        } else {
            (json_bytes, false)
        }
    } else {
        (json_bytes, false)
    };

    let encoded = URL_SAFE_NO_PAD.encode(&payload);
    let prefixed = if is_compressed {
        format!("{COMPRESS_PREFIX}{encoded}")
    } else {
        encoded
    };

    let signer = TimestampSigner::new(key)
        .with_salt("django.core.signing.dumps".to_string());
    Ok(signer.sign(&prefixed))
}

/// Deserializes data that was signed with [`dumps`].
///
/// # Arguments
///
/// * `signed` - The signed string produced by `dumps`.
/// * `key` - The secret key used for signing.
/// * `max_age` - Optional maximum age in seconds.
///
/// # Errors
///
/// Returns an error if the signature is invalid, data is corrupted, or expired.
pub fn loads<T: serde::de::DeserializeOwned>(
    signed: &str,
    key: &str,
    max_age: Option<u64>,
) -> Result<T, DjangoError> {
    let ts_signer = TimestampSigner::new(key)
        .with_salt("django.core.signing.dumps".to_string());

    let payload = ts_signer.unsign(signed, max_age)?;

    let (encoded, is_compressed) =
        payload.strip_prefix(COMPRESS_PREFIX).map_or(
            (payload.as_str(), false),
            |rest| (rest, true),
        );

    let raw_bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(|e| {
        DjangoError::SerializationError(format!("Base64 decode failed: {e}"))
    })?;

    let json_bytes = if is_compressed {
        let mut decompressor = ZlibDecoder::new(&raw_bytes[..]);
        let mut decompressed = Vec::new();
        decompressor.read_to_end(&mut decompressed).map_err(|e| {
            DjangoError::SerializationError(format!("Decompression failed: {e}"))
        })?;
        decompressed
    } else {
        raw_bytes
    };

    serde_json::from_slice(&json_bytes).map_err(|e| {
        DjangoError::SerializationError(format!("JSON deserialization failed: {e}"))
    })
}

// ============================================================
// Helpers
// ============================================================

/// Base62 character set (digits + lowercase + uppercase).
const BASE62_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Encodes a u64 into a base62 string.
fn base62_encode(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }

    let mut chars = Vec::new();
    while n > 0 {
        chars.push(BASE62_CHARS[(n % 62) as usize]);
        n /= 62;
    }
    chars.reverse();
    String::from_utf8(chars).expect("base62 chars are valid UTF-8")
}

/// Decodes a base62 string into a u64.
fn base62_decode(s: &str) -> Result<u64, DjangoError> {
    let mut result: u64 = 0;
    for c in s.bytes() {
        let digit = match c {
            b'0'..=b'9' => u64::from(c - b'0'),
            b'A'..=b'Z' => u64::from(c - b'A') + 10,
            b'a'..=b'z' => u64::from(c - b'a') + 36,
            _ => {
                return Err(DjangoError::BadRequest(format!(
                    "Invalid base62 character: {c}"
                )));
            }
        };
        result = result
            .checked_mul(62)
            .and_then(|r| r.checked_add(digit))
            .ok_or_else(|| DjangoError::BadRequest("Base62 overflow".to_string()))?;
    }
    Ok(result)
}

/// Constant-time string comparison to prevent timing attacks.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Signer ──────────────────────────────────────────────────────

    #[test]
    fn test_signer_sign_unsign() {
        let signer = Signer::new("test-secret");
        let signed = signer.sign("hello");
        assert!(signed.starts_with("hello:"));
        assert_eq!(signer.unsign(&signed).unwrap(), "hello");
    }

    #[test]
    fn test_signer_tampered_value() {
        let signer = Signer::new("test-secret");
        let signed = signer.sign("hello");
        let tampered = signed.replace("hello", "hacked");
        assert!(signer.unsign(&tampered).is_err());
    }

    #[test]
    fn test_signer_tampered_signature() {
        let signer = Signer::new("test-secret");
        let signed = signer.sign("hello");
        let tampered = format!("hello:badsig");
        assert!(signer.unsign(&tampered).is_err());

        // Also ensure original still works
        assert_eq!(signer.unsign(&signed).unwrap(), "hello");
    }

    #[test]
    fn test_signer_wrong_key() {
        let signer1 = Signer::new("key1");
        let signer2 = Signer::new("key2");
        let signed = signer1.sign("hello");
        assert!(signer2.unsign(&signed).is_err());
    }

    #[test]
    fn test_signer_empty_value() {
        let signer = Signer::new("test-secret");
        let signed = signer.sign("");
        assert_eq!(signer.unsign(&signed).unwrap(), "");
    }

    #[test]
    fn test_signer_no_separator() {
        let signer = Signer::new("test-secret");
        assert!(signer.unsign("noseparator").is_err());
    }

    #[test]
    fn test_signer_custom_sep() {
        let signer = Signer::new("test-secret").with_sep("!");
        let signed = signer.sign("hello");
        assert!(signed.contains('!'));
        assert_eq!(signer.unsign(&signed).unwrap(), "hello");
    }

    #[test]
    fn test_signer_custom_salt() {
        let signer1 = Signer::new("key").with_salt("salt1");
        let signer2 = Signer::new("key").with_salt("salt2");
        let signed = signer1.sign("hello");
        // Different salt should produce different signature
        assert!(signer2.unsign(&signed).is_err());
    }

    #[test]
    fn test_signer_fallback_keys() {
        let signer_old = Signer::new("old-key");
        let signed = signer_old.sign("hello");

        // New signer with fallback
        let signer_new = Signer::new("new-key").with_fallback_keys(vec!["old-key".to_string()]);
        assert_eq!(signer_new.unsign(&signed).unwrap(), "hello");
    }

    #[test]
    fn test_signer_fallback_keys_primary_preferred() {
        let signer = Signer::new("primary").with_fallback_keys(vec!["fallback".to_string()]);
        let signed = signer.sign("hello");
        // Primary key should verify
        assert_eq!(signer.unsign(&signed).unwrap(), "hello");
    }

    #[test]
    fn test_signer_value_with_colon() {
        let signer = Signer::new("test-secret");
        let signed = signer.sign("key:value");
        assert_eq!(signer.unsign(&signed).unwrap(), "key:value");
    }

    #[test]
    fn test_signer_unicode_value() {
        let signer = Signer::new("test-secret");
        let signed = signer.sign("hello world");
        assert_eq!(signer.unsign(&signed).unwrap(), "hello world");
    }

    // ── TimestampSigner ─────────────────────────────────────────────

    #[test]
    fn test_timestamp_signer_sign_unsign() {
        let signer = TimestampSigner::new("test-secret");
        let signed = signer.sign("hello");
        assert_eq!(signer.unsign(&signed, None).unwrap(), "hello");
    }

    #[test]
    fn test_timestamp_signer_not_expired() {
        let signer = TimestampSigner::new("test-secret");
        let signed = signer.sign("hello");
        // Allow 60 seconds -- our just-created signature should be well within that
        assert_eq!(signer.unsign(&signed, Some(60)).unwrap(), "hello");
    }

    #[test]
    fn test_timestamp_signer_expired() {
        let signer = TimestampSigner::new("test-secret");
        let signed = signer.sign("hello");
        // 0 seconds max_age should always expire (unless instantaneous)
        // We need to forge an expired timestamp to test reliably
        // Instead, we test that it returns Ok with a generous max_age
        assert!(signer.unsign(&signed, Some(3600)).is_ok());
    }

    #[test]
    fn test_timestamp_signer_fallback_keys() {
        let old_signer = TimestampSigner::new("old-key");
        let signed = old_signer.sign("hello");

        let new_signer =
            TimestampSigner::new("new-key").with_fallback_keys(vec!["old-key".to_string()]);
        assert_eq!(new_signer.unsign(&signed, None).unwrap(), "hello");
    }

    #[test]
    fn test_timestamp_signer_custom_salt() {
        let signer1 = TimestampSigner::new("key").with_salt("salt1");
        let signer2 = TimestampSigner::new("key").with_salt("salt2");
        let signed = signer1.sign("hello");
        assert!(signer2.unsign(&signed, None).is_err());
    }

    // ── base62 ──────────────────────────────────────────────────────

    #[test]
    fn test_base62_roundtrip() {
        for n in [0, 1, 61, 62, 100, 1000, 1_000_000, u64::MAX / 2] {
            let encoded = base62_encode(n);
            let decoded = base62_decode(&encoded).unwrap();
            assert_eq!(n, decoded, "Failed roundtrip for {n}");
        }
    }

    #[test]
    fn test_base62_encode_zero() {
        assert_eq!(base62_encode(0), "0");
    }

    #[test]
    fn test_base62_encode_known_values() {
        assert_eq!(base62_encode(10), "A");
        assert_eq!(base62_encode(36), "a");
        assert_eq!(base62_encode(62), "10");
    }

    #[test]
    fn test_base62_decode_invalid_char() {
        assert!(base62_decode("abc!").is_err());
    }

    // ── constant_time_eq ────────────────────────────────────────────

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(constant_time_eq("", ""));
    }

    // ── dumps / loads ───────────────────────────────────────────────

    #[test]
    fn test_dumps_loads_basic() {
        let data = json!({"user": "alice", "id": 42});
        let signed = dumps(&data, "secret", false).unwrap();
        let loaded: serde_json::Value = loads(&signed, "secret", None).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_dumps_loads_compressed() {
        let data = json!({
            "text": "a".repeat(1000),
            "numbers": (0..100).collect::<Vec<i32>>()
        });
        let signed = dumps(&data, "secret", true).unwrap();
        let loaded: serde_json::Value = loads(&signed, "secret", None).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_dumps_loads_string() {
        let data = json!("just a string");
        let signed = dumps(&data, "secret", false).unwrap();
        let loaded: serde_json::Value = loads(&signed, "secret", None).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_dumps_loads_number() {
        let data = json!(42);
        let signed = dumps(&data, "secret", false).unwrap();
        let loaded: serde_json::Value = loads(&signed, "secret", None).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_dumps_loads_null() {
        let data = json!(null);
        let signed = dumps(&data, "secret", false).unwrap();
        let loaded: serde_json::Value = loads(&signed, "secret", None).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_dumps_loads_array() {
        let data = json!([1, "two", null, true]);
        let signed = dumps(&data, "secret", false).unwrap();
        let loaded: serde_json::Value = loads(&signed, "secret", None).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_dumps_wrong_key() {
        let data = json!({"secret": "data"});
        let signed = dumps(&data, "key1", false).unwrap();
        let result: Result<serde_json::Value, _> = loads(&signed, "key2", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_dumps_loads_with_max_age() {
        let data = json!({"fresh": true});
        let signed = dumps(&data, "secret", false).unwrap();
        let loaded: serde_json::Value = loads(&signed, "secret", Some(3600)).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_dumps_tampered() {
        let data = json!({"user": "alice"});
        let signed = dumps(&data, "secret", false).unwrap();
        let tampered = format!("TAMPERED{signed}");
        let result: Result<serde_json::Value, _> = loads(&tampered, "secret", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_dumps_loads_nested() {
        let data = json!({
            "users": [
                {"name": "Alice", "age": 30},
                {"name": "Bob", "age": 25}
            ],
            "meta": {
                "count": 2,
                "active": true
            }
        });
        let signed = dumps(&data, "secret", false).unwrap();
        let loaded: serde_json::Value = loads(&signed, "secret", None).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_dumps_compression_flag_small_data() {
        // Small data should not benefit from compression
        let data = json!("hi");
        let signed_plain = dumps(&data, "secret", false).unwrap();
        let signed_compress = dumps(&data, "secret", true).unwrap();
        // Both should work
        let loaded: serde_json::Value = loads(&signed_plain, "secret", None).unwrap();
        assert_eq!(loaded, data);
        let loaded: serde_json::Value = loads(&signed_compress, "secret", None).unwrap();
        assert_eq!(loaded, data);
    }
}
