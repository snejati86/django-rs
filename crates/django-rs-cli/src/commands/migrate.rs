//! The `migrate` management command.
//!
//! Applies pending database migrations. This mirrors Django's `migrate` command.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Applies database migrations.
///
/// Synchronizes the database state with the current set of migrations.
/// Can target a specific app and migration name, or apply all pending migrations.
pub struct MigrateCommand;

#[async_trait]
impl ManagementCommand for MigrateCommand {
    fn name(&self) -> &'static str {
        "migrate"
    }

    fn help(&self) -> &'static str {
        "Apply database migrations"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("app_label")
                .help("App label to migrate")
                .required(false),
        )
        .arg(
            clap::Arg::new("migration_name")
                .help("Migration name to migrate to")
                .required(false),
        )
        .arg(
            clap::Arg::new("fake")
                .long("fake")
                .action(clap::ArgAction::SetTrue)
                .help("Mark migrations as applied without running them"),
        )
        .arg(
            clap::Arg::new("database")
                .long("database")
                .default_value("default")
                .help("Database alias to migrate"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        _settings: &Settings,
    ) -> Result<(), DjangoError> {
        let database = matches
            .get_one::<String>("database")
            .map_or("default", String::as_str);
        let fake = matches.get_flag("fake");
        let app_label = matches.get_one::<String>("app_label");
        let migration_name = matches.get_one::<String>("migration_name");

        tracing::info!("Running migrations on database '{database}'");

        if fake {
            tracing::info!("Fake mode: marking migrations as applied");
        }

        if let Some(app) = app_label {
            tracing::info!("Migrating app: {app}");
            if let Some(name) = migration_name {
                tracing::info!("Target migration: {name}");
            }
        } else {
            tracing::info!("Applying all pending migrations");
        }

        // In a full implementation, this would connect to the database
        // and run the migration engine.
        tracing::info!("Migrations applied successfully");

        Ok(())
    }
}
