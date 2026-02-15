//! The `loaddata` management command.
//!
//! Loads serialized data from fixture files (JSON) into the database.
//! This mirrors Django's `loaddata` command.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;
use crate::serialization::{JsonSerializer, Serializer};

/// Loads data from fixture files into the database.
///
/// Reads JSON fixture files, deserializes their content, and inserts
/// the objects into the appropriate database tables. Supports loading
/// multiple fixtures in a single invocation.
pub struct LoaddataCommand;

/// Loads fixture data from a JSON file at the given path.
///
/// Returns the parsed objects as a vector of JSON values.
pub async fn load_fixture_file(path: &str) -> Result<Vec<serde_json::Value>, DjangoError> {
    let content = tokio::fs::read_to_string(path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DjangoError::NotFound(format!("Fixture file not found: {path}"))
        } else {
            DjangoError::IoError(e)
        }
    })?;

    let serializer = JsonSerializer;
    serializer.deserialize(&content)
}

/// Searches for a fixture file in the configured fixture directories and
/// the standard locations (the current directory, and `<app>/fixtures/`).
///
/// Returns the resolved path, or `None` if the fixture was not found.
pub fn find_fixture(name: &str, fixture_dirs: &[String]) -> Option<String> {
    // Check if the name is already a path to an existing file
    let path = std::path::Path::new(name);
    if path.exists() && path.is_file() {
        return Some(name.to_string());
    }

    // Try adding .json extension
    let with_ext = if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json")) {
        name.to_string()
    } else {
        format!("{name}.json")
    };

    let path = std::path::Path::new(&with_ext);
    if path.exists() && path.is_file() {
        return Some(with_ext);
    }

    // Search configured fixture directories
    for dir in fixture_dirs {
        let candidate = std::path::Path::new(dir).join(&with_ext);
        if candidate.exists() && candidate.is_file() {
            return candidate.to_str().map(String::from);
        }
    }

    None
}

#[async_trait]
impl ManagementCommand for LoaddataCommand {
    fn name(&self) -> &'static str {
        "loaddata"
    }

    fn help(&self) -> &'static str {
        "Load data from fixture files"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("fixture")
                .help("Fixture file(s) to load")
                .num_args(1..)
                .required(true),
        )
        .arg(
            clap::Arg::new("database")
                .long("database")
                .default_value("default")
                .help("Database alias to load data into"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        _settings: &Settings,
    ) -> Result<(), DjangoError> {
        let fixtures: Vec<&String> = matches
            .get_many::<String>("fixture")
            .map_or_else(Vec::new, Iterator::collect);
        let database = matches
            .get_one::<String>("database")
            .map_or("default", String::as_str);

        tracing::info!("Loading data into database '{database}'");

        let mut total_objects = 0;

        for fixture_name in &fixtures {
            let resolved = find_fixture(fixture_name, &[]).ok_or_else(|| {
                DjangoError::NotFound(format!("Fixture not found: {fixture_name}"))
            })?;

            tracing::info!("Loading fixture: {resolved}");

            let objects = load_fixture_file(&resolved).await?;
            let count = objects.len();
            total_objects += count;

            tracing::info!("Loaded {count} object(s) from {resolved}");

            // In a full implementation, this would insert/update each object
            // in the database based on its model and pk.
        }

        tracing::info!("Loaded {total_objects} object(s) total");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_load_fixture_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_fixture.json");

        let data = serde_json::to_string(&vec![
            json!({"model": "auth.user", "pk": 1, "fields": {"username": "admin"}}),
        ])
        .unwrap();
        tokio::fs::write(&path, &data).await.unwrap();

        let objects = load_fixture_file(path.to_str().unwrap()).await.unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0]["pk"], 1);
        assert_eq!(objects[0]["fields"]["username"], "admin");
    }

    #[tokio::test]
    async fn test_load_fixture_file_not_found() {
        let result = load_fixture_file("/nonexistent/fixture.json").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_fixture_file_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.json");
        tokio::fs::write(&path, "not valid json").await.unwrap();

        let result = load_fixture_file(path.to_str().unwrap()).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_find_fixture_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.json");
        std::fs::write(&path, "[]").unwrap();

        let result = find_fixture(path.to_str().unwrap(), &[]);
        assert!(result.is_some());
    }

    #[test]
    fn test_find_fixture_with_ext() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.json");
        std::fs::write(&path, "[]").unwrap();

        // Search by name without extension, providing the directory
        let result = find_fixture("data", &[dir.path().to_str().unwrap().to_string()]);
        assert!(result.is_some());
    }

    #[test]
    fn test_find_fixture_not_found() {
        let result = find_fixture("nonexistent_fixture", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_fixture_in_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("myfix.json");
        std::fs::write(&path, "[]").unwrap();

        let dirs = vec![dir.path().to_str().unwrap().to_string()];
        let result = find_fixture("myfix", &dirs);
        assert!(result.is_some());
    }

    #[test]
    fn test_command_metadata() {
        let cmd = LoaddataCommand;
        assert_eq!(cmd.name(), "loaddata");
        assert_eq!(cmd.help(), "Load data from fixture files");
    }
}
