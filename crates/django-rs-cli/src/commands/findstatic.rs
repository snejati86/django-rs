//! The `findstatic` management command.
//!
//! Finds the absolute path of a static file by searching all configured
//! static file directories. This mirrors Django's `findstatic` command.

use std::path::PathBuf;

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Finds static files across configured directories.
///
/// Searches through `STATICFILES_DIRS` and app static directories to find
/// the file at the given path. With `--all`, shows all locations where
/// the file is found.
pub struct FindstaticCommand;

/// Searches for a static file in the given directories.
///
/// Returns all matching paths (there may be multiple if the file exists
/// in more than one static directory).
pub fn find_static_file(filename: &str, static_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();

    for dir in static_dirs {
        let candidate = dir.join(filename);
        if candidate.exists() && candidate.is_file() {
            found.push(candidate);
        }
    }

    found
}

#[async_trait]
impl ManagementCommand for FindstaticCommand {
    fn name(&self) -> &'static str {
        "findstatic"
    }

    fn help(&self) -> &'static str {
        "Find the path of a static file"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("staticfile")
                .help("Path of the static file to find")
                .required(true),
        )
        .arg(
            clap::Arg::new("all")
                .long("all")
                .action(clap::ArgAction::SetTrue)
                .help("Show all matching files, not just the first"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        settings: &Settings,
    ) -> Result<(), DjangoError> {
        let filename = matches.get_one::<String>("staticfile").ok_or_else(|| {
            DjangoError::ConfigurationError("staticfile argument is required".to_string())
        })?;
        let show_all = matches.get_flag("all");

        let found = find_static_file(filename, &settings.staticfiles_dirs);

        if found.is_empty() {
            tracing::warn!("No matching static file found for '{filename}'");
            return Err(DjangoError::NotFound(format!(
                "Static file not found: {filename}"
            )));
        }

        if show_all {
            tracing::info!("Found {} location(s) for '{filename}':", found.len());
            for path in &found {
                tracing::info!("  {}", path.display());
            }
        } else {
            tracing::info!("Found: {}", found[0].display());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_static_file_found() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("style.css");
        std::fs::write(&file_path, "body {}").unwrap();

        let dirs = vec![dir.path().to_path_buf()];
        let found = find_static_file("style.css", &dirs);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], file_path);
    }

    #[test]
    fn test_find_static_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let found = find_static_file("nonexistent.css", &dirs);
        assert!(found.is_empty());
    }

    #[test]
    fn test_find_static_file_multiple_dirs() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();

        std::fs::write(dir1.path().join("app.js"), "// js").unwrap();
        std::fs::write(dir2.path().join("app.js"), "// js v2").unwrap();

        let dirs = vec![dir1.path().to_path_buf(), dir2.path().to_path_buf()];
        let found = find_static_file("app.js", &dirs);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn test_find_static_file_empty_dirs() {
        let found = find_static_file("style.css", &[]);
        assert!(found.is_empty());
    }

    #[test]
    fn test_find_static_file_nested_path() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("css");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("main.css"), "body {}").unwrap();

        let dirs = vec![dir.path().to_path_buf()];
        let found = find_static_file("css/main.css", &dirs);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn test_command_metadata() {
        let cmd = FindstaticCommand;
        assert_eq!(cmd.name(), "findstatic");
        assert_eq!(cmd.help(), "Find the path of a static file");
    }
}
