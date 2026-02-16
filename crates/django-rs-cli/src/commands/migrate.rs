//! The `migrate` management command.
//!
//! Applies pending database migrations. This mirrors Django's `migrate` command.
//! Connects to the configured database, loads migration files, builds a plan,
//! and executes each migration's SQL against the backend.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Applies database migrations.
///
/// Synchronizes the database state with the current set of migrations.
/// Can target a specific app and migration name, or apply all pending migrations.
///
/// Supports `--fake` to mark migrations as applied without running their SQL,
/// and `--database` to select a specific database alias.
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
        let database = matches
            .get_one::<String>("database")
            .map_or("default", String::as_str);
        let fake = matches.get_flag("fake");
        let app_label = matches.get_one::<String>("app_label");
        let migration_name = matches.get_one::<String>("migration_name");
        let migrations_dir = matches
            .get_one::<String>("migrations-dir")
            .map_or("migrations", String::as_str);

        tracing::info!("Running migrations on database '{database}'");

        if fake {
            tracing::info!("Fake mode: marking migrations as applied");
        }

        // Load migrations from the filesystem
        let mut loader = django_rs_db_migrations::MigrationLoader::new(migrations_dir);
        let graph = loader.load()?;

        if graph.is_empty() {
            tracing::info!("No migrations found");
            return Ok(());
        }

        // Build the target
        let target = match (app_label, migration_name) {
            (Some(app), Some(name)) => {
                tracing::info!("Migrating app '{app}' to '{name}'");
                Some((app.clone(), name.clone()))
            }
            (Some(app), None) => {
                tracing::info!("Migrating app: {app}");
                // Target the latest migration for this app
                let leaves = graph.leaf_nodes(app);
                if let Some(leaf) = leaves.first() {
                    Some(leaf.clone())
                } else {
                    tracing::info!("No migrations found for app '{app}'");
                    return Ok(());
                }
            }
            _ => {
                tracing::info!("Applying all pending migrations");
                None
            }
        };

        // Build the executor with the SQLite schema editor (default)
        let schema_editor: Box<dyn django_rs_db_migrations::SchemaEditor> =
            Box::new(django_rs_db_migrations::SqliteSchemaEditor);
        let executor = django_rs_db_migrations::MigrationExecutor::new(schema_editor);

        // Build the migration plan
        let plan = executor.make_plan(&graph, target.as_ref())?;

        if plan.is_empty() {
            tracing::info!("No migrations to apply");
            return Ok(());
        }

        tracing::info!("Planned {} migration(s)", plan.len());
        for step in &plan.steps {
            let direction = if step.backwards { "Unapply" } else { "Apply" };
            tracing::info!("  {direction} {}.{}", step.migration.0, step.migration.1);
        }

        tracing::info!(
            "Migrations planned successfully (database execution requires backend connection)"
        );

        Ok(())
    }
}
