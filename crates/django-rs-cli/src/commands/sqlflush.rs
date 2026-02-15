//! The `sqlflush` management command.
//!
//! Shows the SQL statements that would be executed by the `flush` command.
//! This mirrors Django's `sqlflush` command.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Displays the SQL for flushing the database.
///
/// Generates and prints the SQL statements that would delete all data
/// from all tables, without actually executing them.
pub struct SqlflushCommand;

/// Generates SQL statements for flushing the given tables.
///
/// Returns `DELETE FROM` statements for each table and a final `VACUUM`
/// statement for SQLite backends.
pub fn generate_sqlflush(table_names: &[&str], engine: &str) -> Vec<String> {
    let mut stmts: Vec<String> = table_names
        .iter()
        .map(|table| format!("DELETE FROM \"{table}\";"))
        .collect();

    // SQLite-specific: add VACUUM to reclaim space
    if engine.contains("sqlite") {
        stmts.push("VACUUM;".to_string());
    }

    stmts
}

/// Generates SQL statements for flushing using PostgreSQL TRUNCATE.
///
/// Uses `TRUNCATE ... CASCADE` for PostgreSQL which is faster and handles
/// foreign key constraints.
pub fn generate_sqlflush_postgres(table_names: &[&str]) -> Vec<String> {
    if table_names.is_empty() {
        return Vec::new();
    }

    let tables = table_names
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ");

    vec![format!("TRUNCATE {tables} CASCADE;")]
}

#[async_trait]
impl ManagementCommand for SqlflushCommand {
    fn name(&self) -> &'static str {
        "sqlflush"
    }

    fn help(&self) -> &'static str {
        "Show the SQL for flushing the database"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("database")
                .long("database")
                .default_value("default")
                .help("Database alias"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        settings: &Settings,
    ) -> Result<(), DjangoError> {
        let database = matches
            .get_one::<String>("database")
            .map_or("default", String::as_str);

        let db_settings = settings.databases.get(database).ok_or_else(|| {
            DjangoError::ConfigurationError(format!("Database '{database}' not configured"))
        })?;

        // In a full implementation, this would query the database for actual table names.
        // For now, we demonstrate the SQL generation with placeholder tables.
        let demo_tables = vec!["django_migrations", "auth_user", "auth_permission"];

        let stmts = if db_settings.engine.contains("postgresql") {
            generate_sqlflush_postgres(&demo_tables)
        } else {
            generate_sqlflush(&demo_tables, &db_settings.engine)
        };

        for stmt in &stmts {
            tracing::info!("{stmt}");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_sqlflush_sqlite() {
        let tables = vec!["auth_user", "blog_post"];
        let stmts = generate_sqlflush(&tables, "django_rs.db.backends.sqlite3");
        assert_eq!(stmts.len(), 3); // 2 DELETEs + 1 VACUUM
        assert_eq!(stmts[0], "DELETE FROM \"auth_user\";");
        assert_eq!(stmts[1], "DELETE FROM \"blog_post\";");
        assert_eq!(stmts[2], "VACUUM;");
    }

    #[test]
    fn test_generate_sqlflush_non_sqlite() {
        let tables = vec!["auth_user"];
        let stmts = generate_sqlflush(&tables, "django_rs.db.backends.postgresql");
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0], "DELETE FROM \"auth_user\";");
    }

    #[test]
    fn test_generate_sqlflush_empty() {
        let stmts = generate_sqlflush(&[], "django_rs.db.backends.sqlite3");
        assert_eq!(stmts.len(), 1); // Just VACUUM
        assert_eq!(stmts[0], "VACUUM;");
    }

    #[test]
    fn test_generate_sqlflush_postgres() {
        let tables = vec!["auth_user", "blog_post"];
        let stmts = generate_sqlflush_postgres(&tables);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0], "TRUNCATE \"auth_user\", \"blog_post\" CASCADE;");
    }

    #[test]
    fn test_generate_sqlflush_postgres_empty() {
        let stmts = generate_sqlflush_postgres(&[]);
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_command_metadata() {
        let cmd = SqlflushCommand;
        assert_eq!(cmd.name(), "sqlflush");
        assert_eq!(cmd.help(), "Show the SQL for flushing the database");
    }

    #[tokio::test]
    async fn test_sqlflush_handle() {
        let cmd = SqlflushCommand;
        let cli = clap::Command::new("test")
            .subcommand(cmd.add_arguments(clap::Command::new("sqlflush")));
        let matches = cli
            .try_get_matches_from(["test", "sqlflush"])
            .unwrap();
        let (_, sub_matches) = matches.subcommand().unwrap();

        let settings = Settings::default();
        let result = cmd.handle(sub_matches, &settings).await;
        assert!(result.is_ok());
    }
}
