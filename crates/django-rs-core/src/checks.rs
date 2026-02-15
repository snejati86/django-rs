//! System check framework for the django-rs framework.
//!
//! This module provides a framework for registering and running checks on your
//! project configuration. It mirrors Django's `django.core.checks` module.
//!
//! ## Overview
//!
//! - [`CheckMessage`]: A diagnostic message from a check (with level, message, hint, etc.).
//! - [`CheckLevel`]: Severity level (Debug, Info, Warning, Error, Critical).
//! - [`CheckRegistry`]: Registry for check functions with tag-based filtering.
//! - Built-in checks: `SECRET_KEY` set, `DEBUG` is false in production, `ALLOWED_HOSTS` set.
//!
//! ## Examples
//!
//! ```
//! use django_rs_core::checks::{CheckMessage, CheckLevel, CheckRegistry};
//!
//! let mut registry = CheckRegistry::new();
//! registry.register(
//!     |_settings| {
//!         vec![CheckMessage::warning(
//!             "Custom check warning",
//!             Some("Consider fixing this."),
//!             None,
//!             Some("myapp.W001"),
//!         )]
//!     },
//!     &["myapp"],
//! );
//!
//! let settings = django_rs_core::settings::Settings::default();
//! let messages = registry.run_checks(None, &settings);
//! assert!(!messages.is_empty());
//! ```

use crate::settings::Settings;

/// Severity level for a check message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckLevel {
    /// Debugging information.
    Debug = 0,
    /// Informational message.
    Info = 1,
    /// A potential problem.
    Warning = 2,
    /// A definite problem that should be fixed.
    Error = 3,
    /// A critical error that prevents the application from running.
    Critical = 4,
}

impl std::fmt::Display for CheckLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Debug => write!(f, "DEBUG"),
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Error => write!(f, "ERROR"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// A diagnostic message produced by a system check.
///
/// Each message has a severity level, a human-readable message, an optional hint,
/// the object that the issue relates to, and an optional identifier.
#[derive(Debug, Clone)]
pub struct CheckMessage {
    /// The severity level.
    pub level: CheckLevel,
    /// The human-readable message describing the issue.
    pub msg: String,
    /// An optional hint on how to fix the issue.
    pub hint: Option<String>,
    /// The object (setting, model, etc.) that has the issue.
    pub obj: Option<String>,
    /// A unique identifier for this check message (e.g. "security.W001").
    pub id: Option<String>,
}

impl CheckMessage {
    /// Creates a new `CheckMessage` with the given level and details.
    pub fn new(
        level: CheckLevel,
        msg: impl Into<String>,
        hint: Option<&str>,
        obj: Option<&str>,
        id: Option<&str>,
    ) -> Self {
        Self {
            level,
            msg: msg.into(),
            hint: hint.map(String::from),
            obj: obj.map(String::from),
            id: id.map(String::from),
        }
    }

    /// Creates a debug-level message.
    pub fn debug(msg: impl Into<String>, hint: Option<&str>, obj: Option<&str>, id: Option<&str>) -> Self {
        Self::new(CheckLevel::Debug, msg, hint, obj, id)
    }

    /// Creates an info-level message.
    pub fn info(msg: impl Into<String>, hint: Option<&str>, obj: Option<&str>, id: Option<&str>) -> Self {
        Self::new(CheckLevel::Info, msg, hint, obj, id)
    }

    /// Creates a warning-level message.
    pub fn warning(msg: impl Into<String>, hint: Option<&str>, obj: Option<&str>, id: Option<&str>) -> Self {
        Self::new(CheckLevel::Warning, msg, hint, obj, id)
    }

    /// Creates an error-level message.
    pub fn error(msg: impl Into<String>, hint: Option<&str>, obj: Option<&str>, id: Option<&str>) -> Self {
        Self::new(CheckLevel::Error, msg, hint, obj, id)
    }

    /// Creates a critical-level message.
    pub fn critical(msg: impl Into<String>, hint: Option<&str>, obj: Option<&str>, id: Option<&str>) -> Self {
        Self::new(CheckLevel::Critical, msg, hint, obj, id)
    }

    /// Returns `true` if this is a warning or higher severity.
    pub fn is_serious(&self) -> bool {
        self.level >= CheckLevel::Warning
    }
}

impl std::fmt::Display for CheckMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref id) = self.id {
            write!(f, "({id}) ")?;
        }
        write!(f, "{}: {}", self.level, self.msg)?;
        if let Some(ref hint) = self.hint {
            write!(f, "\n\tHINT: {hint}")?;
        }
        if let Some(ref obj) = self.obj {
            write!(f, "\n\tObject: {obj}")?;
        }
        Ok(())
    }
}

/// A check function that receives settings and returns diagnostic messages.
pub type CheckFn = fn(&Settings) -> Vec<CheckMessage>;

/// A registered check with associated tags.
struct RegisteredCheck {
    func: CheckFn,
    tags: Vec<String>,
}

/// Registry for system check functions.
///
/// Check functions can be registered with tags, and then run all at once
/// or filtered by tag.
pub struct CheckRegistry {
    checks: Vec<RegisteredCheck>,
}

impl CheckRegistry {
    /// Creates a new empty check registry.
    pub const fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Creates a new check registry pre-loaded with built-in checks.
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(check_secret_key, &["security"]);
        registry.register(check_debug_production, &["security"]);
        registry.register(check_allowed_hosts, &["security"]);
        registry
    }

    /// Registers a check function with the given tags.
    pub fn register(&mut self, func: CheckFn, tags: &[&str]) {
        self.checks.push(RegisteredCheck {
            func,
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
        });
    }

    /// Runs all registered checks (or only those matching the given tags)
    /// and collects all resulting messages.
    ///
    /// If `tags` is `None`, all checks are run. If `Some(&["security"])`,
    /// only checks tagged with "security" are run.
    pub fn run_checks(
        &self,
        tags: Option<&[&str]>,
        settings: &Settings,
    ) -> Vec<CheckMessage> {
        let mut messages = Vec::new();

        for check in &self.checks {
            let should_run = tags.map_or(true, |filter_tags| {
                filter_tags.iter().any(|t| check.tags.contains(&(*t).to_string()))
            });

            if should_run {
                messages.extend((check.func)(settings));
            }
        }

        messages
    }

    /// Returns the number of registered checks.
    pub fn len(&self) -> usize {
        self.checks.len()
    }

    /// Returns `true` if no checks are registered.
    pub fn is_empty(&self) -> bool {
        self.checks.is_empty()
    }
}

impl Default for CheckRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// Built-in checks
// ============================================================

/// Checks that `SECRET_KEY` is set and non-empty.
fn check_secret_key(settings: &Settings) -> Vec<CheckMessage> {
    let mut messages = Vec::new();

    if settings.secret_key.is_empty() {
        messages.push(CheckMessage::warning(
            "SECRET_KEY is empty. This is insecure for production.",
            Some("Set a strong, unique SECRET_KEY in your settings."),
            Some("settings.secret_key"),
            Some("security.W001"),
        ));
    } else if settings.secret_key.len() < 50 {
        messages.push(CheckMessage::warning(
            "SECRET_KEY is too short. Use at least 50 characters.",
            Some("Generate a longer SECRET_KEY for better security."),
            Some("settings.secret_key"),
            Some("security.W002"),
        ));
    }

    messages
}

/// Checks that `DEBUG` is false in production (when `ALLOWED_HOSTS` is set, indicating production).
fn check_debug_production(settings: &Settings) -> Vec<CheckMessage> {
    let mut messages = Vec::new();

    if settings.debug && !settings.allowed_hosts.is_empty() {
        messages.push(CheckMessage::warning(
            "DEBUG is True with ALLOWED_HOSTS set. This looks like a production configuration with debug enabled.",
            Some("Set DEBUG to false for production deployments."),
            Some("settings.debug"),
            Some("security.W003"),
        ));
    }

    messages
}

/// Checks that `ALLOWED_HOSTS` is set when `DEBUG` is false.
fn check_allowed_hosts(settings: &Settings) -> Vec<CheckMessage> {
    let mut messages = Vec::new();

    if !settings.debug && settings.allowed_hosts.is_empty() {
        messages.push(CheckMessage::error(
            "ALLOWED_HOSTS is empty with DEBUG=false. Your site will not serve any requests.",
            Some("Set ALLOWED_HOSTS to a list of allowed hostnames."),
            Some("settings.allowed_hosts"),
            Some("security.E001"),
        ));
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── CheckLevel ──────────────────────────────────────────────────

    #[test]
    fn test_check_level_ordering() {
        assert!(CheckLevel::Debug < CheckLevel::Info);
        assert!(CheckLevel::Info < CheckLevel::Warning);
        assert!(CheckLevel::Warning < CheckLevel::Error);
        assert!(CheckLevel::Error < CheckLevel::Critical);
    }

    #[test]
    fn test_check_level_display() {
        assert_eq!(CheckLevel::Debug.to_string(), "DEBUG");
        assert_eq!(CheckLevel::Info.to_string(), "INFO");
        assert_eq!(CheckLevel::Warning.to_string(), "WARNING");
        assert_eq!(CheckLevel::Error.to_string(), "ERROR");
        assert_eq!(CheckLevel::Critical.to_string(), "CRITICAL");
    }

    // ── CheckMessage ────────────────────────────────────────────────

    #[test]
    fn test_check_message_constructors() {
        let m = CheckMessage::debug("msg", Some("hint"), Some("obj"), Some("id"));
        assert_eq!(m.level, CheckLevel::Debug);
        assert_eq!(m.msg, "msg");
        assert_eq!(m.hint.as_deref(), Some("hint"));
        assert_eq!(m.obj.as_deref(), Some("obj"));
        assert_eq!(m.id.as_deref(), Some("id"));

        let m = CheckMessage::info("info msg", None, None, None);
        assert_eq!(m.level, CheckLevel::Info);

        let m = CheckMessage::warning("warn msg", None, None, None);
        assert_eq!(m.level, CheckLevel::Warning);

        let m = CheckMessage::error("err msg", None, None, None);
        assert_eq!(m.level, CheckLevel::Error);

        let m = CheckMessage::critical("crit msg", None, None, None);
        assert_eq!(m.level, CheckLevel::Critical);
    }

    #[test]
    fn test_check_message_is_serious() {
        assert!(!CheckMessage::debug("", None, None, None).is_serious());
        assert!(!CheckMessage::info("", None, None, None).is_serious());
        assert!(CheckMessage::warning("", None, None, None).is_serious());
        assert!(CheckMessage::error("", None, None, None).is_serious());
        assert!(CheckMessage::critical("", None, None, None).is_serious());
    }

    #[test]
    fn test_check_message_display() {
        let m = CheckMessage::warning(
            "Bad config",
            Some("Fix it"),
            Some("settings.foo"),
            Some("myapp.W001"),
        );
        let s = m.to_string();
        assert!(s.contains("(myapp.W001)"));
        assert!(s.contains("WARNING: Bad config"));
        assert!(s.contains("HINT: Fix it"));
        assert!(s.contains("Object: settings.foo"));
    }

    #[test]
    fn test_check_message_display_minimal() {
        let m = CheckMessage::info("Just info", None, None, None);
        let s = m.to_string();
        assert_eq!(s, "INFO: Just info");
    }

    // ── CheckRegistry ───────────────────────────────────────────────

    #[test]
    fn test_registry_empty() {
        let registry = CheckRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_register_and_run() {
        let mut registry = CheckRegistry::new();
        registry.register(
            |_| vec![CheckMessage::warning("test", None, None, Some("test.W001"))],
            &["test"],
        );

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let settings = Settings::default();
        let messages = registry.run_checks(None, &settings);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id.as_deref(), Some("test.W001"));
    }

    #[test]
    fn test_registry_tag_filtering() {
        let mut registry = CheckRegistry::new();
        registry.register(
            |_| vec![CheckMessage::warning("security issue", None, None, None)],
            &["security"],
        );
        registry.register(
            |_| vec![CheckMessage::info("model info", None, None, None)],
            &["models"],
        );

        let settings = Settings::default();

        // Run all
        let all = registry.run_checks(None, &settings);
        assert_eq!(all.len(), 2);

        // Filter by security
        let security_only = registry.run_checks(Some(&["security"]), &settings);
        assert_eq!(security_only.len(), 1);
        assert!(security_only[0].msg.contains("security"));

        // Filter by models
        let models_only = registry.run_checks(Some(&["models"]), &settings);
        assert_eq!(models_only.len(), 1);
        assert!(models_only[0].msg.contains("model"));

        // Filter by nonexistent tag
        let none = registry.run_checks(Some(&["templates"]), &settings);
        assert!(none.is_empty());
    }

    #[test]
    fn test_registry_multiple_tags() {
        let mut registry = CheckRegistry::new();
        registry.register(
            |_| vec![CheckMessage::info("multi-tagged", None, None, None)],
            &["security", "models"],
        );

        let settings = Settings::default();

        // Should match either tag
        let by_security = registry.run_checks(Some(&["security"]), &settings);
        assert_eq!(by_security.len(), 1);

        let by_models = registry.run_checks(Some(&["models"]), &settings);
        assert_eq!(by_models.len(), 1);
    }

    #[test]
    fn test_registry_check_returns_multiple() {
        let mut registry = CheckRegistry::new();
        registry.register(
            |_| {
                vec![
                    CheckMessage::warning("w1", None, None, None),
                    CheckMessage::error("e1", None, None, None),
                ]
            },
            &["test"],
        );

        let settings = Settings::default();
        let messages = registry.run_checks(None, &settings);
        assert_eq!(messages.len(), 2);
    }

    // ── Built-in checks ─────────────────────────────────────────────

    #[test]
    fn test_builtin_check_secret_key_empty() {
        let settings = Settings {
            secret_key: String::new(),
            ..Settings::default()
        };
        let messages = check_secret_key(&settings);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id.as_deref(), Some("security.W001"));
    }

    #[test]
    fn test_builtin_check_secret_key_short() {
        let settings = Settings {
            secret_key: "short".to_string(),
            ..Settings::default()
        };
        let messages = check_secret_key(&settings);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id.as_deref(), Some("security.W002"));
    }

    #[test]
    fn test_builtin_check_secret_key_ok() {
        let settings = Settings {
            secret_key: "a".repeat(50),
            ..Settings::default()
        };
        let messages = check_secret_key(&settings);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_builtin_check_debug_production() {
        // debug=true with allowed_hosts set => warning
        let settings = Settings {
            debug: true,
            allowed_hosts: vec!["example.com".to_string()],
            ..Settings::default()
        };
        let messages = check_debug_production(&settings);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id.as_deref(), Some("security.W003"));
    }

    #[test]
    fn test_builtin_check_debug_development() {
        // debug=true with no allowed_hosts => OK (development)
        let settings = Settings {
            debug: true,
            allowed_hosts: Vec::new(),
            ..Settings::default()
        };
        let messages = check_debug_production(&settings);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_builtin_check_debug_production_ok() {
        // debug=false with allowed_hosts => OK (proper production)
        let settings = Settings {
            debug: false,
            allowed_hosts: vec!["example.com".to_string()],
            ..Settings::default()
        };
        let messages = check_debug_production(&settings);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_builtin_check_allowed_hosts_missing() {
        // debug=false with no allowed_hosts => error
        let settings = Settings {
            debug: false,
            allowed_hosts: Vec::new(),
            ..Settings::default()
        };
        let messages = check_allowed_hosts(&settings);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].level, CheckLevel::Error);
        assert_eq!(messages[0].id.as_deref(), Some("security.E001"));
    }

    #[test]
    fn test_builtin_check_allowed_hosts_ok() {
        // debug=false with allowed_hosts set => OK
        let settings = Settings {
            debug: false,
            allowed_hosts: vec!["example.com".to_string()],
            ..Settings::default()
        };
        let messages = check_allowed_hosts(&settings);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_builtin_check_allowed_hosts_debug_on() {
        // debug=true with no allowed_hosts => OK (development)
        let settings = Settings {
            debug: true,
            allowed_hosts: Vec::new(),
            ..Settings::default()
        };
        let messages = check_allowed_hosts(&settings);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_registry_with_builtins() {
        let registry = CheckRegistry::with_builtins();
        assert_eq!(registry.len(), 3);

        // Default settings: debug=true, empty secret_key, no allowed_hosts
        let settings = Settings::default();
        let messages = registry.run_checks(Some(&["security"]), &settings);
        // Should get at least the secret_key warning
        assert!(!messages.is_empty());
        assert!(messages.iter().any(|m| m.id.as_deref() == Some("security.W001")));
    }

    #[test]
    fn test_registry_with_builtins_production_problems() {
        let registry = CheckRegistry::with_builtins();
        let settings = Settings {
            debug: false,
            secret_key: String::new(),
            allowed_hosts: Vec::new(),
            ..Settings::default()
        };
        let messages = registry.run_checks(None, &settings);
        // Should get: empty secret key (W001) + empty allowed_hosts (E001)
        assert!(messages.iter().any(|m| m.id.as_deref() == Some("security.W001")));
        assert!(messages.iter().any(|m| m.id.as_deref() == Some("security.E001")));
    }
}
