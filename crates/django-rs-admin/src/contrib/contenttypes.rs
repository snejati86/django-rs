//! Content types framework.
//!
//! Provides a registry of content types for models, enabling generic relations
//! and model-agnostic references. This mirrors Django's `django.contrib.contenttypes`.

use serde::{Deserialize, Serialize};

/// Represents a model's content type, identified by its app label and model name.
///
/// Content types enable generic foreign keys and model-agnostic references.
/// Every model in the framework has a corresponding `ContentType`.
///
/// # Examples
///
/// ```
/// use django_rs_admin::contrib::contenttypes::ContentType;
///
/// let ct = ContentType::new("blog", "article");
/// assert_eq!(ct.natural_key(), ("blog".to_string(), "article".to_string()));
/// assert_eq!(ct.model_class(), "blog.article");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentType {
    /// The application label (e.g., "blog", "auth").
    pub app_label: String,
    /// The model name in lowercase (e.g., "article", "user").
    pub model: String,
}

impl ContentType {
    /// Creates a new content type.
    pub fn new(app_label: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            app_label: app_label.into(),
            model: model.into(),
        }
    }

    /// Returns the natural key as a tuple of (`app_label`, `model_name`).
    ///
    /// Natural keys are used for serialization and cross-database references.
    pub fn natural_key(&self) -> (String, String) {
        (self.app_label.clone(), self.model.clone())
    }

    /// Returns the model class identifier in `"app_label.model"` format.
    pub fn model_class(&self) -> String {
        format!("{}.{}", self.app_label, self.model)
    }

    /// Returns the verbose name of this content type.
    pub fn verbose_name(&self) -> String {
        self.model.replace('_', " ")
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} | {}", self.app_label, self.model)
    }
}

/// A registry of content types.
///
/// Maintains a collection of known content types, allowing lookup by app label
/// and model name. In a full implementation, this would be backed by a database table.
#[derive(Debug, Clone, Default)]
pub struct ContentTypeRegistry {
    types: Vec<ContentType>,
}

impl ContentTypeRegistry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a content type.
    pub fn register(&mut self, ct: ContentType) {
        if !self.types.contains(&ct) {
            self.types.push(ct);
        }
    }

    /// Looks up a content type by app label and model name.
    pub fn get(&self, app_label: &str, model: &str) -> Option<&ContentType> {
        self.types
            .iter()
            .find(|ct| ct.app_label == app_label && ct.model == model)
    }

    /// Returns all registered content types.
    pub fn all(&self) -> &[ContentType] {
        &self.types
    }

    /// Returns all content types for a given app label.
    pub fn for_app(&self, app_label: &str) -> Vec<&ContentType> {
        self.types
            .iter()
            .filter(|ct| ct.app_label == app_label)
            .collect()
    }

    /// Returns the number of registered content types.
    pub fn count(&self) -> usize {
        self.types.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_new() {
        let ct = ContentType::new("blog", "article");
        assert_eq!(ct.app_label, "blog");
        assert_eq!(ct.model, "article");
    }

    #[test]
    fn test_content_type_natural_key() {
        let ct = ContentType::new("auth", "user");
        assert_eq!(
            ct.natural_key(),
            ("auth".to_string(), "user".to_string())
        );
    }

    #[test]
    fn test_content_type_model_class() {
        let ct = ContentType::new("blog", "article");
        assert_eq!(ct.model_class(), "blog.article");
    }

    #[test]
    fn test_content_type_verbose_name() {
        let ct = ContentType::new("blog", "blog_post");
        assert_eq!(ct.verbose_name(), "blog post");
    }

    #[test]
    fn test_content_type_display() {
        let ct = ContentType::new("blog", "article");
        assert_eq!(ct.to_string(), "blog | article");
    }

    #[test]
    fn test_content_type_equality() {
        let ct1 = ContentType::new("blog", "article");
        let ct2 = ContentType::new("blog", "article");
        let ct3 = ContentType::new("blog", "comment");
        assert_eq!(ct1, ct2);
        assert_ne!(ct1, ct3);
    }

    #[test]
    fn test_content_type_serialization() {
        let ct = ContentType::new("blog", "article");
        let json = serde_json::to_string(&ct).unwrap();
        assert!(json.contains("\"app_label\":\"blog\""));
        assert!(json.contains("\"model\":\"article\""));
    }

    #[test]
    fn test_content_type_deserialization() {
        let json = r#"{"app_label":"auth","model":"user"}"#;
        let ct: ContentType = serde_json::from_str(json).unwrap();
        assert_eq!(ct.app_label, "auth");
        assert_eq!(ct.model, "user");
    }

    #[test]
    fn test_registry_new() {
        let registry = ContentTypeRegistry::new();
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_registry_register() {
        let mut registry = ContentTypeRegistry::new();
        registry.register(ContentType::new("blog", "article"));
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_registry_register_duplicate() {
        let mut registry = ContentTypeRegistry::new();
        registry.register(ContentType::new("blog", "article"));
        registry.register(ContentType::new("blog", "article"));
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_registry_get() {
        let mut registry = ContentTypeRegistry::new();
        registry.register(ContentType::new("blog", "article"));
        let ct = registry.get("blog", "article").unwrap();
        assert_eq!(ct.model_class(), "blog.article");
    }

    #[test]
    fn test_registry_get_not_found() {
        let registry = ContentTypeRegistry::new();
        assert!(registry.get("blog", "article").is_none());
    }

    #[test]
    fn test_registry_for_app() {
        let mut registry = ContentTypeRegistry::new();
        registry.register(ContentType::new("blog", "article"));
        registry.register(ContentType::new("blog", "comment"));
        registry.register(ContentType::new("auth", "user"));
        let blog_types = registry.for_app("blog");
        assert_eq!(blog_types.len(), 2);
    }

    #[test]
    fn test_registry_all() {
        let mut registry = ContentTypeRegistry::new();
        registry.register(ContentType::new("blog", "article"));
        registry.register(ContentType::new("auth", "user"));
        assert_eq!(registry.all().len(), 2);
    }
}
