//! Migration loader for discovering migrations from the filesystem.
//!
//! The [`MigrationLoader`] scans a directory structure to find migration files
//! and builds a [`MigrationGraph`] from them. This mirrors Django's
//! `MigrationLoader`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use django_rs_core::DjangoError;

use crate::migration::{Migration, MigrationGraph};

/// Metadata about a discovered migration file.
///
/// The loader discovers migration files but does not execute them directly.
/// Instead it returns metadata that the executor uses to build a plan.
#[derive(Debug, Clone)]
pub struct MigrationFileInfo {
    /// The app label.
    pub app_label: String,
    /// The migration name (without extension).
    pub name: String,
    /// The path to the migration file.
    pub path: PathBuf,
    /// Dependencies declared in the migration file.
    pub dependencies: Vec<(String, String)>,
    /// Whether this is an initial migration.
    pub initial: bool,
}

/// Discovers and loads migrations from the filesystem.
///
/// The loader expects a directory structure like:
/// ```text
/// migrations_dir/
///   app_label/
///     0001_initial.json
///     0002_add_field.json
/// ```
///
/// Each migration file is a JSON file containing migration metadata.
pub struct MigrationLoader {
    /// The base directory containing app migration directories.
    migrations_dir: PathBuf,
    /// Discovered migrations keyed by `(app_label, name)`.
    migrations: HashMap<(String, String), MigrationFileInfo>,
}

impl MigrationLoader {
    /// Creates a new loader for the given migrations directory.
    pub fn new(migrations_dir: impl Into<PathBuf>) -> Self {
        Self {
            migrations_dir: migrations_dir.into(),
            migrations: HashMap::new(),
        }
    }

    /// Scans the filesystem for migration files and builds a graph.
    ///
    /// Returns the migration graph with all discovered migrations added as
    /// nodes and their dependencies as edges.
    pub fn load(&mut self) -> Result<MigrationGraph, DjangoError> {
        self.discover()?;
        self.build_graph()
    }

    /// Discovers migration files from the directory structure.
    fn discover(&mut self) -> Result<(), DjangoError> {
        self.migrations.clear();

        let dir = &self.migrations_dir;
        if !dir.exists() {
            return Ok(());
        }

        let entries = std::fs::read_dir(dir).map_err(|e| {
            DjangoError::DatabaseError(format!("Cannot read migrations directory: {e}"))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                DjangoError::DatabaseError(format!("Cannot read directory entry: {e}"))
            })?;
            let path = entry.path();
            if path.is_dir() {
                let app_label = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                if !app_label.is_empty() {
                    self.discover_app(&app_label, &path)?;
                }
            }
        }

        Ok(())
    }

    /// Discovers migration files for a single app.
    fn discover_app(&mut self, app_label: &str, app_dir: &Path) -> Result<(), DjangoError> {
        let entries = std::fs::read_dir(app_dir)
            .map_err(|e| DjangoError::DatabaseError(format!("Cannot read app directory: {e}")))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                DjangoError::DatabaseError(format!("Cannot read directory entry: {e}"))
            })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let name = path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    let info = self.parse_migration_file(app_label, &name, &path)?;
                    let key = (app_label.to_string(), name);
                    self.migrations.insert(key, info);
                }
            }
        }

        Ok(())
    }

    /// Parses a migration JSON file to extract metadata.
    #[allow(clippy::unused_self)]
    fn parse_migration_file(
        &self,
        app_label: &str,
        name: &str,
        path: &Path,
    ) -> Result<MigrationFileInfo, DjangoError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| DjangoError::DatabaseError(format!("Cannot read migration file: {e}")))?;

        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| DjangoError::DatabaseError(format!("Invalid migration JSON: {e}")))?;

        let initial = json
            .get("initial")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let dependencies = json
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|dep| {
                        let dep_arr = dep.as_array()?;
                        if dep_arr.len() == 2 {
                            Some((
                                dep_arr[0].as_str()?.to_string(),
                                dep_arr[1].as_str()?.to_string(),
                            ))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(MigrationFileInfo {
            app_label: app_label.to_string(),
            name: name.to_string(),
            path: path.to_path_buf(),
            dependencies,
            initial,
        })
    }

    /// Builds a migration graph from discovered migrations.
    fn build_graph(&self) -> Result<MigrationGraph, DjangoError> {
        let mut graph = MigrationGraph::new();

        // Add all nodes first
        for (key, info) in &self.migrations {
            graph.add_node(&key.0, &key.1, info.initial);
        }

        // Add dependency edges
        for (key, info) in &self.migrations {
            for dep in &info.dependencies {
                graph.add_dependency(key.clone(), dep.clone())?;
            }
        }

        graph.validate()?;
        Ok(graph)
    }

    /// Returns the discovered migrations.
    pub fn migrations(&self) -> &HashMap<(String, String), MigrationFileInfo> {
        &self.migrations
    }

    /// Returns the migrations directory.
    pub fn migrations_dir(&self) -> &Path {
        &self.migrations_dir
    }

    /// Creates a `MigrationGraph` from a list of in-memory migrations.
    ///
    /// This is useful for testing and for programmatic migration definitions
    /// that don't come from the filesystem.
    pub fn graph_from_migrations(migrations: &[&Migration]) -> Result<MigrationGraph, DjangoError> {
        let mut graph = MigrationGraph::new();

        for m in migrations {
            graph.add_node(&m.app_label, &m.name, m.initial);
        }

        for m in migrations {
            for dep in &m.dependencies {
                graph.add_dependency(m.key(), dep.clone())?;
            }
        }

        graph.validate()?;
        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn create_temp_dir() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "django_rs_test_migrations_{}_{}",
            std::process::id(),
            id
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    // ── MigrationLoader basic tests ─────────────────────────────────

    #[test]
    fn test_loader_new() {
        let loader = MigrationLoader::new("/tmp/test");
        assert_eq!(loader.migrations_dir(), Path::new("/tmp/test"));
        assert!(loader.migrations().is_empty());
    }

    #[test]
    fn test_loader_nonexistent_dir() {
        let mut loader = MigrationLoader::new("/nonexistent/path/to/migrations");
        let graph = loader.load().unwrap();
        assert!(graph.is_empty());
    }

    #[test]
    fn test_loader_empty_dir() {
        let dir = create_temp_dir();
        let mut loader = MigrationLoader::new(&dir);
        let graph = loader.load().unwrap();
        assert!(graph.is_empty());
        cleanup(&dir);
    }

    #[test]
    fn test_loader_discover_single_migration() {
        let dir = create_temp_dir();
        let app_dir = dir.join("blog");
        fs::create_dir_all(&app_dir).unwrap();
        fs::write(
            app_dir.join("0001_initial.json"),
            r#"{"initial": true, "dependencies": [], "operations": []}"#,
        )
        .unwrap();

        let mut loader = MigrationLoader::new(&dir);
        let graph = loader.load().unwrap();
        assert_eq!(graph.len(), 1);
        assert!(graph.contains(&("blog".into(), "0001_initial".into())));
        cleanup(&dir);
    }

    #[test]
    fn test_loader_discover_multiple_migrations() {
        let dir = create_temp_dir();
        let app_dir = dir.join("blog");
        fs::create_dir_all(&app_dir).unwrap();
        fs::write(
            app_dir.join("0001_initial.json"),
            r#"{"initial": true, "dependencies": [], "operations": []}"#,
        )
        .unwrap();
        fs::write(
            app_dir.join("0002_add_title.json"),
            r#"{"initial": false, "dependencies": [["blog", "0001_initial"]], "operations": []}"#,
        )
        .unwrap();

        let mut loader = MigrationLoader::new(&dir);
        let graph = loader.load().unwrap();
        assert_eq!(graph.len(), 2);

        let order = graph.topological_order().unwrap();
        let pos_1 = order.iter().position(|k| k.1 == "0001_initial").unwrap();
        let pos_2 = order.iter().position(|k| k.1 == "0002_add_title").unwrap();
        assert!(pos_1 < pos_2);
        cleanup(&dir);
    }

    #[test]
    fn test_loader_discover_multiple_apps() {
        let dir = create_temp_dir();
        let blog_dir = dir.join("blog");
        let auth_dir = dir.join("auth");
        fs::create_dir_all(&blog_dir).unwrap();
        fs::create_dir_all(&auth_dir).unwrap();
        fs::write(
            blog_dir.join("0001_initial.json"),
            r#"{"initial": true, "dependencies": [], "operations": []}"#,
        )
        .unwrap();
        fs::write(
            auth_dir.join("0001_initial.json"),
            r#"{"initial": true, "dependencies": [], "operations": []}"#,
        )
        .unwrap();

        let mut loader = MigrationLoader::new(&dir);
        let graph = loader.load().unwrap();
        assert_eq!(graph.len(), 2);
        cleanup(&dir);
    }

    #[test]
    fn test_loader_cross_app_dependency() {
        let dir = create_temp_dir();
        let auth_dir = dir.join("auth");
        let blog_dir = dir.join("blog");
        fs::create_dir_all(&auth_dir).unwrap();
        fs::create_dir_all(&blog_dir).unwrap();
        fs::write(
            auth_dir.join("0001_initial.json"),
            r#"{"initial": true, "dependencies": [], "operations": []}"#,
        )
        .unwrap();
        fs::write(
            blog_dir.join("0001_initial.json"),
            r#"{"initial": true, "dependencies": [["auth", "0001_initial"]], "operations": []}"#,
        )
        .unwrap();

        let mut loader = MigrationLoader::new(&dir);
        let graph = loader.load().unwrap();
        let order = graph.topological_order().unwrap();
        let pos_auth = order.iter().position(|k| k.0 == "auth").unwrap();
        let pos_blog = order.iter().position(|k| k.0 == "blog").unwrap();
        assert!(pos_auth < pos_blog);
        cleanup(&dir);
    }

    // ── graph_from_migrations ───────────────────────────────────────

    #[test]
    fn test_graph_from_migrations_empty() {
        let graph = MigrationLoader::graph_from_migrations(&[]).unwrap();
        assert!(graph.is_empty());
    }

    #[test]
    fn test_graph_from_migrations_single() {
        let m = Migration::new("blog", "0001_initial").initial();
        let graph = MigrationLoader::graph_from_migrations(&[&m]).unwrap();
        assert_eq!(graph.len(), 1);
    }

    #[test]
    fn test_graph_from_migrations_chain() {
        let m1 = Migration::new("blog", "0001_initial").initial();
        let m2 = Migration::new("blog", "0002_add_title").depends_on("blog", "0001_initial");
        let graph = MigrationLoader::graph_from_migrations(&[&m1, &m2]).unwrap();
        assert_eq!(graph.len(), 2);
        let order = graph.topological_order().unwrap();
        let pos_1 = order.iter().position(|k| k.1 == "0001_initial").unwrap();
        let pos_2 = order.iter().position(|k| k.1 == "0002_add_title").unwrap();
        assert!(pos_1 < pos_2);
    }

    // ── MigrationFileInfo ───────────────────────────────────────────

    #[test]
    fn test_migration_file_info() {
        let info = MigrationFileInfo {
            app_label: "blog".into(),
            name: "0001_initial".into(),
            path: PathBuf::from("/tmp/blog/0001_initial.json"),
            dependencies: vec![("auth".into(), "0001_initial".into())],
            initial: true,
        };
        assert_eq!(info.app_label, "blog");
        assert!(info.initial);
        assert_eq!(info.dependencies.len(), 1);
    }
}
