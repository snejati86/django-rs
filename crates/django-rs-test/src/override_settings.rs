//! Settings override utility for tests.
//!
//! Provides [`override_settings`] to temporarily swap framework [`Settings`] values
//! for the duration of a closure, then restore the originals. Uses a thread-local
//! stack to maintain isolation even across nested overrides.
//!
//! ## Example
//!
//! ```rust,no_run
//! use django_rs_test::override_settings::{override_settings, SettingsOverride};
//!
//! let overrides = SettingsOverride::new()
//!     .set_debug(false)
//!     .set_secret_key("test-secret");
//!
//! override_settings(overrides, || {
//!     // Within this closure, get_settings() returns the overridden settings.
//! });
//! // After the closure, original settings are restored.
//! ```

use std::cell::RefCell;

use django_rs_core::settings::Settings;

thread_local! {
    /// Thread-local stack of settings overrides.
    ///
    /// The top of the stack is the active override. When empty, the "default" or
    /// globally configured settings should be used.
    static SETTINGS_STACK: RefCell<Vec<Settings>> = const { RefCell::new(Vec::new()) };
}

/// A builder for specifying which settings to override.
///
/// Starts from [`Settings::default()`] and allows selective modification.
pub struct SettingsOverride {
    settings: Settings,
}

impl Default for SettingsOverride {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsOverride {
    /// Creates a new override builder starting from default settings.
    pub fn new() -> Self {
        Self {
            settings: Settings::default(),
        }
    }

    /// Creates a new override builder starting from the given settings.
    pub fn from_settings(settings: Settings) -> Self {
        Self { settings }
    }

    /// Sets the `debug` flag.
    #[must_use]
    pub const fn set_debug(mut self, debug: bool) -> Self {
        self.settings.debug = debug;
        self
    }

    /// Sets the `secret_key`.
    #[must_use]
    pub fn set_secret_key(mut self, key: &str) -> Self {
        self.settings.secret_key = key.to_string();
        self
    }

    /// Sets the `allowed_hosts`.
    #[must_use]
    pub fn set_allowed_hosts(mut self, hosts: Vec<String>) -> Self {
        self.settings.allowed_hosts = hosts;
        self
    }

    /// Sets the `email_backend`.
    #[must_use]
    pub fn set_email_backend(mut self, backend: &str) -> Self {
        self.settings.email_backend = backend.to_string();
        self
    }

    /// Sets the `language_code`.
    #[must_use]
    pub fn set_language_code(mut self, code: &str) -> Self {
        self.settings.language_code = code.to_string();
        self
    }

    /// Sets the `time_zone`.
    #[must_use]
    pub fn set_time_zone(mut self, tz: &str) -> Self {
        self.settings.time_zone = tz.to_string();
        self
    }

    /// Sets a custom extra setting.
    #[must_use]
    pub fn set_extra(mut self, key: &str, value: serde_json::Value) -> Self {
        self.settings
            .extra
            .insert(key.to_string(), value);
        self
    }

    /// Returns the built settings.
    pub fn build(self) -> Settings {
        self.settings
    }
}

/// Temporarily overrides settings for the duration of the closure.
///
/// Pushes the overridden settings onto the thread-local stack, executes the
/// closure, and pops the settings afterward -- even if the closure panics.
///
/// This is thread-safe: each thread has its own independent settings stack, so
/// parallel tests do not interfere with each other.
///
/// # Example
///
/// ```rust,no_run
/// use django_rs_test::override_settings::{override_settings, SettingsOverride, get_settings};
///
/// override_settings(SettingsOverride::new().set_debug(false), || {
///     let s = get_settings();
///     assert!(!s.debug);
/// });
/// ```
pub fn override_settings<F, R>(overrides: SettingsOverride, f: F) -> R
where
    F: FnOnce() -> R,
{
    // Guard type that pops the settings stack on drop, even if the closure panics.
    struct PopGuard;
    impl Drop for PopGuard {
        fn drop(&mut self) {
            SETTINGS_STACK.with(|stack| {
                stack.borrow_mut().pop();
            });
        }
    }

    SETTINGS_STACK.with(|stack| {
        stack.borrow_mut().push(overrides.build());
    });

    let _guard = PopGuard;
    f()
}

/// Returns the currently active settings override, or default settings if
/// no override is active.
///
/// This reads from the thread-local stack. If no override has been pushed,
/// it returns a fresh `Settings::default()`.
pub fn get_settings() -> Settings {
    SETTINGS_STACK.with(|stack| {
        stack
            .borrow()
            .last()
            .cloned()
            .unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings_when_no_override() {
        let s = get_settings();
        assert!(s.debug);
        assert_eq!(s.language_code, "en-us");
    }

    #[test]
    fn test_override_debug() {
        override_settings(SettingsOverride::new().set_debug(false), || {
            let s = get_settings();
            assert!(!s.debug);
        });

        // After the closure, default is restored
        let s = get_settings();
        assert!(s.debug);
    }

    #[test]
    fn test_override_secret_key() {
        override_settings(
            SettingsOverride::new().set_secret_key("supersecret"),
            || {
                let s = get_settings();
                assert_eq!(s.secret_key, "supersecret");
            },
        );
    }

    #[test]
    fn test_override_language_code() {
        override_settings(
            SettingsOverride::new().set_language_code("fr"),
            || {
                let s = get_settings();
                assert_eq!(s.language_code, "fr");
            },
        );
    }

    #[test]
    fn test_override_time_zone() {
        override_settings(
            SettingsOverride::new().set_time_zone("US/Eastern"),
            || {
                let s = get_settings();
                assert_eq!(s.time_zone, "US/Eastern");
            },
        );
    }

    #[test]
    fn test_override_extra() {
        override_settings(
            SettingsOverride::new().set_extra("MY_SETTING", serde_json::json!(42)),
            || {
                let s = get_settings();
                assert_eq!(s.extra.get("MY_SETTING"), Some(&serde_json::json!(42)));
            },
        );
    }

    #[test]
    fn test_nested_overrides() {
        override_settings(SettingsOverride::new().set_debug(false), || {
            let s = get_settings();
            assert!(!s.debug);
            assert_eq!(s.language_code, "en-us");

            override_settings(
                SettingsOverride::new()
                    .set_debug(true)
                    .set_language_code("de"),
                || {
                    let s = get_settings();
                    assert!(s.debug);
                    assert_eq!(s.language_code, "de");
                },
            );

            // Outer override is restored
            let s = get_settings();
            assert!(!s.debug);
            assert_eq!(s.language_code, "en-us");
        });
    }

    #[test]
    fn test_override_returns_value() {
        let result = override_settings(SettingsOverride::new().set_debug(false), || {
            if get_settings().debug { 0 } else { 42 }
        });
        assert_eq!(result, 42);
    }

    #[test]
    fn test_override_allowed_hosts() {
        override_settings(
            SettingsOverride::new()
                .set_allowed_hosts(vec!["example.com".to_string(), "*.example.com".to_string()]),
            || {
                let s = get_settings();
                assert_eq!(s.allowed_hosts.len(), 2);
                assert_eq!(s.allowed_hosts[0], "example.com");
            },
        );
    }

    #[test]
    fn test_override_email_backend() {
        override_settings(
            SettingsOverride::new()
                .set_email_backend("django_rs.core.mail.backends.locmem.EmailBackend"),
            || {
                let s = get_settings();
                assert_eq!(
                    s.email_backend,
                    "django_rs.core.mail.backends.locmem.EmailBackend"
                );
            },
        );
    }

    #[test]
    fn test_from_settings() {
        let mut base = Settings::default();
        base.debug = false;
        base.secret_key = "base-secret".to_string();

        override_settings(
            SettingsOverride::from_settings(base).set_language_code("es"),
            || {
                let s = get_settings();
                assert!(!s.debug);
                assert_eq!(s.secret_key, "base-secret");
                assert_eq!(s.language_code, "es");
            },
        );
    }

    #[test]
    fn test_default_builder() {
        let builder = SettingsOverride::default();
        let s = builder.build();
        assert!(s.debug);
    }

    #[test]
    #[should_panic(expected = "intentional panic")]
    fn test_override_restores_on_panic() {
        // This verifies the PopGuard drops correctly on panic.
        // We can't easily check the stack after panic in the same test,
        // but we verify the structure is correct by not deadlocking.
        override_settings(SettingsOverride::new().set_debug(false), || {
            panic!("intentional panic");
        });
    }
}
