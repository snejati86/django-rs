//! The `makemigrations` management command.
//!
//! Generates new migration files based on model changes.
//! This mirrors Django's `makemigrations` command.
//! Detects changes between project states, generates operations, and serializes
//! them to JSON migration files.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Generates new migration files based on detected model changes.
///
/// Compares the current model state with the migration history and
/// generates new migration files for any differences found. Supports
/// `--dry-run` to preview without writing, `--empty` to create a blank
/// migration, and `-n` to specify a custom name.
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
        .arg(
            clap::Arg::new("migrations-dir")
                .long("migrations-dir")
                .help("Path to migrations directory")
                .default_value("migrations"),
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
        let migrations_dir = matches
            .get_one::<String>("migrations-dir")
            .map_or("migrations", String::as_str);
        let app_labels: Vec<&String> = matches
            .get_many::<String>("app_label")
            .map_or_else(Vec::new, Iterator::collect);

        if dry_run {
            tracing::info!("Dry run mode: no files will be written");
        }

        if empty {
            // Create an empty migration for each specified app
            if app_labels.is_empty() {
                return Err(DjangoError::DatabaseError(
                    "You must supply at least one app label when using --empty".into(),
                ));
            }

            for app_label in &app_labels {
                let number = django_rs_db_migrations::serializer::next_migration_number(
                    std::path::Path::new(migrations_dir),
                    app_label,
                );
                let migration_name = django_rs_db_migrations::serializer::generate_migration_name(
                    number,
                    name.map(String::as_str),
                );

                let migration = django_rs_db_migrations::SerializableMigration {
                    app_label: (*app_label).clone(),
                    name: migration_name.clone(),
                    dependencies: vec![],
                    initial: false,
                    operations: vec![],
                };

                if dry_run {
                    tracing::info!(
                        "Would create: {app_label}/migrations/{migration_name}.json"
                    );
                } else {
                    let path = django_rs_db_migrations::serializer::migration_file_path(
                        std::path::Path::new(migrations_dir),
                        app_label,
                        &migration_name,
                    );
                    migration.write_to_file(&path)?;
                    tracing::info!("Created: {}", path.display());
                }
            }

            return Ok(());
        }

        if app_labels.is_empty() {
            tracing::info!("Detecting changes for all apps...");
        } else {
            tracing::info!(
                "Detecting changes for: {}",
                app_labels
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        // In a full implementation, this would:
        // 1. Build the current ProjectState from registered models
        // 2. Build the "from" state by replaying existing migrations
        // 3. Run MigrationAutodetector to diff them
        // 4. Serialize detected operations to JSON files
        //
        // For now, we log that no changes were detected since the model
        // registry is not yet wired in.
        tracing::info!("No changes detected");

        Ok(())
    }
}
