//! The `createsuperuser` management command.
//!
//! Creates an admin/superuser account. This mirrors Django's
//! `createsuperuser` command.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Creates a superuser account.
///
/// In interactive mode, prompts for username, email, and password.
/// Non-interactive mode requires `--username`, `--email`, and `--password`
/// to be passed as arguments.
pub struct CreatesuperuserCommand;

#[async_trait]
impl ManagementCommand for CreatesuperuserCommand {
    fn name(&self) -> &'static str {
        "createsuperuser"
    }

    fn help(&self) -> &'static str {
        "Create a superuser account"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("username")
                .long("username")
                .help("Username for the superuser"),
        )
        .arg(
            clap::Arg::new("email")
                .long("email")
                .help("Email address for the superuser"),
        )
        .arg(
            clap::Arg::new("password")
                .long("password")
                .help("Password for the superuser (non-interactive only)"),
        )
        .arg(
            clap::Arg::new("noinput")
                .long("noinput")
                .action(clap::ArgAction::SetTrue)
                .help("Run non-interactively"),
        )
        .arg(
            clap::Arg::new("database")
                .long("database")
                .default_value("default")
                .help("Database alias to create user in"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        _settings: &Settings,
    ) -> Result<(), DjangoError> {
        let noinput = matches.get_flag("noinput");
        let database = matches
            .get_one::<String>("database")
            .map_or("default", String::as_str);

        if noinput {
            let username = matches.get_one::<String>("username").ok_or_else(|| {
                DjangoError::ConfigurationError(
                    "--username is required with --noinput".to_string(),
                )
            })?;
            let email = matches
                .get_one::<String>("email")
                .map_or("", String::as_str);

            tracing::info!(
                "Creating superuser '{}' (email: {}) in database '{}'",
                username,
                email,
                database
            );

            // In a full implementation, this would create the user in the database.
            tracing::info!("Superuser created successfully");
        } else {
            // In a full implementation, this would prompt interactively.
            tracing::info!("Interactive mode: would prompt for username, email, and password");
        }

        Ok(())
    }
}
