//! The `collectstatic` management command.
//!
//! Collects static files from installed apps and other configured directories
//! into a single location. This mirrors Django's `collectstatic` command.

use std::path::PathBuf;

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Collects static files into `STATIC_ROOT`.
///
/// Scans all configured static file directories and copies them to
/// the directory specified by `settings.static_root`. Uses async I/O
/// for file operations.
pub struct CollectstaticCommand;

/// Collects static files from source directories into the target directory.
///
/// Returns the number of files collected.
pub async fn collect_static_files(
    source_dirs: &[PathBuf],
    target_dir: &PathBuf,
) -> Result<usize, DjangoError> {
    // Ensure target directory exists
    tokio::fs::create_dir_all(target_dir).await.map_err(|e| {
        DjangoError::IoError(std::io::Error::new(
            e.kind(),
            format!("Failed to create STATIC_ROOT: {e}"),
        ))
    })?;

    let mut count = 0;

    for source_dir in source_dirs {
        if !source_dir.exists() {
            tracing::warn!("Static files directory does not exist: {}", source_dir.display());
            continue;
        }

        count += collect_from_dir(source_dir, target_dir, source_dir).await?;
    }

    Ok(count)
}

/// Recursively collects files from a source directory into the target directory.
async fn collect_from_dir(
    current_dir: &PathBuf,
    target_dir: &PathBuf,
    base_dir: &PathBuf,
) -> Result<usize, DjangoError> {
    let mut count = 0;
    let mut entries = tokio::fs::read_dir(current_dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        let path = entry.path();

        if file_type.is_dir() {
            count += Box::pin(collect_from_dir(&path, target_dir, base_dir)).await?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(base_dir)
                .map_err(|e| DjangoError::InternalServerError(e.to_string()))?;
            let dest = target_dir.join(relative);

            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            tokio::fs::copy(&path, &dest).await?;
            count += 1;
        }
    }

    Ok(count)
}

#[async_trait]
impl ManagementCommand for CollectstaticCommand {
    fn name(&self) -> &'static str {
        "collectstatic"
    }

    fn help(&self) -> &'static str {
        "Collect static files"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("noinput")
                .long("noinput")
                .action(clap::ArgAction::SetTrue)
                .help("Do NOT prompt the user for confirmation"),
        )
        .arg(
            clap::Arg::new("clear")
                .long("clear")
                .action(clap::ArgAction::SetTrue)
                .help("Clear the existing files before collecting"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        settings: &Settings,
    ) -> Result<(), DjangoError> {
        let clear = matches.get_flag("clear");

        let static_root = settings.static_root.as_ref().ok_or_else(|| {
            DjangoError::ImproperlyConfigured(
                "STATIC_ROOT is not set. Cannot collect static files.".to_string(),
            )
        })?;

        if clear {
            tracing::info!("Clearing existing static files in {}", static_root.display());
            if static_root.exists() {
                tokio::fs::remove_dir_all(static_root).await.map_err(|e| {
                    DjangoError::IoError(std::io::Error::new(
                        e.kind(),
                        format!("Failed to clear STATIC_ROOT: {e}"),
                    ))
                })?;
            }
        }

        let count = collect_static_files(&settings.staticfiles_dirs, static_root).await?;

        tracing::info!("Collected {count} static file(s) to {}", static_root.display());

        Ok(())
    }
}
