//! The `runserver` management command.
//!
//! Starts the django-rs development server on a configurable host and port.
//! This mirrors Django's `runserver` command.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Starts the development server.
///
/// By default, the server binds to `127.0.0.1:8000`. The address and port
/// can be configured via the `--host` and `--port` options.
pub struct RunserverCommand;

#[async_trait]
impl ManagementCommand for RunserverCommand {
    fn name(&self) -> &'static str {
        "runserver"
    }

    fn help(&self) -> &'static str {
        "Starts the development server"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("host")
                .long("host")
                .default_value("127.0.0.1")
                .help("Host to bind to"),
        )
        .arg(
            clap::Arg::new("port")
                .long("port")
                .default_value("8000")
                .help("Port to bind to"),
        )
        .arg(
            clap::Arg::new("noreload")
                .long("noreload")
                .action(clap::ArgAction::SetTrue)
                .help("Disable auto-reloading"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        settings: &Settings,
    ) -> Result<(), DjangoError> {
        let host = matches
            .get_one::<String>("host")
            .map_or("127.0.0.1", String::as_str);
        let port = matches
            .get_one::<String>("port")
            .map_or("8000", String::as_str);

        let addr = format!("{host}:{port}");

        tracing::info!(
            "Starting development server at http://{addr}/ (debug={})",
            settings.debug
        );

        // In a full implementation, this would start an Axum server.
        // For now, we log the intent and return.
        tracing::info!("Server would bind to {addr}");

        Ok(())
    }
}
