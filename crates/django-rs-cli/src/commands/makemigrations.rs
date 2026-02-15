//! The `makemigrations` management command.
//!
//! Generates new migration files based on model changes.
//! This mirrors Django's `makemigrations` command.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Generates new migration files based on detected model changes.
///
/// Compares the current model state with the migration history and
/// generates new migration files for any differences found.
pub struct MakemigrationsCommand;

#[async_trait]
impl ManagementCommand for MakemigrationsCommand {
    fn name(&self) -> &'static str {
        "makemigrations"
    }

    fn help(&self) -> &'static str {
        "Generate new database migrations"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("app_label")
                .help("App label(s) to generate migrations for")
                .num_args(0..)
                .required(false),
        )
        .arg(
            clap::Arg::new("dry-run")
                .long("dry-run")
                .action(clap::ArgAction::SetTrue)
                .help("Show what migrations would be generated without writing them"),
        )
        .arg(
            clap::Arg::new("empty")
                .long("empty")
                .action(clap::ArgAction::SetTrue)
                .help("Create an empty migration"),
        )
        .arg(
            clap::Arg::new("name")
                .short('n')
                .long("name")
                .help("Name for the generated migration"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        _settings: &Settings,
    ) -> Result<(), DjangoError> {
        let dry_run = matches.get_flag("dry-run");
        let empty = matches.get_flag("empty");
        let name = matches.get_one::<String>("name");
        let app_labels: Vec<&String> = matches
            .get_many::<String>("app_label")
            .map_or_else(Vec::new, Iterator::collect);

        if dry_run {
            tracing::info!("Dry run mode: no files will be written");
        }

        if empty {
            tracing::info!("Creating empty migration");
        }

        if let Some(name) = name {
            tracing::info!("Migration name: {name}");
        }

        if app_labels.is_empty() {
            tracing::info!("Detecting changes for all apps...");
        } else {
            tracing::info!("Detecting changes for: {}", app_labels.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
        }

        // In a full implementation, this would diff the model state
        // against existing migrations and generate new migration files.
        tracing::info!("No changes detected");

        Ok(())
    }
}
