//! Template loaders.
//!
//! Template loaders are responsible for finding and reading template source files
//! from various locations. The [`TemplateLoader`] trait defines the interface,
//! with built-in implementations for filesystem and string-based loading.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use django_rs_core::error::DjangoError;

/// Loads template source text by name.
///
/// Implementations search one or more locations for a template file and
/// return its contents as a string.
pub trait TemplateLoader: Send + Sync {
    /// Loads the template source with the given name.
    ///
    /// # Errors
    ///
    /// Returns `TemplateDoesNotExist` if the template cannot be found.
    fn load(&self, name: &str) -> Result<String, DjangoError>;
}

/// Loads templates from one or more directories on the filesystem.
///
/// Searches each configured directory in order and returns the first match.
pub struct FileSystemLoader {
    /// Directories to search for templates.
    dirs: Vec<PathBuf>,
}

impl FileSystemLoader {
    /// Creates a new `FileSystemLoader` with the given search directories.
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }
}

impl TemplateLoader for FileSystemLoader {
    fn load(&self, name: &str) -> Result<String, DjangoError> {
        for dir in &self.dirs {
            let path = dir.join(name);
            if path.exists() {
                return std::fs::read_to_string(&path).map_err(|e| {
                    DjangoError::TemplateDoesNotExist(format!(
                        "Error reading template '{}': {}",
                        path.display(),
                        e
                    ))
                });
            }
        }

        Err(DjangoError::TemplateDoesNotExist(format!(
            "Template '{name}' not found in directories: {:?}",
            self.dirs
        )))
    }
}

/// Loads templates from `<app>/templates/` directories.
///
/// This is a stub that delegates to a list of known app template directories.
pub struct AppDirectoriesLoader {
    /// App template directories to search.
    dirs: Vec<PathBuf>,
}

impl AppDirectoriesLoader {
    /// Creates a new `AppDirectoriesLoader` with the given app directories.
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }
}

impl TemplateLoader for AppDirectoriesLoader {
    fn load(&self, name: &str) -> Result<String, DjangoError> {
        for dir in &self.dirs {
            let path = dir.join("templates").join(name);
            if path.exists() {
                return std::fs::read_to_string(&path).map_err(|e| {
                    DjangoError::TemplateDoesNotExist(format!(
                        "Error reading template '{}': {}",
                        path.display(),
                        e
                    ))
                });
            }
        }

        Err(DjangoError::TemplateDoesNotExist(format!(
            "Template '{name}' not found in app directories"
        )))
    }
}

/// Loads templates from an in-memory map of name to source strings.
///
/// This is useful for testing and for applications that store templates
/// in a database or other non-filesystem location.
pub struct StringLoader {
    templates: RwLock<HashMap<String, String>>,
}

impl StringLoader {
    /// Creates a new empty `StringLoader`.
    pub fn new() -> Self {
        Self {
            templates: RwLock::new(HashMap::new()),
        }
    }

    /// Creates a `StringLoader` from a map of template names to source strings.
    pub fn from_map(templates: HashMap<String, String>) -> Self {
        Self {
            templates: RwLock::new(templates),
        }
    }

    /// Adds or replaces a template.
    pub fn add(&self, name: impl Into<String>, source: impl Into<String>) {
        self.templates
            .write()
            .unwrap()
            .insert(name.into(), source.into());
    }
}

impl Default for StringLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateLoader for StringLoader {
    fn load(&self, name: &str) -> Result<String, DjangoError> {
        self.templates
            .read()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| {
                DjangoError::TemplateDoesNotExist(format!("Template '{name}' not found in StringLoader"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_loader_basic() {
        let loader = StringLoader::new();
        loader.add("hello.html", "Hello {{ name }}!");

        let source = loader.load("hello.html").unwrap();
        assert_eq!(source, "Hello {{ name }}!");
    }

    #[test]
    fn test_string_loader_not_found() {
        let loader = StringLoader::new();
        let result = loader.load("missing.html");
        assert!(result.is_err());
    }

    #[test]
    fn test_string_loader_from_map() {
        let mut map = HashMap::new();
        map.insert("a.html".to_string(), "content A".to_string());
        map.insert("b.html".to_string(), "content B".to_string());

        let loader = StringLoader::from_map(map);
        assert_eq!(loader.load("a.html").unwrap(), "content A");
        assert_eq!(loader.load("b.html").unwrap(), "content B");
    }

    #[test]
    fn test_string_loader_overwrite() {
        let loader = StringLoader::new();
        loader.add("x.html", "version 1");
        assert_eq!(loader.load("x.html").unwrap(), "version 1");

        loader.add("x.html", "version 2");
        assert_eq!(loader.load("x.html").unwrap(), "version 2");
    }

    #[test]
    fn test_filesystem_loader_not_found() {
        let loader = FileSystemLoader::new(vec![PathBuf::from("/nonexistent/path")]);
        let result = loader.load("missing.html");
        assert!(result.is_err());
    }

    #[test]
    fn test_filesystem_loader_with_temp_dir() {
        let dir = std::env::temp_dir().join("django_rs_test_loader");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("test.html"), "Hello from file!").unwrap();

        let loader = FileSystemLoader::new(vec![dir.clone()]);
        let source = loader.load("test.html").unwrap();
        assert_eq!(source, "Hello from file!");

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }
}
