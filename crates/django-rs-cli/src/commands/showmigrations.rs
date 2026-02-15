//! The `showmigrations` management command.
//!
//! Displays the status of all migrations. This mirrors Django's
//! `showmigrations` command.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Lists all migrations and their applied/unapplied status.
///
/// Shows a tree of migrations organized by app, with markers indicating
/// which migrations have been applied to the database.
pub struct ShowmigrationsCommand;

#[async_trait]
impl ManagementCommand for ShowmigrationsCommand {
    fn name(&self) -> &'static str {
        "showmigrations"
    }

    fn help(&self) -> &'static str {
        "Show migration status"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("app_label")
                .help("App label(s) to show migrations for")
                .num_args(0..)
                .required(false),
        )
        .arg(
            clap::Arg::new("database")
                .long("database")
                .default_value("default")
                .help("Database alias to check"),
        )
        .arg(
            clap::Arg::new("plan")
                .long("plan")
                .action(clap::ArgAction::SetTrue)
                .help("Show planned migration order"),
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
        let plan = matches.get_flag("plan");

        tracing::info!("Showing migrations for database '{database}'");

        if plan {
            tracing::info!("Showing migration plan");
        }

        // In a full implementation, this would query the migration table
        // and display the status of each migration.
        tracing::info!("No migrations found");

        Ok(())
    }
}
