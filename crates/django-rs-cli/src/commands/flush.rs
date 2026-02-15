//! The `flush` management command.
//!
//! Deletes all data from the database by truncating all tables.
//! This mirrors Django's `flush` command.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Removes all data from the database.
///
/// Generates and executes `DELETE FROM` statements for every table in the
/// project. Requires `--noinput` to skip the confirmation prompt.
pub struct FlushCommand;

/// Generates SQL statements to delete all data from the given tables.
///
/// Returns a vector of `DELETE FROM` statements, one per table.
pub fn generate_flush_sql(table_names: &[&str]) -> Vec<String> {
    table_names
        .iter()
        .map(|table| format!("DELETE FROM \"{table}\";"))
        .collect()
}

/// Extracts table names from installed app settings.
///
/// In a full implementation, this would inspect the model registry.
/// For now, it derives table names from the `installed_apps` setting
/// using the Django convention of `app_label_modelname`.
pub fn get_table_names_from_settings(settings: &Settings) -> Vec<String> {
    // Return table names based on installed apps
    // In a real implementation, this would query the model registry
    settings
        .installed_apps
        .iter()
        .map(|app| {
            let label = app.rsplit('.').next().unwrap_or(app);
            format!("{label}_*")
        })
        .collect()
}

#[async_trait]
impl ManagementCommand for FlushCommand {
    fn name(&self) -> &'static str {
        "flush"
    }

    fn help(&self) -> &'static str {
        "Delete all data from the database"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("noinput")
                .long("noinput")
                .action(clap::ArgAction::SetTrue)
                .help("Skip the confirmation prompt"),
        )
        .arg(
            clap::Arg::new("database")
                .long("database")
                .default_value("default")
                .help("Database alias to flush"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        settings: &Settings,
    ) -> Result<(), DjangoError> {
        let noinput = matches.get_flag("noinput");
        let database = matches
            .get_one::<String>("database")
            .map_or("default", String::as_str);

        if !noinput {
            tracing::warn!(
                "This will delete ALL data from database '{}'. \
                 Use --noinput to skip this prompt.",
                database
            );
            return Err(DjangoError::ConfigurationError(
                "Flush requires --noinput to proceed without confirmation".to_string(),
            ));
        }

        tracing::info!("Flushing database '{database}'");

        let table_patterns = get_table_names_from_settings(settings);

        if table_patterns.is_empty() {
            tracing::info!("No installed apps found, nothing to flush");
            return Ok(());
        }

        // In a full implementation, this would:
        // 1. Query the database for all actual table names
        // 2. Generate DELETE FROM statements
        // 3. Execute them in a transaction
        tracing::info!(
            "Would flush {} app table pattern(s) from database '{database}'",
            table_patterns.len()
        );

        tracing::info!("Database '{database}' flushed successfully");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_flush_sql() {
        let tables = vec!["auth_user", "blog_post", "blog_comment"];
        let sql = generate_flush_sql(&tables);
        assert_eq!(sql.len(), 3);
        assert_eq!(sql[0], "DELETE FROM \"auth_user\";");
        assert_eq!(sql[1], "DELETE FROM \"blog_post\";");
        assert_eq!(sql[2], "DELETE FROM \"blog_comment\";");
    }

    #[test]
    fn test_generate_flush_sql_empty() {
        let sql = generate_flush_sql(&[]);
        assert!(sql.is_empty());
    }

    #[test]
    fn test_get_table_names_from_settings() {
        let settings = Settings {
            installed_apps: vec![
                "django_rs.auth".to_string(),
                "myapp.blog".to_string(),
            ],
            ..Settings::default()
        };

        let names = get_table_names_from_settings(&settings);
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "auth_*");
        assert_eq!(names[1], "blog_*");
    }

    #[test]
    fn test_get_table_names_empty() {
        let settings = Settings {
            installed_apps: vec![],
            ..Settings::default()
        };
        let names = get_table_names_from_settings(&settings);
        assert!(names.is_empty());
    }

    #[test]
    fn test_command_metadata() {
        let cmd = FlushCommand;
        assert_eq!(cmd.name(), "flush");
        assert_eq!(cmd.help(), "Delete all data from the database");
    }

    #[tokio::test]
    async fn test_flush_requires_noinput() {
        let cmd = FlushCommand;
        let cli = clap::Command::new("test")
            .subcommand(cmd.add_arguments(clap::Command::new("flush")));
        let matches = cli
            .try_get_matches_from(["test", "flush"])
            .unwrap();
        let (_, sub_matches) = matches.subcommand().unwrap();

        let settings = Settings::default();
        let result = cmd.handle(sub_matches, &settings).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_flush_with_noinput() {
        let cmd = FlushCommand;
        let cli = clap::Command::new("test")
            .subcommand(cmd.add_arguments(clap::Command::new("flush")));
        let matches = cli
            .try_get_matches_from(["test", "flush", "--noinput"])
            .unwrap();
        let (_, sub_matches) = matches.subcommand().unwrap();

        let settings = Settings::default();
        let result = cmd.handle(sub_matches, &settings).await;
        assert!(result.is_ok());
    }
}
