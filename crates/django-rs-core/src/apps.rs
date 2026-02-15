//! Application registry for the django-rs framework.
//!
//! This module provides the [`AppConfig`] trait and [`AppRegistry`], which together
//! manage the lifecycle of installed applications. This mirrors Django's
//! `django.apps` module.

use std::collections::HashMap;

/// Configuration for an installed application.
///
/// Implement this trait for each application that needs to participate in
/// the framework lifecycle. The [`ready`](AppConfig::ready) method is called
/// after all applications have been loaded.
///
/// # Examples
///
/// ```
/// use django_rs_core::apps::AppConfig;
///
/// struct MyApp;
///
/// impl AppConfig for MyApp {
///     fn name(&self) -> &str { "my_app" }
///     fn verbose_name(&self) -> &str { "My Application" }
/// }
/// ```
pub trait AppConfig: Send + Sync {
    /// Returns the full Python-style dotted path of the application.
    fn name(&self) -> &str;

    /// Returns a short label derived from the name (the last component).
    ///
    /// For example, `"django_rs.contrib.auth"` yields `"auth"`.
    fn label(&self) -> &str {
        self.name().rsplit('.').next().unwrap_or_else(|| self.name())
    }

    /// Returns a human-readable name for the application.
    fn verbose_name(&self) -> &str {
        self.name()
    }

    /// Called after all apps have been loaded.
    ///
    /// Override this to perform one-time initialization such as registering
    /// signal handlers or performing checks.
    fn ready(&self) {}
}

/// The central registry of installed applications.
///
/// Applications are registered via [`register`](AppRegistry::register) and then
/// [`populate`](AppRegistry::populate) is called once to finalize initialization
/// (calling each app's `ready()` method).
pub struct AppRegistry {
    apps: Vec<Box<dyn AppConfig>>,
    app_labels: HashMap<String, usize>,
    ready: bool,
}

impl Default for AppRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRegistry {
    /// Creates a new, empty `AppRegistry`.
    pub fn new() -> Self {
        Self {
            apps: Vec::new(),
            app_labels: HashMap::new(),
            ready: false,
        }
    }

    /// Registers an application.
    ///
    /// # Panics
    ///
    /// Panics if an application with the same label is already registered,
    /// or if [`populate`](AppRegistry::populate) has already been called.
    pub fn register(&mut self, app: Box<dyn AppConfig>) {
        assert!(
            !self.ready,
            "Cannot register apps after the registry has been populated"
        );

        let label = app.label().to_string();
        assert!(
            !self.app_labels.contains_key(&label),
            "Application with label '{label}' is already registered"
        );

        let index = self.apps.len();
        self.app_labels.insert(label, index);
        self.apps.push(app);
    }

    /// Returns the configuration for the app with the given label, if registered.
    pub fn get_app_config(&self, label: &str) -> Option<&dyn AppConfig> {
        self.app_labels
            .get(label)
            .map(|&idx| self.apps[idx].as_ref())
    }

    /// Returns a slice of all registered app configurations.
    pub fn get_app_configs(&self) -> &[Box<dyn AppConfig>] {
        &self.apps
    }

    /// Finalizes the registry by calling `ready()` on each app in registration order.
    ///
    /// # Panics
    ///
    /// Panics if `populate` has already been called.
    pub fn populate(&mut self) {
        assert!(
            !self.ready,
            "AppRegistry has already been populated"
        );

        for app in &self.apps {
            app.ready();
        }

        self.ready = true;
    }

    /// Returns `true` if the registry has been populated.
    pub const fn is_ready(&self) -> bool {
        self.ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct TestApp {
        app_name: String,
        ready_called: Arc<AtomicBool>,
    }

    impl TestApp {
        fn new(name: &str, ready_called: Arc<AtomicBool>) -> Self {
            Self {
                app_name: name.to_string(),
                ready_called,
            }
        }
    }

    impl AppConfig for TestApp {
        fn name(&self) -> &str {
            &self.app_name
        }

        fn ready(&self) {
            self.ready_called.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = AppRegistry::new();
        let ready = Arc::new(AtomicBool::new(false));
        registry.register(Box::new(TestApp::new("myproject.myapp", ready)));

        let app = registry.get_app_config("myapp").expect("app should exist");
        assert_eq!(app.name(), "myproject.myapp");
        assert_eq!(app.label(), "myapp");
    }

    #[test]
    fn test_get_app_configs() {
        let mut registry = AppRegistry::new();
        let r1 = Arc::new(AtomicBool::new(false));
        let r2 = Arc::new(AtomicBool::new(false));
        registry.register(Box::new(TestApp::new("app1", r1)));
        registry.register(Box::new(TestApp::new("app2", r2)));

        assert_eq!(registry.get_app_configs().len(), 2);
    }

    #[test]
    fn test_populate_calls_ready() {
        let mut registry = AppRegistry::new();
        let ready = Arc::new(AtomicBool::new(false));
        registry.register(Box::new(TestApp::new("myapp", ready.clone())));

        assert!(!registry.is_ready());
        assert!(!ready.load(Ordering::SeqCst));

        registry.populate();

        assert!(registry.is_ready());
        assert!(ready.load(Ordering::SeqCst));
    }

    #[test]
    fn test_get_missing_app() {
        let registry = AppRegistry::new();
        assert!(registry.get_app_config("nonexistent").is_none());
    }

    #[test]
    #[should_panic(expected = "already registered")]
    fn test_duplicate_label_panics() {
        let mut registry = AppRegistry::new();
        let r1 = Arc::new(AtomicBool::new(false));
        let r2 = Arc::new(AtomicBool::new(false));
        registry.register(Box::new(TestApp::new("myapp", r1)));
        registry.register(Box::new(TestApp::new("myapp", r2)));
    }

    #[test]
    #[should_panic(expected = "Cannot register apps after the registry has been populated")]
    fn test_register_after_populate_panics() {
        let mut registry = AppRegistry::new();
        registry.populate();
        let ready = Arc::new(AtomicBool::new(false));
        registry.register(Box::new(TestApp::new("myapp", ready)));
    }

    #[test]
    #[should_panic(expected = "already been populated")]
    fn test_double_populate_panics() {
        let mut registry = AppRegistry::new();
        registry.populate();
        registry.populate();
    }

    #[test]
    fn test_label_derived_from_dotted_name() {
        let mut registry = AppRegistry::new();
        let ready = Arc::new(AtomicBool::new(false));
        registry.register(Box::new(TestApp::new("django_rs.contrib.auth", ready)));

        let app = registry.get_app_config("auth").expect("app should exist");
        assert_eq!(app.label(), "auth");
        assert_eq!(app.verbose_name(), "django_rs.contrib.auth");
    }

    #[test]
    fn test_default() {
        let registry = AppRegistry::default();
        assert!(!registry.is_ready());
        assert!(registry.get_app_configs().is_empty());
    }
}
