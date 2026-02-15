//! Blog application settings.
//!
//! Demonstrates how to configure a django-rs application using the Settings
//! struct. In a real project, settings would be loaded from environment
//! variables or a TOML configuration file.

use std::collections::HashMap;
use std::path::PathBuf;

use django_rs_core::settings::{DatabaseSettings, Settings, TemplateSettings};

/// Creates the blog application settings.
///
/// Returns a fully configured `Settings` instance suitable for the
/// blog example. This mirrors Django's `settings.py`.
pub fn blog_settings() -> Settings {
    let mut databases = HashMap::new();
    databases.insert(
        "default".to_string(),
        DatabaseSettings {
            engine: "django_rs.db.backends.sqlite3".to_string(),
            name: "blog.sqlite3".to_string(),
            ..DatabaseSettings::default()
        },
    );

    Settings {
        debug: true,
        secret_key: "blog-example-secret-key-not-for-production".to_string(),
        allowed_hosts: vec!["localhost".to_string(), "127.0.0.1".to_string()],
        root_urlconf: "blog.urls".to_string(),
        installed_apps: vec![
            "django_rs.auth".to_string(),
            "django_rs.admin".to_string(),
            "blog".to_string(),
        ],

        databases,

        static_url: "/static/".to_string(),
        static_root: Some(PathBuf::from("static_collected")),
        staticfiles_dirs: vec![PathBuf::from("static")],

        templates: vec![TemplateSettings {
            backend: "django_rs.template.backends.tera".to_string(),
            dirs: vec![PathBuf::from("templates")],
            app_dirs: true,
            options: HashMap::new(),
        }],

        middleware: vec![
            "django_rs.middleware.security.SecurityMiddleware".to_string(),
            "django_rs.middleware.common.CommonMiddleware".to_string(),
            "django_rs.middleware.csrf.CsrfViewMiddleware".to_string(),
            "django_rs.middleware.auth.AuthenticationMiddleware".to_string(),
        ],

        email_backend: "django_rs.core.mail.backends.console.EmailBackend".to_string(),
        email_host: "localhost".to_string(),
        email_port: 25,

        language_code: "en-us".to_string(),
        time_zone: "UTC".to_string(),
        use_tz: true,

        log_level: "info".to_string(),

        ..Settings::default()
    }
}

/// Loads settings from a TOML configuration file.
///
/// Falls back to `blog_settings()` if the file cannot be loaded.
/// This demonstrates loading configuration from external files.
pub fn load_settings_from_toml(path: &str) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            // Parse the TOML content
            match toml::from_str::<toml::Value>(&content) {
                Ok(config) => {
                    let mut settings = blog_settings();

                    // Override settings from the TOML file
                    if let Some(debug) = config.get("debug").and_then(|v| v.as_bool()) {
                        settings.debug = debug;
                    }

                    if let Some(secret_key) = config.get("secret_key").and_then(|v| v.as_str()) {
                        settings.secret_key = secret_key.to_string();
                    }

                    if let Some(host) = config.get("email_host").and_then(|v| v.as_str()) {
                        settings.email_host = host.to_string();
                    }

                    if let Some(port) = config.get("email_port").and_then(|v| v.as_integer()) {
                        settings.email_port = port as u16;
                    }

                    if let Some(level) = config.get("log_level").and_then(|v| v.as_str()) {
                        settings.log_level = level.to_string();
                    }

                    if let Some(hosts) = config.get("allowed_hosts").and_then(|v| v.as_array()) {
                        settings.allowed_hosts = hosts
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                    }

                    settings
                }
                Err(e) => {
                    tracing::warn!("Failed to parse TOML settings: {e}. Using defaults.");
                    blog_settings()
                }
            }
        }
        Err(e) => {
            tracing::info!("Settings file not found ({e}). Using defaults.");
            blog_settings()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blog_settings_defaults() {
        let settings = blog_settings();
        assert!(settings.debug);
        assert!(!settings.secret_key.is_empty());
        assert_eq!(settings.installed_apps.len(), 3);
        assert!(settings.installed_apps.contains(&"blog".to_string()));
        assert_eq!(settings.root_urlconf, "blog.urls");
    }

    #[test]
    fn test_blog_settings_database() {
        let settings = blog_settings();
        let db = settings.databases.get("default").unwrap();
        assert_eq!(db.engine, "django_rs.db.backends.sqlite3");
        assert_eq!(db.name, "blog.sqlite3");
    }

    #[test]
    fn test_blog_settings_templates() {
        let settings = blog_settings();
        assert_eq!(settings.templates.len(), 1);
        assert!(settings.templates[0].app_dirs);
    }

    #[test]
    fn test_blog_settings_middleware() {
        let settings = blog_settings();
        assert_eq!(settings.middleware.len(), 4);
    }

    #[test]
    fn test_load_settings_from_missing_file() {
        // Should fall back to defaults without error
        let settings = load_settings_from_toml("/nonexistent/config.toml");
        assert!(settings.debug);
        assert!(!settings.secret_key.is_empty());
    }

    #[test]
    fn test_load_settings_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blog.toml");
        std::fs::write(
            &path,
            r#"
debug = false
secret_key = "my-secret"
log_level = "debug"
email_host = "smtp.example.com"
email_port = 587
allowed_hosts = ["example.com", "www.example.com"]
"#,
        )
        .unwrap();

        let settings = load_settings_from_toml(path.to_str().unwrap());
        assert!(!settings.debug);
        assert_eq!(settings.secret_key, "my-secret");
        assert_eq!(settings.log_level, "debug");
        assert_eq!(settings.email_host, "smtp.example.com");
        assert_eq!(settings.email_port, 587);
        assert_eq!(settings.allowed_hosts.len(), 2);
        assert!(settings.allowed_hosts.contains(&"example.com".to_string()));
    }

    #[test]
    fn test_load_settings_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.toml");
        std::fs::write(&path, "this is not valid toml [[[").unwrap();

        // Should fall back to defaults
        let settings = load_settings_from_toml(path.to_str().unwrap());
        assert!(settings.debug);
    }
}
