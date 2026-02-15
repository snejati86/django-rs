//! Custom template tags and filters library system.
//!
//! This module provides a public API for registering custom template filters
//! and tags, grouping them into libraries that can be loaded with `{% load mylib %}`.
//!
//! ## Overview
//!
//! - [`Library`]: A named collection of custom filters and tags.
//! - [`LibraryRegistry`]: A global registry of template libraries.
//! - Custom filters are functions: `fn(&str, &[&str]) -> String`.
//! - Custom tags are functions that receive the tag arguments and context, returning rendered output.
//!
//! ## Examples
//!
//! ```
//! use django_rs_template::library::{Library, LibraryRegistry};
//!
//! // Create a library
//! let mut lib = Library::new("mylib");
//!
//! // Register a custom filter
//! lib.register_filter("double", |value, _args| {
//!     format!("{value}{value}")
//! });
//!
//! // Register a custom simple tag
//! lib.register_simple_tag("greet", |args| {
//!     let name = args.first().map(|s| s.as_str()).unwrap_or("World");
//!     format!("Hello, {name}!")
//! });
//!
//! // Add to registry
//! let mut registry = LibraryRegistry::new();
//! registry.register(lib);
//!
//! // Look up and use
//! let lib = registry.get("mylib").unwrap();
//! let result = lib.apply_filter("double", "abc", &[]).unwrap();
//! assert_eq!(result, "abcabc");
//! ```

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use django_rs_core::error::DjangoError;

use crate::context::ContextValue;
use crate::filters::{Filter, FilterRegistry};

/// A custom filter function.
///
/// Takes a value string and optional argument strings, returns a transformed string.
pub type CustomFilterFn = fn(&str, &[&str]) -> String;

/// A custom simple tag function.
///
/// Takes argument strings and returns rendered output.
pub type SimpleTagFn = fn(&[String]) -> String;

/// A custom inclusion tag function.
///
/// Takes argument strings and returns a template name and context variables.
pub type InclusionTagFn = fn(&[String]) -> (String, HashMap<String, ContextValue>);

/// A named collection of custom filters and tags.
///
/// Libraries group related custom tags and filters together. They can be
/// loaded in templates with `{% load library_name %}`.
pub struct Library {
    /// The library name (used in `{% load name %}`).
    name: String,
    /// Custom filter functions.
    filters: HashMap<String, CustomFilterFn>,
    /// Custom simple tag functions.
    simple_tags: HashMap<String, SimpleTagFn>,
    /// Custom inclusion tag functions.
    inclusion_tags: HashMap<String, InclusionTagFn>,
}

impl Library {
    /// Creates a new empty library with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            filters: HashMap::new(),
            simple_tags: HashMap::new(),
            inclusion_tags: HashMap::new(),
        }
    }

    /// Returns the library name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Registers a custom filter function.
    ///
    /// # Arguments
    ///
    /// * `name` - The filter name (used as `{{ value|filtername }}`).
    /// * `func` - The filter function.
    pub fn register_filter(&mut self, name: impl Into<String>, func: CustomFilterFn) {
        self.filters.insert(name.into(), func);
    }

    /// Registers a custom simple tag function.
    ///
    /// Simple tags take arguments and return rendered output.
    ///
    /// # Arguments
    ///
    /// * `name` - The tag name (used as `{% tagname arg1 arg2 %}`).
    /// * `func` - The tag function.
    pub fn register_simple_tag(&mut self, name: impl Into<String>, func: SimpleTagFn) {
        self.simple_tags.insert(name.into(), func);
    }

    /// Registers a custom inclusion tag function.
    ///
    /// Inclusion tags return a template name and context variables.
    ///
    /// # Arguments
    ///
    /// * `name` - The tag name.
    /// * `func` - The inclusion tag function.
    pub fn register_inclusion_tag(&mut self, name: impl Into<String>, func: InclusionTagFn) {
        self.inclusion_tags.insert(name.into(), func);
    }

    /// Applies a custom filter by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the filter is not found.
    pub fn apply_filter(
        &self,
        name: &str,
        value: &str,
        args: &[&str],
    ) -> Result<String, DjangoError> {
        let func = self.filters.get(name).ok_or_else(|| {
            DjangoError::TemplateSyntaxError(format!(
                "Filter '{name}' not found in library '{}'",
                self.name
            ))
        })?;
        Ok(func(value, args))
    }

    /// Executes a custom simple tag by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the tag is not found.
    pub fn execute_simple_tag(
        &self,
        name: &str,
        args: &[String],
    ) -> Result<String, DjangoError> {
        let func = self.simple_tags.get(name).ok_or_else(|| {
            DjangoError::TemplateSyntaxError(format!(
                "Simple tag '{name}' not found in library '{}'",
                self.name
            ))
        })?;
        Ok(func(args))
    }

    /// Executes a custom inclusion tag by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the tag is not found.
    pub fn execute_inclusion_tag(
        &self,
        name: &str,
        args: &[String],
    ) -> Result<(String, HashMap<String, ContextValue>), DjangoError> {
        let func = self.inclusion_tags.get(name).ok_or_else(|| {
            DjangoError::TemplateSyntaxError(format!(
                "Inclusion tag '{name}' not found in library '{}'",
                self.name
            ))
        })?;
        Ok(func(args))
    }

    /// Returns all filter names in this library.
    pub fn filter_names(&self) -> Vec<&str> {
        self.filters.keys().map(|s| s.as_str()).collect()
    }

    /// Returns all simple tag names in this library.
    pub fn simple_tag_names(&self) -> Vec<&str> {
        self.simple_tags.keys().map(|s| s.as_str()).collect()
    }

    /// Returns all inclusion tag names in this library.
    pub fn inclusion_tag_names(&self) -> Vec<&str> {
        self.inclusion_tags.keys().map(|s| s.as_str()).collect()
    }

    /// Returns true if this library has a filter with the given name.
    pub fn has_filter(&self, name: &str) -> bool {
        self.filters.contains_key(name)
    }

    /// Returns true if this library has a simple tag with the given name.
    pub fn has_simple_tag(&self, name: &str) -> bool {
        self.simple_tags.contains_key(name)
    }

    /// Returns true if this library has an inclusion tag with the given name.
    pub fn has_inclusion_tag(&self, name: &str) -> bool {
        self.inclusion_tags.contains_key(name)
    }

    /// Installs all custom filters from this library into a `FilterRegistry`.
    ///
    /// This bridges the custom filter API with the engine's built-in filter system.
    pub fn install_filters(&self, registry: &mut FilterRegistry) {
        for (name, func) in &self.filters {
            registry.register(Box::new(CustomFilterAdapter {
                name: name.clone(),
                func: *func,
            }));
        }
    }
}

/// Adapter that wraps a `CustomFilterFn` as a `Filter` trait object.
struct CustomFilterAdapter {
    name: String,
    func: CustomFilterFn,
}

impl Filter for CustomFilterAdapter {
    fn name(&self) -> &'static str {
        // Leak the name string to get a 'static str.
        // This is acceptable because filters are registered once and live for
        // the duration of the program.
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn apply(
        &self,
        value: &ContextValue,
        args: &[ContextValue],
    ) -> Result<ContextValue, DjangoError> {
        let value_str = value.to_display_string();
        let arg_strings: Vec<String> = args.iter().map(|a| a.to_display_string()).collect();
        let arg_strs: Vec<&str> = arg_strings.iter().map(|s| s.as_str()).collect();
        let result = (self.func)(&value_str, &arg_strs);
        Ok(ContextValue::String(result))
    }
}

/// A registry of template libraries.
///
/// Libraries are registered by name and can be looked up when a template
/// uses `{% load libraryname %}`.
pub struct LibraryRegistry {
    libraries: HashMap<String, Arc<Library>>,
}

impl LibraryRegistry {
    /// Creates a new empty library registry.
    pub fn new() -> Self {
        Self {
            libraries: HashMap::new(),
        }
    }

    /// Registers a library. If a library with the same name exists, it is replaced.
    pub fn register(&mut self, library: Library) {
        self.libraries
            .insert(library.name.clone(), Arc::new(library));
    }

    /// Looks up a library by name.
    pub fn get(&self, name: &str) -> Option<Arc<Library>> {
        self.libraries.get(name).cloned()
    }

    /// Returns all registered library names.
    pub fn names(&self) -> Vec<&str> {
        self.libraries.keys().map(|s| s.as_str()).collect()
    }

    /// Returns the number of registered libraries.
    pub fn len(&self) -> usize {
        self.libraries.len()
    }

    /// Returns true if no libraries are registered.
    pub fn is_empty(&self) -> bool {
        self.libraries.is_empty()
    }

    /// Installs all filters from all libraries into a `FilterRegistry`.
    pub fn install_all_filters(&self, registry: &mut FilterRegistry) {
        for lib in self.libraries.values() {
            lib.install_filters(registry);
        }
    }
}

impl Default for LibraryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the global library registry.
///
/// This is a singleton that can be used to register libraries from anywhere
/// in the application, and the template engine will pick them up on
/// `{% load %}` tags.
pub fn global_registry() -> &'static RwLock<LibraryRegistry> {
    static REGISTRY: OnceLock<RwLock<LibraryRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(LibraryRegistry::new()))
}

/// Convenience function to register a library in the global registry.
pub fn register_library(library: Library) {
    let registry = global_registry();
    let mut reg = registry.write().expect("library registry lock poisoned");
    reg.register(library);
}

/// Convenience function to look up a library in the global registry.
pub fn get_library(name: &str) -> Option<Arc<Library>> {
    let registry = global_registry();
    let reg = registry.read().expect("library registry lock poisoned");
    reg.get(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Library ─────────────────────────────────────────────────────

    #[test]
    fn test_library_new() {
        let lib = Library::new("mylib");
        assert_eq!(lib.name(), "mylib");
        assert!(lib.filter_names().is_empty());
        assert!(lib.simple_tag_names().is_empty());
        assert!(lib.inclusion_tag_names().is_empty());
    }

    #[test]
    fn test_library_register_filter() {
        let mut lib = Library::new("test");
        lib.register_filter("double", |value, _args| {
            format!("{value}{value}")
        });

        assert!(lib.has_filter("double"));
        assert!(!lib.has_filter("triple"));
        assert_eq!(lib.filter_names().len(), 1);
    }

    #[test]
    fn test_library_apply_filter() {
        let mut lib = Library::new("test");
        lib.register_filter("double", |value, _args| {
            format!("{value}{value}")
        });

        let result = lib.apply_filter("double", "abc", &[]).unwrap();
        assert_eq!(result, "abcabc");
    }

    #[test]
    fn test_library_apply_filter_with_args() {
        let mut lib = Library::new("test");
        lib.register_filter("repeat", |value, args| {
            let count: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(1);
            value.repeat(count)
        });

        let result = lib.apply_filter("repeat", "ab", &["3"]).unwrap();
        assert_eq!(result, "ababab");
    }

    #[test]
    fn test_library_apply_missing_filter() {
        let lib = Library::new("test");
        let result = lib.apply_filter("nonexistent", "value", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_library_register_simple_tag() {
        let mut lib = Library::new("test");
        lib.register_simple_tag("greet", |args| {
            let name = args.first().map(|s| s.as_str()).unwrap_or("World");
            format!("Hello, {name}!")
        });

        assert!(lib.has_simple_tag("greet"));
        assert!(!lib.has_simple_tag("farewell"));
        assert_eq!(lib.simple_tag_names().len(), 1);
    }

    #[test]
    fn test_library_execute_simple_tag() {
        let mut lib = Library::new("test");
        lib.register_simple_tag("greet", |args| {
            let name = args.first().map(|s| s.as_str()).unwrap_or("World");
            format!("Hello, {name}!")
        });

        let result = lib
            .execute_simple_tag("greet", &["Alice".to_string()])
            .unwrap();
        assert_eq!(result, "Hello, Alice!");
    }

    #[test]
    fn test_library_execute_simple_tag_no_args() {
        let mut lib = Library::new("test");
        lib.register_simple_tag("greet", |args| {
            let name = args.first().map(|s| s.as_str()).unwrap_or("World");
            format!("Hello, {name}!")
        });

        let result = lib.execute_simple_tag("greet", &[]).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_library_execute_missing_simple_tag() {
        let lib = Library::new("test");
        let result = lib.execute_simple_tag("nonexistent", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_library_register_inclusion_tag() {
        let mut lib = Library::new("test");
        lib.register_inclusion_tag("sidebar", |_args| {
            let mut ctx = HashMap::new();
            ctx.insert("items".to_string(), ContextValue::List(vec![
                ContextValue::from("Home"),
                ContextValue::from("About"),
            ]));
            ("sidebar.html".to_string(), ctx)
        });

        assert!(lib.has_inclusion_tag("sidebar"));
        assert_eq!(lib.inclusion_tag_names().len(), 1);
    }

    #[test]
    fn test_library_execute_inclusion_tag() {
        let mut lib = Library::new("test");
        lib.register_inclusion_tag("sidebar", |_args| {
            let mut ctx = HashMap::new();
            ctx.insert("title".to_string(), ContextValue::from("Sidebar"));
            ("sidebar.html".to_string(), ctx)
        });

        let (template, ctx) = lib.execute_inclusion_tag("sidebar", &[]).unwrap();
        assert_eq!(template, "sidebar.html");
        assert_eq!(ctx.get("title").unwrap().to_display_string(), "Sidebar");
    }

    #[test]
    fn test_library_execute_missing_inclusion_tag() {
        let lib = Library::new("test");
        let result = lib.execute_inclusion_tag("nonexistent", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_library_multiple_filters() {
        let mut lib = Library::new("text_utils");
        lib.register_filter("reverse", |value, _| {
            value.chars().rev().collect()
        });
        lib.register_filter("upper", |value, _| {
            value.to_uppercase()
        });
        lib.register_filter("repeat", |value, args| {
            let n: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(1);
            value.repeat(n)
        });

        assert_eq!(lib.filter_names().len(), 3);
        assert_eq!(lib.apply_filter("reverse", "hello", &[]).unwrap(), "olleh");
        assert_eq!(lib.apply_filter("upper", "hello", &[]).unwrap(), "HELLO");
        assert_eq!(lib.apply_filter("repeat", "ab", &["3"]).unwrap(), "ababab");
    }

    // ── Install filters into FilterRegistry ─────────────────────────

    #[test]
    fn test_library_install_filters() {
        let mut lib = Library::new("test");
        lib.register_filter("custom_double", |value, _args| {
            format!("{value}{value}")
        });

        let mut registry = FilterRegistry::new();
        lib.install_filters(&mut registry);

        let result = registry
            .apply("custom_double", &ContextValue::from("x"), &[])
            .unwrap();
        assert_eq!(result.to_display_string(), "xx");
    }

    // ── LibraryRegistry ─────────────────────────────────────────────

    #[test]
    fn test_registry_new() {
        let registry = LibraryRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.names().is_empty());
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = LibraryRegistry::new();
        let mut lib = Library::new("mylib");
        lib.register_filter("f1", |v, _| v.to_uppercase());
        registry.register(lib);

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
        assert!(registry.names().contains(&"mylib"));

        let lib = registry.get("mylib").unwrap();
        assert_eq!(lib.name(), "mylib");
        assert!(lib.has_filter("f1"));
    }

    #[test]
    fn test_registry_get_missing() {
        let registry = LibraryRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_replace_library() {
        let mut registry = LibraryRegistry::new();

        let mut lib1 = Library::new("mylib");
        lib1.register_filter("old_filter", |v, _| v.to_string());
        registry.register(lib1);

        let mut lib2 = Library::new("mylib");
        lib2.register_filter("new_filter", |v, _| v.to_string());
        registry.register(lib2);

        assert_eq!(registry.len(), 1);
        let lib = registry.get("mylib").unwrap();
        assert!(!lib.has_filter("old_filter"));
        assert!(lib.has_filter("new_filter"));
    }

    #[test]
    fn test_registry_multiple_libraries() {
        let mut registry = LibraryRegistry::new();

        let mut lib_a = Library::new("lib_a");
        lib_a.register_filter("fa", |v, _| v.to_string());
        registry.register(lib_a);

        let mut lib_b = Library::new("lib_b");
        lib_b.register_filter("fb", |v, _| v.to_string());
        registry.register(lib_b);

        assert_eq!(registry.len(), 2);
        assert!(registry.get("lib_a").is_some());
        assert!(registry.get("lib_b").is_some());
    }

    #[test]
    fn test_registry_install_all_filters() {
        let mut registry = LibraryRegistry::new();

        let mut lib_a = Library::new("lib_a");
        lib_a.register_filter("custom_a", |v, _| format!("A:{v}"));
        registry.register(lib_a);

        let mut lib_b = Library::new("lib_b");
        lib_b.register_filter("custom_b", |v, _| format!("B:{v}"));
        registry.register(lib_b);

        let mut filter_reg = FilterRegistry::new();
        registry.install_all_filters(&mut filter_reg);

        let r1 = filter_reg
            .apply("custom_a", &ContextValue::from("x"), &[])
            .unwrap();
        assert_eq!(r1.to_display_string(), "A:x");

        let r2 = filter_reg
            .apply("custom_b", &ContextValue::from("y"), &[])
            .unwrap();
        assert_eq!(r2.to_display_string(), "B:y");
    }

    // ── Global registry ─────────────────────────────────────────────

    #[test]
    fn test_global_registry_exists() {
        let reg = global_registry();
        let _guard = reg.read().unwrap();
        // Just verify we can access it without panic
    }

    // ── CustomFilterAdapter ─────────────────────────────────────────

    #[test]
    fn test_custom_filter_adapter() {
        let adapter = CustomFilterAdapter {
            name: "test_adapter".to_string(),
            func: |value, args| {
                let suffix = args.first().unwrap_or(&"");
                format!("{value}_{suffix}")
            },
        };

        assert_eq!(adapter.name(), "test_adapter");

        let result = adapter
            .apply(
                &ContextValue::from("hello"),
                &[ContextValue::from("world")],
            )
            .unwrap();
        assert_eq!(result.to_display_string(), "hello_world");
    }

    #[test]
    fn test_custom_filter_adapter_no_args() {
        let adapter = CustomFilterAdapter {
            name: "reverse".to_string(),
            func: |value, _| value.chars().rev().collect(),
        };

        let result = adapter.apply(&ContextValue::from("abc"), &[]).unwrap();
        assert_eq!(result.to_display_string(), "cba");
    }

    // ── Integration: Library with multiple feature types ────────────

    #[test]
    fn test_library_full_integration() {
        let mut lib = Library::new("myapp");

        // Filters
        lib.register_filter("exclaim", |v, _| format!("{v}!"));
        lib.register_filter("wrap", |v, args| {
            let wrapper = args.first().unwrap_or(&"*");
            format!("{wrapper}{v}{wrapper}")
        });

        // Simple tags
        lib.register_simple_tag("current_year", |_args| "2026".to_string());
        lib.register_simple_tag("repeat_text", |args| {
            let text = args.first().map(|s| s.as_str()).unwrap_or("?");
            let count: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
            text.repeat(count)
        });

        // Inclusion tag
        lib.register_inclusion_tag("user_card", |args| {
            let name = args.first().cloned().unwrap_or_else(|| "Anonymous".to_string());
            let mut ctx = HashMap::new();
            ctx.insert("username".to_string(), ContextValue::from(name));
            ("user_card.html".to_string(), ctx)
        });

        // Verify everything works
        assert_eq!(lib.apply_filter("exclaim", "hello", &[]).unwrap(), "hello!");
        assert_eq!(lib.apply_filter("wrap", "text", &["["]).unwrap(), "[text[");

        assert_eq!(lib.execute_simple_tag("current_year", &[]).unwrap(), "2026");
        assert_eq!(
            lib.execute_simple_tag("repeat_text", &["ab".to_string(), "3".to_string()]).unwrap(),
            "ababab"
        );

        let (tpl, ctx) = lib
            .execute_inclusion_tag("user_card", &["Alice".to_string()])
            .unwrap();
        assert_eq!(tpl, "user_card.html");
        assert_eq!(ctx.get("username").unwrap().to_display_string(), "Alice");

        assert_eq!(lib.filter_names().len(), 2);
        assert_eq!(lib.simple_tag_names().len(), 2);
        assert_eq!(lib.inclusion_tag_names().len(), 1);
    }
}
