//! Settings loading from configuration files.
//!
//! This module provides functions to load [`Settings`] from TOML files, JSON
//! files, and to apply environment variable overrides. It mirrors the concept
//! of Django's `settings.py` but uses configuration files instead.
//!
//! ## Loading Order
//!
//! 1. Start with default settings.
//! 2. Load from a TOML or JSON file (overriding defaults).
//! 3. Apply environment variable overrides (highest priority).
//!
//! ## Environment Variable Mapping
//!
//! Environment variables are mapped from `DJANGO_<SETTING_NAME>` format:
//!
//! | Env Var | Setting |
//! |---|---|
//! | `DJANGO_SECRET_KEY` | `secret_key` |
//! | `DJANGO_DEBUG` | `debug` |
//! | `DJANGO_ALLOWED_HOSTS` | `allowed_hosts` (comma-separated) |
//! | `DJANGO_LOG_LEVEL` | `log_level` |
//! | `DJANGO_LANGUAGE_CODE` | `language_code` |
//! | `DJANGO_TIME_ZONE` | `time_zone` |
//! | `DJANGO_STATIC_URL` | `static_url` |
//! | `DJANGO_MEDIA_URL` | `media_url` |
//!
//! ## Examples
//!
//! ```rust,no_run
//! use django_rs_core::settings_loader;
//!
//! // Load from TOML
//! let settings = settings_loader::from_toml_file("config/settings.toml").unwrap();
//!
//! // Load from JSON
//! let settings = settings_loader::from_json_file("config/settings.json").unwrap();
//!
//! // Load from TOML with environment overrides
//! let settings = settings_loader::from_toml_file_with_env("config/settings.toml").unwrap();
//! ```

use std::path::Path;

use crate::error::DjangoError;
use crate::settings::Settings;

/// Loads settings from a TOML string.
///
/// The TOML is deserialized directly into a [`Settings`] struct. Any fields
/// not present in the TOML will use the default values.
///
/// # Errors
///
/// Returns an error if the TOML is malformed or cannot be deserialized.
pub fn from_toml_str(toml_str: &str) -> Result<Settings, DjangoError> {
    // We use a two-step approach: deserialize the TOML into a serde_json::Value,
    // then merge it with the default settings. This lets us keep defaults for
    // any settings not specified in the TOML.
    let toml_value: toml::Value = toml::from_str(toml_str)
        .map_err(|e| DjangoError::ConfigurationError(format!("Failed to parse TOML: {e}")))?;

    let json_value = toml_to_json(toml_value);
    let default_json = serde_json::to_value(Settings::default()).map_err(|e| {
        DjangoError::ConfigurationError(format!("Failed to serialize default settings: {e}"))
    })?;

    let merged = merge_json(default_json, json_value);
    serde_json::from_value(merged).map_err(|e| {
        DjangoError::ConfigurationError(format!("Failed to deserialize settings from TOML: {e}"))
    })
}

/// Loads settings from a TOML file.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the TOML is malformed.
pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Settings, DjangoError> {
    let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
        DjangoError::ConfigurationError(format!(
            "Failed to read TOML file '{}': {e}",
            path.as_ref().display()
        ))
    })?;
    from_toml_str(&content)
}

/// Loads settings from a TOML file and then applies environment variable overrides.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the TOML is malformed.
pub fn from_toml_file_with_env(path: impl AsRef<Path>) -> Result<Settings, DjangoError> {
    let mut settings = from_toml_file(path)?;
    apply_env_overrides(&mut settings);
    Ok(settings)
}

/// Loads settings from a JSON string.
///
/// # Errors
///
/// Returns an error if the JSON is malformed or cannot be deserialized.
pub fn from_json_str(json_str: &str) -> Result<Settings, DjangoError> {
    let json_value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| DjangoError::ConfigurationError(format!("Failed to parse JSON: {e}")))?;

    let default_json = serde_json::to_value(Settings::default()).map_err(|e| {
        DjangoError::ConfigurationError(format!("Failed to serialize default settings: {e}"))
    })?;

    let merged = merge_json(default_json, json_value);
    serde_json::from_value(merged).map_err(|e| {
        DjangoError::ConfigurationError(format!("Failed to deserialize settings from JSON: {e}"))
    })
}

/// Loads settings from a JSON file.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the JSON is malformed.
pub fn from_json_file(path: impl AsRef<Path>) -> Result<Settings, DjangoError> {
    let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
        DjangoError::ConfigurationError(format!(
            "Failed to read JSON file '{}': {e}",
            path.as_ref().display()
        ))
    })?;
    from_json_str(&content)
}

/// Loads settings from a JSON file and then applies environment variable overrides.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the JSON is malformed.
pub fn from_json_file_with_env(path: impl AsRef<Path>) -> Result<Settings, DjangoError> {
    let mut settings = from_json_file(path)?;
    apply_env_overrides(&mut settings);
    Ok(settings)
}

/// Loads settings from just environment variables (starting from defaults).
pub fn from_env() -> Settings {
    let mut settings = Settings::default();
    apply_env_overrides(&mut settings);
    settings
}

/// Applies environment variable overrides to a settings struct.
///
/// Supported environment variables:
///
/// - `DJANGO_SECRET_KEY` -> `secret_key`
/// - `DJANGO_DEBUG` -> `debug` (values: "true"/"1" => true, anything else => false)
/// - `DJANGO_ALLOWED_HOSTS` -> `allowed_hosts` (comma-separated)
/// - `DJANGO_LOG_LEVEL` -> `log_level`
/// - `DJANGO_LANGUAGE_CODE` -> `language_code`
/// - `DJANGO_TIME_ZONE` -> `time_zone`
/// - `DJANGO_STATIC_URL` -> `static_url`
/// - `DJANGO_MEDIA_URL` -> `media_url`
/// - `DJANGO_EMAIL_HOST` -> `email_host`
/// - `DJANGO_EMAIL_PORT` -> `email_port`
/// - `DJANGO_CSRF_COOKIE_NAME` -> `csrf_cookie_name`
/// - `DJANGO_SESSION_COOKIE_NAME` -> `session_cookie_name`
/// - `DJANGO_ROOT_URLCONF` -> `root_urlconf`
pub fn apply_env_overrides(settings: &mut Settings) {
    if let Ok(val) = std::env::var("DJANGO_SECRET_KEY") {
        settings.secret_key = val;
    }

    if let Ok(val) = std::env::var("DJANGO_DEBUG") {
        settings.debug = matches!(val.to_lowercase().as_str(), "true" | "1" | "yes");
    }

    if let Ok(val) = std::env::var("DJANGO_ALLOWED_HOSTS") {
        settings.allowed_hosts = val
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    if let Ok(val) = std::env::var("DJANGO_LOG_LEVEL") {
        settings.log_level = val;
    }

    if let Ok(val) = std::env::var("DJANGO_LANGUAGE_CODE") {
        settings.language_code = val;
    }

    if let Ok(val) = std::env::var("DJANGO_TIME_ZONE") {
        settings.time_zone = val;
    }

    if let Ok(val) = std::env::var("DJANGO_STATIC_URL") {
        settings.static_url = val;
    }

    if let Ok(val) = std::env::var("DJANGO_MEDIA_URL") {
        settings.media_url = val;
    }

    if let Ok(val) = std::env::var("DJANGO_EMAIL_HOST") {
        settings.email_host = val;
    }

    if let Ok(val) = std::env::var("DJANGO_EMAIL_PORT") {
        if let Ok(port) = val.parse::<u16>() {
            settings.email_port = port;
        }
    }

    if let Ok(val) = std::env::var("DJANGO_CSRF_COOKIE_NAME") {
        settings.csrf_cookie_name = val;
    }

    if let Ok(val) = std::env::var("DJANGO_SESSION_COOKIE_NAME") {
        settings.session_cookie_name = val;
    }

    if let Ok(val) = std::env::var("DJANGO_ROOT_URLCONF") {
        settings.root_urlconf = val;
    }
}

// ============================================================
// Helpers
// ============================================================

/// Converts a TOML value to a `serde_json::Value`.
fn toml_to_json(value: toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s),
        toml::Value::Integer(i) => serde_json::json!(i),
        toml::Value::Float(f) => serde_json::json!(f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(b),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(toml_to_json).collect())
        }
        toml::Value::Table(table) => {
            let map: serde_json::Map<String, serde_json::Value> = table
                .into_iter()
                .map(|(k, v)| (k, toml_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
    }
}

/// Deep-merges two JSON values. The `override_val` takes precedence.
fn merge_json(base: serde_json::Value, override_val: serde_json::Value) -> serde_json::Value {
    match (base, override_val) {
        (serde_json::Value::Object(mut base_map), serde_json::Value::Object(override_map)) => {
            for (key, override_v) in override_map {
                let merged = if let Some(base_v) = base_map.remove(&key) {
                    merge_json(base_v, override_v)
                } else {
                    override_v
                };
                base_map.insert(key, merged);
            }
            serde_json::Value::Object(base_map)
        }
        (_, override_val) => override_val,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── TOML loading ────────────────────────────────────────────────

    #[test]
    fn test_from_toml_str_basic() {
        let toml = r#"
            secret_key = "my-secret-key"
            debug = false
            language_code = "fr"
        "#;

        let settings = from_toml_str(toml).unwrap();
        assert_eq!(settings.secret_key, "my-secret-key");
        assert!(!settings.debug);
        assert_eq!(settings.language_code, "fr");
        // Defaults preserved
        assert_eq!(settings.static_url, "/static/");
    }

    #[test]
    fn test_from_toml_str_allowed_hosts() {
        let toml = r#"
            allowed_hosts = ["example.com", "www.example.com"]
        "#;

        let settings = from_toml_str(toml).unwrap();
        assert_eq!(settings.allowed_hosts.len(), 2);
        assert!(settings.allowed_hosts.contains(&"example.com".to_string()));
    }

    #[test]
    fn test_from_toml_str_databases() {
        let toml = r#"
            [databases.default]
            engine = "django_rs.db.backends.postgresql"
            name = "mydb"
            user = "myuser"
            password = "mypass"
            host = "localhost"
            port = 5432
        "#;

        let settings = from_toml_str(toml).unwrap();
        let db = settings.databases.get("default").unwrap();
        assert_eq!(db.engine, "django_rs.db.backends.postgresql");
        assert_eq!(db.name, "mydb");
        assert_eq!(db.user, "myuser");
        assert_eq!(db.port, 5432);
    }

    #[test]
    fn test_from_toml_str_empty() {
        // Empty TOML should produce defaults
        let settings = from_toml_str("").unwrap();
        assert!(settings.debug);
        assert!(settings.secret_key.is_empty());
    }

    #[test]
    fn test_from_toml_str_invalid() {
        let result = from_toml_str("[[invalid toml content");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_toml_str_middleware() {
        let toml = r#"
            middleware = [
                "django_rs.middleware.security.SecurityMiddleware",
                "django_rs.middleware.common.CommonMiddleware",
            ]
        "#;

        let settings = from_toml_str(toml).unwrap();
        assert_eq!(settings.middleware.len(), 2);
    }

    #[test]
    fn test_from_toml_str_installed_apps() {
        let toml = r#"
            installed_apps = ["django_rs.auth", "django_rs.admin", "myapp"]
        "#;

        let settings = from_toml_str(toml).unwrap();
        assert_eq!(settings.installed_apps.len(), 3);
        assert!(settings.installed_apps.contains(&"myapp".to_string()));
    }

    #[test]
    fn test_from_toml_str_security() {
        let toml = r#"
            secure_ssl_redirect = true
            secure_hsts_seconds = 31536000
        "#;

        let settings = from_toml_str(toml).unwrap();
        assert!(settings.secure_ssl_redirect);
        assert_eq!(settings.secure_hsts_seconds, 31_536_000);
    }

    // ── JSON loading ────────────────────────────────────────────────

    #[test]
    fn test_from_json_str_basic() {
        let json = r#"{
            "secret_key": "json-secret",
            "debug": false,
            "log_level": "debug"
        }"#;

        let settings = from_json_str(json).unwrap();
        assert_eq!(settings.secret_key, "json-secret");
        assert!(!settings.debug);
        assert_eq!(settings.log_level, "debug");
        // Defaults preserved
        assert_eq!(settings.static_url, "/static/");
    }

    #[test]
    fn test_from_json_str_databases() {
        let json = r#"{
            "databases": {
                "default": {
                    "engine": "django_rs.db.backends.postgresql",
                    "name": "mydb",
                    "user": "admin",
                    "password": "secret",
                    "host": "db.example.com",
                    "port": 5432,
                    "options": {}
                }
            }
        }"#;

        let settings = from_json_str(json).unwrap();
        let db = settings.databases.get("default").unwrap();
        assert_eq!(db.engine, "django_rs.db.backends.postgresql");
        assert_eq!(db.host, "db.example.com");
    }

    #[test]
    fn test_from_json_str_empty_object() {
        let settings = from_json_str("{}").unwrap();
        assert!(settings.debug);
        assert!(settings.secret_key.is_empty());
    }

    #[test]
    fn test_from_json_str_invalid() {
        let result = from_json_str("{invalid json");
        assert!(result.is_err());
    }

    // ── File loading ────────────────────────────────────────────────

    #[test]
    fn test_from_toml_file() {
        let dir = std::env::temp_dir().join("django_rs_test_toml");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_settings.toml");

        let toml_content = r#"
            secret_key = "file-secret"
            debug = false
        "#;
        std::fs::write(&path, toml_content).unwrap();

        let settings = from_toml_file(&path).unwrap();
        assert_eq!(settings.secret_key, "file-secret");
        assert!(!settings.debug);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn test_from_json_file() {
        let dir = std::env::temp_dir().join("django_rs_test_json");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_settings.json");

        let json_content = r#"{"secret_key": "json-file-secret", "debug": false}"#;
        std::fs::write(&path, json_content).unwrap();

        let settings = from_json_file(&path).unwrap();
        assert_eq!(settings.secret_key, "json-file-secret");
        assert!(!settings.debug);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn test_from_toml_file_missing() {
        let result = from_toml_file("/nonexistent/path/settings.toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_json_file_missing() {
        let result = from_json_file("/nonexistent/path/settings.json");
        assert!(result.is_err());
    }

    // ── Environment variable overrides ──────────────────────────────

    #[test]
    fn test_apply_env_overrides_secret_key() {
        let mut settings = Settings::default();
        std::env::set_var("DJANGO_SECRET_KEY", "env-secret");
        apply_env_overrides(&mut settings);
        assert_eq!(settings.secret_key, "env-secret");
        std::env::remove_var("DJANGO_SECRET_KEY");
    }

    #[test]
    fn test_apply_env_overrides_debug_true() {
        let mut settings = Settings::default();
        settings.debug = false;
        std::env::set_var("DJANGO_DEBUG", "true");
        apply_env_overrides(&mut settings);
        assert!(settings.debug);
        std::env::remove_var("DJANGO_DEBUG");
    }

    #[test]
    fn test_apply_env_overrides_debug_1() {
        let mut settings = Settings::default();
        settings.debug = false;
        std::env::set_var("DJANGO_DEBUG", "1");
        apply_env_overrides(&mut settings);
        assert!(settings.debug);
        std::env::remove_var("DJANGO_DEBUG");
    }

    #[test]
    fn test_apply_env_overrides_debug_false() {
        let mut settings = Settings::default();
        std::env::set_var("DJANGO_DEBUG", "false");
        apply_env_overrides(&mut settings);
        assert!(!settings.debug);
        std::env::remove_var("DJANGO_DEBUG");
    }

    #[test]
    fn test_apply_env_overrides_allowed_hosts() {
        let mut settings = Settings::default();
        std::env::set_var("DJANGO_ALLOWED_HOSTS", "example.com, api.example.com");
        apply_env_overrides(&mut settings);
        assert_eq!(settings.allowed_hosts.len(), 2);
        assert!(settings.allowed_hosts.contains(&"example.com".to_string()));
        assert!(settings
            .allowed_hosts
            .contains(&"api.example.com".to_string()));
        std::env::remove_var("DJANGO_ALLOWED_HOSTS");
    }

    #[test]
    fn test_apply_env_overrides_log_level() {
        let mut settings = Settings::default();
        std::env::set_var("DJANGO_LOG_LEVEL", "debug");
        apply_env_overrides(&mut settings);
        assert_eq!(settings.log_level, "debug");
        std::env::remove_var("DJANGO_LOG_LEVEL");
    }

    #[test]
    fn test_apply_env_overrides_email_port() {
        let mut settings = Settings::default();
        std::env::set_var("DJANGO_EMAIL_PORT", "587");
        apply_env_overrides(&mut settings);
        assert_eq!(settings.email_port, 587);
        std::env::remove_var("DJANGO_EMAIL_PORT");
    }

    #[test]
    fn test_apply_env_overrides_invalid_port() {
        let mut settings = Settings::default();
        let original_port = settings.email_port;
        std::env::set_var("DJANGO_EMAIL_PORT", "not-a-number");
        apply_env_overrides(&mut settings);
        assert_eq!(settings.email_port, original_port); // Should not change
        std::env::remove_var("DJANGO_EMAIL_PORT");
    }

    #[test]
    fn test_from_env() {
        std::env::set_var("DJANGO_SECRET_KEY", "from-env-secret");
        std::env::set_var("DJANGO_DEBUG", "false");
        let settings = from_env();
        assert_eq!(settings.secret_key, "from-env-secret");
        assert!(!settings.debug);
        std::env::remove_var("DJANGO_SECRET_KEY");
        std::env::remove_var("DJANGO_DEBUG");
    }

    // ── merge_json helper ───────────────────────────────────────────

    #[test]
    fn test_merge_json_basic() {
        let base = serde_json::json!({"a": 1, "b": 2});
        let over = serde_json::json!({"b": 3, "c": 4});
        let merged = merge_json(base, over);
        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"], 3);
        assert_eq!(merged["c"], 4);
    }

    #[test]
    fn test_merge_json_nested() {
        let base = serde_json::json!({"outer": {"a": 1, "b": 2}});
        let over = serde_json::json!({"outer": {"b": 3}});
        let merged = merge_json(base, over);
        assert_eq!(merged["outer"]["a"], 1);
        assert_eq!(merged["outer"]["b"], 3);
    }

    #[test]
    fn test_merge_json_array_override() {
        let base = serde_json::json!({"list": [1, 2, 3]});
        let over = serde_json::json!({"list": [4, 5]});
        let merged = merge_json(base, over);
        // Arrays are replaced, not merged
        assert_eq!(merged["list"], serde_json::json!([4, 5]));
    }

    #[test]
    fn test_toml_to_json() {
        let toml_val: toml::Value = toml::from_str(
            r#"
            name = "test"
            count = 42
            flag = true
            items = [1, 2, 3]
            [nested]
            key = "value"
        "#,
        )
        .unwrap();

        let json = toml_to_json(toml_val);
        assert_eq!(json["name"], "test");
        assert_eq!(json["count"], 42);
        assert_eq!(json["flag"], true);
        assert_eq!(json["items"], serde_json::json!([1, 2, 3]));
        assert_eq!(json["nested"]["key"], "value");
    }

    // ── Full flow with env ──────────────────────────────────────────

    #[test]
    fn test_toml_with_env_override() {
        let dir = std::env::temp_dir().join("django_rs_test_toml_env");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings_env.toml");

        let toml_content = r#"
            secret_key = "toml-secret"
            debug = true
        "#;
        std::fs::write(&path, toml_content).unwrap();

        // Override via env
        std::env::set_var("DJANGO_SECRET_KEY", "env-override-secret");
        std::env::set_var("DJANGO_DEBUG", "false");

        let settings = from_toml_file_with_env(&path).unwrap();
        assert_eq!(settings.secret_key, "env-override-secret");
        assert!(!settings.debug);

        std::env::remove_var("DJANGO_SECRET_KEY");
        std::env::remove_var("DJANGO_DEBUG");
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }
}
