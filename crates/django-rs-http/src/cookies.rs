//! Cookie handling for django-rs HTTP layer.
//!
//! Provides cookie parsing, creation, and signed cookie support using HMAC.
//! This mirrors Django's cookie handling in `django.http.request` and
//! `django.http.response`.

use std::collections::HashMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as Base64Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Errors that can occur during cookie operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieError {
    /// The cookie was not found in the request.
    NotFound,
    /// The cookie signature is invalid (tampered or corrupted).
    InvalidSignature,
    /// The cookie has expired (`max_age` exceeded).
    Expired,
    /// The cookie value could not be decoded.
    DecodingError(String),
}

impl fmt::Display for CookieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "Cookie not found"),
            Self::InvalidSignature => write!(f, "Invalid cookie signature"),
            Self::Expired => write!(f, "Cookie has expired"),
            Self::DecodingError(msg) => write!(f, "Cookie decoding error: {msg}"),
        }
    }
}

impl std::error::Error for CookieError {}

/// The `SameSite` attribute for cookies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SameSite {
    /// Cookies are sent with all requests.
    Strict,
    /// Cookies are sent with top-level navigations.
    Lax,
    /// Cookies are sent with all requests (requires Secure).
    None,
}

impl fmt::Display for SameSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Strict => write!(f, "Strict"),
            Self::Lax => write!(f, "Lax"),
            Self::None => write!(f, "None"),
        }
    }
}

/// A cookie to be set on an HTTP response.
#[derive(Debug, Clone)]
pub struct Cookie {
    /// The cookie name.
    pub name: String,
    /// The cookie value.
    pub value: String,
    /// Maximum age in seconds. `None` means session cookie.
    pub max_age: Option<u64>,
    /// Expiration date string (HTTP date format).
    pub expires: Option<String>,
    /// The path for which the cookie is valid.
    pub path: String,
    /// The domain for which the cookie is valid.
    pub domain: Option<String>,
    /// Whether the cookie should only be sent over HTTPS.
    pub secure: bool,
    /// Whether the cookie is inaccessible to JavaScript.
    pub httponly: bool,
    /// The `SameSite` attribute.
    pub samesite: Option<SameSite>,
}

impl Cookie {
    /// Creates a new cookie with the given name and value, and sensible defaults.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            max_age: None,
            expires: None,
            path: "/".to_string(),
            domain: None,
            secure: false,
            httponly: false,
            samesite: None,
        }
    }

    /// Sets the max age.
    #[must_use]
    pub const fn max_age(mut self, max_age: u64) -> Self {
        self.max_age = Some(max_age);
        self
    }

    /// Sets the expires date string.
    #[must_use]
    pub fn expires(mut self, expires: impl Into<String>) -> Self {
        self.expires = Some(expires.into());
        self
    }

    /// Sets the path.
    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Sets the domain.
    #[must_use]
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Sets the secure flag.
    #[must_use]
    pub const fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Sets the httponly flag.
    #[must_use]
    pub const fn httponly(mut self, httponly: bool) -> Self {
        self.httponly = httponly;
        self
    }

    /// Sets the `SameSite` attribute.
    #[must_use]
    pub const fn samesite(mut self, samesite: SameSite) -> Self {
        self.samesite = Some(samesite);
        self
    }

    /// Formats this cookie as a `Set-Cookie` header value.
    pub fn to_set_cookie_header(&self) -> String {
        let mut parts = vec![format!("{}={}", self.name, self.value)];

        if let Some(max_age) = self.max_age {
            parts.push(format!("Max-Age={max_age}"));
        }

        if let Some(ref expires) = self.expires {
            parts.push(format!("Expires={expires}"));
        }

        parts.push(format!("Path={}", self.path));

        if let Some(ref domain) = self.domain {
            parts.push(format!("Domain={domain}"));
        }

        if self.secure {
            parts.push("Secure".to_string());
        }

        if self.httponly {
            parts.push("HttpOnly".to_string());
        }

        if let Some(ref samesite) = self.samesite {
            parts.push(format!("SameSite={samesite}"));
        }

        parts.join("; ")
    }
}

/// Parses a `Cookie` header value into a map of name-value pairs.
///
/// The Cookie header format is: `name1=value1; name2=value2`
/// Handles malformed cookies gracefully by skipping invalid entries.
pub fn parse_cookie_header(header: &str) -> HashMap<String, String> {
    let mut cookies = HashMap::new();

    for part in header.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((name, value)) = trimmed.split_once('=') {
            let name = name.trim();
            let value = value.trim();
            if !name.is_empty() {
                cookies.insert(name.to_string(), value.to_string());
            }
        }
        // Skip malformed entries (no '=' sign) silently
    }

    cookies
}

/// Signs a cookie value using HMAC-SHA256.
///
/// The signed value format is: `value:timestamp:signature`
/// where signature is `HMAC-SHA256(salt + secret_key, value:timestamp)`.
pub fn sign_cookie_value(value: &str, secret_key: &str, salt: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let payload = format!("{value}:{timestamp}");
    let signing_key = format!("{salt}{secret_key}");

    let mut mac = HmacSha256::new_from_slice(signing_key.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    format!("{value}:{timestamp}:{signature}")
}

/// Verifies and extracts a signed cookie value.
///
/// Returns the original value if the signature is valid and the cookie
/// has not expired (if `max_age` is provided).
pub fn verify_signed_cookie(
    signed_value: &str,
    secret_key: &str,
    salt: &str,
    max_age: Option<u64>,
) -> Result<String, CookieError> {
    // Split into value:timestamp:signature
    let parts: Vec<&str> = signed_value.rsplitn(3, ':').collect();
    if parts.len() != 3 {
        return Err(CookieError::InvalidSignature);
    }

    // rsplitn returns in reverse order: [signature, timestamp, value]
    let signature = parts[0];
    let timestamp_str = parts[1];
    let value = parts[2];

    let timestamp: u64 = timestamp_str
        .parse()
        .map_err(|_| CookieError::InvalidSignature)?;

    // Check expiry
    if let Some(max_age) = max_age {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now - timestamp > max_age {
            return Err(CookieError::Expired);
        }
    }

    // Verify signature
    let payload = format!("{value}:{timestamp_str}");
    let signing_key = format!("{salt}{secret_key}");

    let mut mac = HmacSha256::new_from_slice(signing_key.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());

    let expected_signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    if signature == expected_signature {
        Ok(value.to_string())
    } else {
        Err(CookieError::InvalidSignature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Cookie parsing tests ─────────────────────────────────────────

    #[test]
    fn test_parse_simple_cookies() {
        let cookies = parse_cookie_header("name=value");
        assert_eq!(cookies.get("name"), Some(&"value".to_string()));
    }

    #[test]
    fn test_parse_multiple_cookies() {
        let cookies = parse_cookie_header("name1=value1; name2=value2; name3=value3");
        assert_eq!(cookies.len(), 3);
        assert_eq!(cookies.get("name1"), Some(&"value1".to_string()));
        assert_eq!(cookies.get("name2"), Some(&"value2".to_string()));
        assert_eq!(cookies.get("name3"), Some(&"value3".to_string()));
    }

    #[test]
    fn test_parse_cookies_with_spaces() {
        let cookies = parse_cookie_header("  name1 = value1 ;  name2 = value2 ");
        assert_eq!(cookies.get("name1"), Some(&"value1".to_string()));
        assert_eq!(cookies.get("name2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_parse_empty_cookie_header() {
        let cookies = parse_cookie_header("");
        assert!(cookies.is_empty());
    }

    #[test]
    fn test_parse_cookies_with_special_chars() {
        let cookies = parse_cookie_header("session=abc123-def_456; theme=dark%20mode");
        assert_eq!(cookies.get("session"), Some(&"abc123-def_456".to_string()));
        assert_eq!(cookies.get("theme"), Some(&"dark%20mode".to_string()));
    }

    #[test]
    fn test_parse_malformed_cookies() {
        let cookies = parse_cookie_header("valid=value; malformed; =empty_name; also_valid=yes");
        assert_eq!(cookies.get("valid"), Some(&"value".to_string()));
        assert_eq!(cookies.get("also_valid"), Some(&"yes".to_string()));
        // malformed entry without = is skipped
        assert!(!cookies.contains_key("malformed"));
        // entry with empty name is skipped
        assert!(!cookies.contains_key(""));
    }

    #[test]
    fn test_parse_cookie_with_equals_in_value() {
        let cookies = parse_cookie_header("token=abc=def=ghi");
        assert_eq!(cookies.get("token"), Some(&"abc=def=ghi".to_string()));
    }

    #[test]
    fn test_parse_cookie_empty_value() {
        let cookies = parse_cookie_header("name=");
        assert_eq!(cookies.get("name"), Some(&String::new()));
    }

    #[test]
    fn test_parse_cookie_duplicate_names() {
        // Last one wins (like HashMap behavior)
        let cookies = parse_cookie_header("name=first; name=second");
        assert_eq!(cookies.get("name"), Some(&"second".to_string()));
    }

    #[test]
    fn test_parse_cookie_semicolons_only() {
        let cookies = parse_cookie_header(";;;");
        assert!(cookies.is_empty());
    }

    // ── Cookie Set-Cookie header tests ──────────────────────────────

    #[test]
    fn test_cookie_basic_set_header() {
        let cookie = Cookie::new("name", "value");
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("name=value"));
        assert!(header.contains("Path=/"));
    }

    #[test]
    fn test_cookie_with_max_age() {
        let cookie = Cookie::new("session", "abc").max_age(3600);
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("Max-Age=3600"));
    }

    #[test]
    fn test_cookie_with_expires() {
        let cookie = Cookie::new("session", "abc")
            .expires("Thu, 01 Dec 2025 16:00:00 GMT");
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("Expires=Thu, 01 Dec 2025 16:00:00 GMT"));
    }

    #[test]
    fn test_cookie_with_domain() {
        let cookie = Cookie::new("name", "value").domain("example.com");
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("Domain=example.com"));
    }

    #[test]
    fn test_cookie_with_secure() {
        let cookie = Cookie::new("name", "value").secure(true);
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("Secure"));
    }

    #[test]
    fn test_cookie_with_httponly() {
        let cookie = Cookie::new("name", "value").httponly(true);
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("HttpOnly"));
    }

    #[test]
    fn test_cookie_with_samesite_strict() {
        let cookie = Cookie::new("name", "value").samesite(SameSite::Strict);
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("SameSite=Strict"));
    }

    #[test]
    fn test_cookie_with_samesite_lax() {
        let cookie = Cookie::new("name", "value").samesite(SameSite::Lax);
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("SameSite=Lax"));
    }

    #[test]
    fn test_cookie_with_samesite_none() {
        let cookie = Cookie::new("name", "value")
            .secure(true)
            .samesite(SameSite::None);
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("SameSite=None"));
        assert!(header.contains("Secure"));
    }

    #[test]
    fn test_cookie_full_attributes() {
        let cookie = Cookie::new("session", "token123")
            .max_age(86400)
            .path("/app")
            .domain(".example.com")
            .secure(true)
            .httponly(true)
            .samesite(SameSite::Strict);
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("session=token123"));
        assert!(header.contains("Max-Age=86400"));
        assert!(header.contains("Path=/app"));
        assert!(header.contains("Domain=.example.com"));
        assert!(header.contains("Secure"));
        assert!(header.contains("HttpOnly"));
        assert!(header.contains("SameSite=Strict"));
    }

    #[test]
    fn test_cookie_delete_attributes() {
        // A delete cookie has max_age=0
        let cookie = Cookie::new("session", "").max_age(0).path("/");
        let header = cookie.to_set_cookie_header();
        assert!(header.contains("Max-Age=0"));
        assert!(header.contains("session="));
    }

    // ── Signed cookie tests ─────────────────────────────────────────

    #[test]
    fn test_sign_and_verify_cookie() {
        let signed = sign_cookie_value("hello", "secret-key", "salt");
        let result = verify_signed_cookie(&signed, "secret-key", "salt", None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_signed_cookie_wrong_key() {
        let signed = sign_cookie_value("hello", "secret-key", "salt");
        let result = verify_signed_cookie(&signed, "wrong-key", "salt", None);
        assert_eq!(result, Err(CookieError::InvalidSignature));
    }

    #[test]
    fn test_signed_cookie_wrong_salt() {
        let signed = sign_cookie_value("hello", "secret-key", "salt");
        let result = verify_signed_cookie(&signed, "secret-key", "wrong-salt", None);
        assert_eq!(result, Err(CookieError::InvalidSignature));
    }

    #[test]
    fn test_signed_cookie_tampered_value() {
        let signed = sign_cookie_value("hello", "secret-key", "salt");
        let tampered = signed.replacen("hello", "evil", 1);
        let result = verify_signed_cookie(&tampered, "secret-key", "salt", None);
        assert_eq!(result, Err(CookieError::InvalidSignature));
    }

    #[test]
    fn test_signed_cookie_invalid_format() {
        let result = verify_signed_cookie("no-colons-here", "key", "salt", None);
        assert_eq!(result, Err(CookieError::InvalidSignature));
    }

    #[test]
    fn test_signed_cookie_expired() {
        // Manually create a signed cookie with an old timestamp
        let value = "hello";
        let old_timestamp = 1_000_000u64; // Very old
        let payload = format!("{value}:{old_timestamp}");
        let signing_key = format!("salt{}", "secret-key");

        let mut mac = HmacSha256::new_from_slice(signing_key.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

        let signed = format!("{value}:{old_timestamp}:{signature}");
        let result = verify_signed_cookie(&signed, "secret-key", "salt", Some(3600));
        assert_eq!(result, Err(CookieError::Expired));
    }

    #[test]
    fn test_signed_cookie_not_expired() {
        let signed = sign_cookie_value("hello", "secret-key", "salt");
        // Should not be expired with a large max_age
        let result = verify_signed_cookie(&signed, "secret-key", "salt", Some(86400));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_signed_cookie_value_with_colons() {
        // Value containing colons should work correctly
        let signed = sign_cookie_value("a:b:c", "secret-key", "salt");
        let result = verify_signed_cookie(&signed, "secret-key", "salt", None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "a:b:c");
    }

    #[test]
    fn test_signed_cookie_empty_value() {
        let signed = sign_cookie_value("", "secret-key", "salt");
        let result = verify_signed_cookie(&signed, "secret-key", "salt", None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    // ── SameSite display tests ──────────────────────────────────────

    #[test]
    fn test_samesite_display() {
        assert_eq!(format!("{}", SameSite::Strict), "Strict");
        assert_eq!(format!("{}", SameSite::Lax), "Lax");
        assert_eq!(format!("{}", SameSite::None), "None");
    }

    // ── CookieError display tests ───────────────────────────────────

    #[test]
    fn test_cookie_error_display() {
        assert_eq!(format!("{}", CookieError::NotFound), "Cookie not found");
        assert_eq!(
            format!("{}", CookieError::InvalidSignature),
            "Invalid cookie signature"
        );
        assert_eq!(format!("{}", CookieError::Expired), "Cookie has expired");
        assert_eq!(
            format!("{}", CookieError::DecodingError("test".into())),
            "Cookie decoding error: test"
        );
    }
}
