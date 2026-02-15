//! Settings system for the django-rs framework.
//!
//! This module provides the [`Settings`] struct, which holds all framework configuration,
//! and [`LazySettings`], a globally-accessible, lazily-initialized settings instance.
//! The design mirrors Django's `django.conf.settings` with sensible defaults.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Database connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSettings {
    /// The database engine (e.g. `django_rs.db.backends.postgresql`).
    pub engine: String,
    /// The database name (or file path for `SQLite`).
    pub name: String,
    /// The database user.
    pub user: String,
    /// The database password.
    pub password: String,
    /// The database host.
    pub host: String,
    /// The database port.
    pub port: u16,
    /// Additional engine-specific options.
    pub options: HashMap<String, String>,
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            engine: "django_rs.db.backends.sqlite3".to_string(),
            name: "db.sqlite3".to_string(),
            user: String::new(),
            password: String::new(),
            host: String::new(),
            port: 0,
            options: HashMap::new(),
        }
    }
}

/// Template engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSettings {
    /// The template backend (e.g. `django_rs.template.backends.tera`).
    pub backend: String,
    /// Directories to search for template files.
    pub dirs: Vec<PathBuf>,
    /// Whether to look for templates inside installed apps.
    pub app_dirs: bool,
    /// Additional backend-specific options.
    pub options: HashMap<String, serde_json::Value>,
}

impl Default for TemplateSettings {
    fn default() -> Self {
        Self {
            backend: "django_rs.template.backends.tera".to_string(),
            dirs: Vec::new(),
            app_dirs: false,
            options: HashMap::new(),
        }
    }
}

/// Cache backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSettings {
    /// The cache backend (e.g. `django_rs.core.cache.backends.locmem`).
    pub backend: String,
    /// The cache location (connection string, file path, etc.).
    pub location: String,
    /// Cache timeout in seconds.
    pub timeout: u64,
    /// Additional backend-specific options.
    pub options: HashMap<String, serde_json::Value>,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            backend: "django_rs.core.cache.backends.locmem".to_string(),
            location: String::new(),
            timeout: 300,
            options: HashMap::new(),
        }
    }
}

/// The complete set of framework settings.
///
/// This mirrors Django's `settings` module with sensible defaults. Use
/// [`SETTINGS`] to access the global instance.
///
/// # Examples
///
/// ```
/// use django_rs_core::settings::Settings;
///
/// let settings = Settings::default();
/// assert!(settings.debug);
/// assert_eq!(settings.language_code, "en-us");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    // ── Core ─────────────────────────────────────────────────────────

    /// Whether debug mode is enabled.
    pub debug: bool,
    /// The secret key used for cryptographic signing.
    pub secret_key: String,
    /// Hostnames that this application can serve.
    pub allowed_hosts: Vec<String>,
    /// List of installed application dotted paths.
    pub installed_apps: Vec<String>,
    /// The root URL configuration module.
    pub root_urlconf: String,

    // ── Database ─────────────────────────────────────────────────────

    /// Database configurations, keyed by alias (e.g. "default").
    pub databases: HashMap<String, DatabaseSettings>,

    // ── Static files ─────────────────────────────────────────────────

    /// URL prefix for static files.
    pub static_url: String,
    /// Absolute path to the directory where `collectstatic` will place files.
    pub static_root: Option<PathBuf>,
    /// Additional directories to search for static files.
    pub staticfiles_dirs: Vec<PathBuf>,

    // ── Media ────────────────────────────────────────────────────────

    /// URL prefix for user-uploaded media files.
    pub media_url: String,
    /// Absolute path to the directory for user-uploaded files.
    pub media_root: Option<PathBuf>,

    // ── Templates ────────────────────────────────────────────────────

    /// Template engine configurations.
    pub templates: Vec<TemplateSettings>,

    // ── Middleware ────────────────────────────────────────────────────

    /// Ordered list of middleware dotted paths.
    pub middleware: Vec<String>,

    // ── Auth ─────────────────────────────────────────────────────────

    /// The model to use for the user (e.g. "auth.User").
    pub auth_user_model: String,
    /// Authentication backend dotted paths.
    pub authentication_backends: Vec<String>,
    /// Password hasher dotted paths, in order of preference.
    pub password_hashers: Vec<String>,

    // ── Security ─────────────────────────────────────────────────────

    /// The name of the CSRF cookie.
    pub csrf_cookie_name: String,
    /// Origins that are trusted for CSRF checks.
    pub csrf_trusted_origins: Vec<String>,
    /// Whether to redirect all HTTP requests to HTTPS.
    pub secure_ssl_redirect: bool,
    /// The number of seconds for the HSTS header.
    pub secure_hsts_seconds: u64,
    /// The name of the session cookie.
    pub session_cookie_name: String,
    /// The session cookie max age in seconds.
    pub session_cookie_age: u64,

    // ── Internationalization ─────────────────────────────────────────

    /// The language code (e.g. "en-us").
    pub language_code: String,
    /// The default time zone (e.g. "UTC").
    pub time_zone: String,
    /// Whether to use timezone-aware datetimes.
    pub use_tz: bool,

    // ── Email ────────────────────────────────────────────────────────

    /// The email backend dotted path.
    pub email_backend: String,
    /// The SMTP host.
    pub email_host: String,
    /// The SMTP port.
    pub email_port: u16,

    // ── Logging ──────────────────────────────────────────────────────

    /// The log level (e.g. "info", "debug", "warn").
    pub log_level: String,

    // ── Cache ────────────────────────────────────────────────────────

    /// Cache backend configurations, keyed by alias (e.g. "default").
    pub caches: HashMap<String, CacheSettings>,

    // ── Escape hatch ─────────────────────────────────────────────────

    /// Custom settings that don't fit into the above categories.
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut databases = HashMap::new();
        databases.insert("default".to_string(), DatabaseSettings::default());

        let mut caches = HashMap::new();
        caches.insert("default".to_string(), CacheSettings::default());

        Self {
            // Core
            debug: true,
            secret_key: String::new(),
            allowed_hosts: Vec::new(),
            installed_apps: Vec::new(),
            root_urlconf: String::new(),

            // Database
            databases,

            // Static files
            static_url: "/static/".to_string(),
            static_root: None,
            staticfiles_dirs: Vec::new(),

            // Media
            media_url: "/media/".to_string(),
            media_root: None,

            // Templates
            templates: vec![TemplateSettings::default()],

            // Middleware
            middleware: vec![
                "django_rs.middleware.security.SecurityMiddleware".to_string(),
                "django_rs.middleware.common.CommonMiddleware".to_string(),
                "django_rs.middleware.csrf.CsrfViewMiddleware".to_string(),
                "django_rs.middleware.clickjacking.XFrameOptionsMiddleware".to_string(),
            ],

            // Auth
            auth_user_model: "auth.User".to_string(),
            authentication_backends: vec![
                "django_rs.auth.backends.ModelBackend".to_string(),
            ],
            password_hashers: vec![
                "django_rs.auth.hashers.Argon2PasswordHasher".to_string(),
                "django_rs.auth.hashers.BCryptPasswordHasher".to_string(),
            ],

            // Security
            csrf_cookie_name: "csrftoken".to_string(),
            csrf_trusted_origins: Vec::new(),
            secure_ssl_redirect: false,
            secure_hsts_seconds: 0,
            session_cookie_name: "sessionid".to_string(),
            session_cookie_age: 1_209_600, // 2 weeks

            // Internationalization
            language_code: "en-us".to_string(),
            time_zone: "UTC".to_string(),
            use_tz: true,

            // Email
            email_backend: "django_rs.core.mail.backends.smtp.EmailBackend".to_string(),
            email_host: "localhost".to_string(),
            email_port: 25,

            // Logging
            log_level: "info".to_string(),

            // Cache
            caches,

            // Extra
            extra: HashMap::new(),
        }
    }
}

/// A lazily-initialized, globally-accessible settings container.
///
/// Call [`configure`](LazySettings::configure) once at startup to set the
/// settings, then use [`get`](LazySettings::get) to access them.
///
/// # Panics
///
/// [`get`](LazySettings::get) panics if settings have not been configured.
/// [`configure`](LazySettings::configure) panics if called more than once.
pub struct LazySettings {
    inner: OnceLock<Settings>,
}

impl Default for LazySettings {
    fn default() -> Self {
        Self::new()
    }
}

impl LazySettings {
    /// Creates a new, unconfigured `LazySettings`.
    pub const fn new() -> Self {
        Self {
            inner: OnceLock::new(),
        }
    }

    /// Configures the global settings. Must be called exactly once.
    ///
    /// # Panics
    ///
    /// Panics if settings have already been configured.
    pub fn configure(&self, settings: Settings) {
        self.inner
            .set(settings)
            .expect("Settings have already been configured");
    }

    /// Returns a reference to the configured settings.
    ///
    /// # Panics
    ///
    /// Panics if settings have not been configured.
    pub fn get(&self) -> &Settings {
        self.inner
            .get()
            .expect("Settings have not been configured. Call SETTINGS.configure() first.")
    }

    /// Returns `true` if settings have been configured.
    pub fn is_configured(&self) -> bool {
        self.inner.get().is_some()
    }
}

/// The global settings instance.
///
/// Call `SETTINGS.configure(settings)` once at application startup, then
/// access settings via `SETTINGS.get()` anywhere in the framework.
pub static SETTINGS: LazySettings = LazySettings::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let s = Settings::default();
        assert!(s.debug);
        assert!(s.secret_key.is_empty());
        assert_eq!(s.static_url, "/static/");
        assert_eq!(s.media_url, "/media/");
        assert_eq!(s.language_code, "en-us");
        assert_eq!(s.time_zone, "UTC");
        assert!(s.use_tz);
        assert_eq!(s.email_port, 25);
        assert_eq!(s.session_cookie_age, 1_209_600);
        assert_eq!(s.csrf_cookie_name, "csrftoken");
        assert_eq!(s.session_cookie_name, "sessionid");
        assert_eq!(s.log_level, "info");
        assert!(!s.secure_ssl_redirect);
        assert_eq!(s.secure_hsts_seconds, 0);
        assert_eq!(s.auth_user_model, "auth.User");
    }

    #[test]
    fn test_default_database() {
        let s = Settings::default();
        let db = s.databases.get("default").expect("default db should exist");
        assert_eq!(db.engine, "django_rs.db.backends.sqlite3");
        assert_eq!(db.name, "db.sqlite3");
    }

    #[test]
    fn test_default_cache() {
        let s = Settings::default();
        let cache = s.caches.get("default").expect("default cache should exist");
        assert_eq!(cache.backend, "django_rs.core.cache.backends.locmem");
        assert_eq!(cache.timeout, 300);
    }

    #[test]
    fn test_default_middleware() {
        let s = Settings::default();
        assert_eq!(s.middleware.len(), 4);
        assert!(s.middleware[0].contains("SecurityMiddleware"));
    }

    #[test]
    fn test_default_password_hashers() {
        let s = Settings::default();
        assert_eq!(s.password_hashers.len(), 2);
        assert!(s.password_hashers[0].contains("Argon2"));
    }

    #[test]
    fn test_lazy_settings_configure_and_get() {
        let lazy = LazySettings::new();
        assert!(!lazy.is_configured());

        let mut settings = Settings::default();
        settings.debug = false;
        settings.secret_key = "test-secret".to_string();

        lazy.configure(settings);
        assert!(lazy.is_configured());
        assert!(!lazy.get().debug);
        assert_eq!(lazy.get().secret_key, "test-secret");
    }

    #[test]
    #[should_panic(expected = "already been configured")]
    fn test_lazy_settings_double_configure_panics() {
        let lazy = LazySettings::new();
        lazy.configure(Settings::default());
        lazy.configure(Settings::default());
    }

    #[test]
    #[should_panic(expected = "not been configured")]
    fn test_lazy_settings_get_before_configure_panics() {
        let lazy = LazySettings::new();
        let _ = lazy.get();
    }
}
