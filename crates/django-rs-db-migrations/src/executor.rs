//! Migration execution engine.
//!
//! The [`MigrationExecutor`] takes a [`MigrationPlan`] and applies or reverts
//! migrations in the correct order. The [`MigrationRecorder`] tracks which
//! migrations have been applied in the `django_migrations` table.
//!
//! ## Async Execution
//!
//! The executor can run SQL against a real database via
//! [`MigrationExecutor::execute_against_db`], which takes a
//! [`DatabaseBackend`](django_rs_db_backends::DatabaseBackend) and executes
//! each generated SQL statement. The recorder persists applied migrations
//! to the `django_migrations` table.

use std::collections::HashSet;

use django_rs_core::DjangoError;
use django_rs_db_backends::DatabaseBackend;

use crate::autodetect::ProjectState;
use crate::migration::MigrationGraph;
use crate::operations::Operation;
use crate::schema_editor::SchemaEditor;

/// A single step in a migration plan.
///
/// Each step references a migration by its `(app_label, name)` key and
/// indicates whether the migration should be applied or reversed.
#[derive(Debug, Clone)]
pub struct MigrationStep {
    /// The migration key: `(app_label, migration_name)`.
    pub migration: (String, String),
    /// If `true`, this step reverses the migration.
    pub backwards: bool,
}

impl MigrationStep {
    /// Creates a forward migration step.
    pub fn forward(app_label: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            migration: (app_label.into(), name.into()),
            backwards: false,
        }
    }

    /// Creates a backward (reverse) migration step.
    pub fn backward(app_label: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            migration: (app_label.into(), name.into()),
            backwards: true,
        }
    }
}

/// A plan describing which migrations to apply or reverse.
///
/// The plan is an ordered list of [`MigrationStep`]s that should be executed
/// sequentially.
#[derive(Debug, Clone, Default)]
pub struct MigrationPlan {
    /// The ordered steps to execute.
    pub steps: Vec<MigrationStep>,
}

impl MigrationPlan {
    /// Creates a new empty migration plan.
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Adds a step to the plan.
    pub fn add_step(&mut self, step: MigrationStep) {
        self.steps.push(step);
    }

    /// Returns whether the plan is empty.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Returns the number of steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }
}

/// Executes migration plans using a schema editor.
///
/// The executor applies migrations in order, tracking state changes in a
/// [`ProjectState`]. It uses the [`MigrationRecorder`] to determine which
/// migrations have already been applied and produces SQL statements via
/// the [`SchemaEditor`].
pub struct MigrationExecutor {
    /// The schema editor to use for generating SQL.
    schema_editor: Box<dyn SchemaEditor>,
    /// The recorder tracking applied migrations.
    recorder: MigrationRecorder,
}

impl MigrationExecutor {
    /// Creates a new executor with the given schema editor.
    pub fn new(schema_editor: Box<dyn SchemaEditor>) -> Self {
        Self {
            schema_editor,
            recorder: MigrationRecorder::new(),
        }
    }

    /// Creates a new executor with a pre-populated recorder.
    pub fn with_recorder(
        schema_editor: Box<dyn SchemaEditor>,
        recorder: MigrationRecorder,
    ) -> Self {
        Self {
            schema_editor,
            recorder,
        }
    }

    /// Creates a migration plan to reach the target state from the current state.
    ///
    /// If `target` is `None`, applies all unapplied migrations. If `target` is
    /// `Some((app, name))`, migrates the app to that specific migration (or
    /// reverts if it's already past it).
    pub fn make_plan(
        &self,
        graph: &MigrationGraph,
        target: Option<&(String, String)>,
    ) -> Result<MigrationPlan, DjangoError> {
        let order = graph.topological_order()?;
        let applied = self.recorder.applied();
        let mut plan = MigrationPlan::new();

        match target {
            None => {
                // Apply all unapplied migrations in order
                for key in &order {
                    if !applied.contains(key) {
                        plan.add_step(MigrationStep::forward(key.0.clone(), key.1.clone()));
                    }
                }
            }
            Some(target_key) => {
                if !graph.contains(target_key) {
                    return Err(DjangoError::DatabaseError(format!(
                        "Target migration {target_key:?} not found in graph"
                    )));
                }

                // Find the target position
                let target_pos = order.iter().position(|k| k == target_key).ok_or_else(|| {
                    DjangoError::DatabaseError("Target not in topological order".into())
                })?;

                // Filter to same app
                let app_label = &target_key.0;
                let app_migrations: Vec<_> = order
                    .iter()
                    .enumerate()
                    .filter(|(_, k)| &k.0 == app_label)
                    .collect();

                let target_app_pos = app_migrations.iter().position(|(_, k)| *k == target_key);

                // Apply unapplied up to target
                for (global_pos, key) in &app_migrations {
                    if *global_pos <= target_pos && !applied.contains(key) {
                        plan.add_step(MigrationStep::forward(key.0.clone(), key.1.clone()));
                    }
                }

                // Reverse applied migrations after target
                if let Some(tap) = target_app_pos {
                    for (_global_pos, key) in app_migrations.iter().rev() {
                        let key_app_pos = app_migrations.iter().position(|(_, k)| k == key);
                        if let Some(pos) = key_app_pos {
                            if pos > tap && applied.contains(key) {
                                plan.add_step(MigrationStep::backward(
                                    key.0.clone(),
                                    key.1.clone(),
                                ));
                            }
                        }
                    }
                }
            }
        }

        Ok(plan)
    }

    /// Executes a migration plan, returning all generated SQL statements.
    ///
    /// This applies each step's operations in order, updating the project state
    /// and recording the migration as applied/unapplied.
    pub fn execute_plan(
        &mut self,
        plan: &MigrationPlan,
        operations: &std::collections::HashMap<(String, String), Vec<Box<dyn Operation>>>,
        initial_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let mut all_sql = Vec::new();
        let mut state = initial_state.clone();

        for step in &plan.steps {
            let ops = operations.get(&step.migration).ok_or_else(|| {
                DjangoError::DatabaseError(format!(
                    "Operations for migration {:?} not found",
                    step.migration
                ))
            })?;

            let from_state = state.clone();
            if step.backwards {
                // Apply operations in reverse
                for op in ops.iter().rev() {
                    let sql = op.database_backwards(
                        &step.migration.0,
                        &*self.schema_editor,
                        &from_state,
                        &state,
                    )?;
                    all_sql.extend(sql);
                }
                // Revert state (re-apply forward from initial to rebuild)
                self.recorder.unapply(&step.migration);
            } else {
                for op in ops {
                    op.state_forwards(&step.migration.0, &mut state);
                    let sql = op.database_forwards(
                        &step.migration.0,
                        &*self.schema_editor,
                        &from_state,
                        &state,
                    )?;
                    all_sql.extend(sql);
                }
                self.recorder.apply(step.migration.clone());
            }
        }

        Ok(all_sql)
    }

    /// Returns a reference to the recorder.
    pub fn recorder(&self) -> &MigrationRecorder {
        &self.recorder
    }

    /// Returns a mutable reference to the recorder.
    pub fn recorder_mut(&mut self) -> &mut MigrationRecorder {
        &mut self.recorder
    }

    /// Executes a migration plan against a real database.
    ///
    /// For each step in the plan, generates SQL via the schema editor, executes
    /// each statement against the backend, and records the migration in the
    /// `django_migrations` table.
    ///
    /// If `fake` is `true`, the migration is recorded as applied without
    /// executing the SQL statements.
    pub async fn execute_against_db(
        &mut self,
        plan: &MigrationPlan,
        operations: &std::collections::HashMap<(String, String), Vec<Box<dyn Operation>>>,
        initial_state: &ProjectState,
        backend: &dyn DatabaseBackend,
        fake: bool,
    ) -> Result<Vec<String>, DjangoError> {
        // Ensure the django_migrations table exists
        self.recorder.ensure_table(backend).await?;

        let mut all_sql = Vec::new();
        let mut state = initial_state.clone();

        for step in &plan.steps {
            let ops = operations.get(&step.migration).ok_or_else(|| {
                DjangoError::DatabaseError(format!(
                    "Operations for migration {:?} not found",
                    step.migration
                ))
            })?;

            let from_state = state.clone();
            let mut step_sql = Vec::new();

            if step.backwards {
                // Generate backwards SQL
                for op in ops.iter().rev() {
                    let sql = op.database_backwards(
                        &step.migration.0,
                        &*self.schema_editor,
                        &from_state,
                        &state,
                    )?;
                    step_sql.extend(sql);
                }
            } else {
                // Generate forwards SQL
                for op in ops {
                    op.state_forwards(&step.migration.0, &mut state);
                    let sql = op.database_forwards(
                        &step.migration.0,
                        &*self.schema_editor,
                        &from_state,
                        &state,
                    )?;
                    step_sql.extend(sql);
                }
            }

            // Execute unless faking
            if !fake {
                for sql in &step_sql {
                    // Skip SQL comment lines (e.g. SQLite recreation hints)
                    if sql.starts_with("--") {
                        continue;
                    }
                    backend.execute(sql, &[]).await?;
                }
            }
            all_sql.extend(step_sql);

            // Update in-memory state and database record
            if step.backwards {
                self.recorder.unapply(&step.migration);
                self.recorder
                    .unrecord_from_db(backend, &step.migration.0, &step.migration.1)
                    .await?;
            } else {
                self.recorder.apply(step.migration.clone());
                self.recorder
                    .record_to_db(backend, &step.migration.0, &step.migration.1)
                    .await?;
            }
        }

        Ok(all_sql)
    }
}

/// Tracks which migrations have been applied.
///
/// Operates both in-memory and against the `django_migrations` database table.
/// The in-memory set is the source of truth for plan building; the database
/// table provides persistence across runs.
#[derive(Debug, Clone, Default)]
pub struct MigrationRecorder {
    /// Set of applied migration keys.
    applied_migrations: HashSet<(String, String)>,
}

impl MigrationRecorder {
    /// Creates a new empty recorder.
    pub fn new() -> Self {
        Self {
            applied_migrations: HashSet::new(),
        }
    }

    /// Returns the SQL to create the `django_migrations` table.
    ///
    /// Uses SQLite-compatible syntax (INTEGER PRIMARY KEY AUTOINCREMENT).
    /// For PostgreSQL, use `ensure_schema_sql_pg()`.
    pub fn ensure_schema_sql() -> Vec<String> {
        vec!["CREATE TABLE IF NOT EXISTS \"django_migrations\" (\
                \"id\" BIGSERIAL PRIMARY KEY, \
                \"app\" VARCHAR(255) NOT NULL, \
                \"name\" VARCHAR(255) NOT NULL, \
                \"applied\" TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP\
            )"
        .to_string()]
    }

    /// Returns the SQLite-compatible SQL to create the `django_migrations` table.
    pub fn ensure_schema_sql_sqlite() -> &'static str {
        "CREATE TABLE IF NOT EXISTS \"django_migrations\" (\
            \"id\" INTEGER PRIMARY KEY AUTOINCREMENT, \
            \"app\" TEXT NOT NULL, \
            \"name\" TEXT NOT NULL, \
            \"applied\" TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP\
        )"
    }

    /// Records a migration as applied (in-memory only).
    pub fn apply(&mut self, key: (String, String)) {
        self.applied_migrations.insert(key);
    }

    /// Records a migration as unapplied (in-memory only).
    pub fn unapply(&mut self, key: &(String, String)) {
        self.applied_migrations.remove(key);
    }

    /// Returns the set of applied migrations.
    pub fn applied(&self) -> &HashSet<(String, String)> {
        &self.applied_migrations
    }

    /// Returns whether a specific migration has been applied.
    pub fn is_applied(&self, key: &(String, String)) -> bool {
        self.applied_migrations.contains(key)
    }

    /// Returns the SQL to record a migration as applied.
    pub fn record_applied_sql(app_label: &str, name: &str) -> String {
        format!(
            "INSERT INTO \"django_migrations\" (\"app\", \"name\", \"applied\") \
             VALUES ('{app_label}', '{name}', CURRENT_TIMESTAMP)"
        )
    }

    /// Returns the SQL to record a migration as unapplied.
    pub fn record_unapplied_sql(app_label: &str, name: &str) -> String {
        format!(
            "DELETE FROM \"django_migrations\" \
             WHERE \"app\" = '{app_label}' AND \"name\" = '{name}'"
        )
    }

    // ── Async database operations ────────────────────────────────────

    /// Ensures the `django_migrations` table exists in the database.
    ///
    /// Detects the backend type and uses the appropriate DDL syntax.
    pub async fn ensure_table(&self, backend: &dyn DatabaseBackend) -> Result<(), DjangoError> {
        let sql = match backend.vendor() {
            "sqlite" => Self::ensure_schema_sql_sqlite().to_string(),
            _ => Self::ensure_schema_sql()[0].clone(),
        };
        backend.execute(&sql, &[]).await?;
        Ok(())
    }

    /// Loads applied migrations from the database into the in-memory set.
    ///
    /// Reads all rows from `django_migrations` and populates the applied set.
    /// If the table does not exist, it is created first.
    pub async fn load_from_db(&mut self, backend: &dyn DatabaseBackend) -> Result<(), DjangoError> {
        self.ensure_table(backend).await?;

        let rows = backend
            .query("SELECT \"app\", \"name\" FROM \"django_migrations\"", &[])
            .await?;

        self.applied_migrations.clear();
        for row in &rows {
            let app: String = row
                .get("app")
                .map_err(|_| DjangoError::DatabaseError("Missing 'app' column".into()))?;
            let name: String = row
                .get("name")
                .map_err(|_| DjangoError::DatabaseError("Missing 'name' column".into()))?;
            self.applied_migrations.insert((app, name));
        }

        Ok(())
    }

    /// Records a migration as applied in the database.
    pub async fn record_to_db(
        &self,
        backend: &dyn DatabaseBackend,
        app_label: &str,
        name: &str,
    ) -> Result<(), DjangoError> {
        let sql = Self::record_applied_sql(app_label, name);
        backend.execute(&sql, &[]).await?;
        Ok(())
    }

    /// Removes a migration record from the database.
    pub async fn unrecord_from_db(
        &self,
        backend: &dyn DatabaseBackend,
        app_label: &str,
        name: &str,
    ) -> Result<(), DjangoError> {
        let sql = Self::record_unapplied_sql(app_label, name);
        backend.execute(&sql, &[]).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autodetect::{MigrationFieldDef, ModelOptions};
    use crate::migration::MigrationGraph;
    use crate::operations::{AddField, CreateModel, RunSQL};
    use crate::schema_editor::PostgresSchemaEditor;
    use django_rs_db::fields::FieldType;

    // ── MigrationStep tests ─────────────────────────────────────────

    #[test]
    fn test_step_forward() {
        let step = MigrationStep::forward("blog", "0001_initial");
        assert_eq!(step.migration, ("blog".into(), "0001_initial".into()));
        assert!(!step.backwards);
    }

    #[test]
    fn test_step_backward() {
        let step = MigrationStep::backward("blog", "0001_initial");
        assert!(step.backwards);
    }

    // ── MigrationPlan tests ─────────────────────────────────────────

    #[test]
    fn test_plan_new() {
        let plan = MigrationPlan::new();
        assert!(plan.is_empty());
        assert_eq!(plan.len(), 0);
    }

    #[test]
    fn test_plan_add_step() {
        let mut plan = MigrationPlan::new();
        plan.add_step(MigrationStep::forward("blog", "0001"));
        assert_eq!(plan.len(), 1);
        assert!(!plan.is_empty());
    }

    #[test]
    fn test_plan_default() {
        let plan = MigrationPlan::default();
        assert!(plan.is_empty());
    }

    // ── MigrationRecorder tests ─────────────────────────────────────

    #[test]
    fn test_recorder_new() {
        let recorder = MigrationRecorder::new();
        assert!(recorder.applied().is_empty());
    }

    #[test]
    fn test_recorder_apply() {
        let mut recorder = MigrationRecorder::new();
        recorder.apply(("blog".into(), "0001".into()));
        assert!(recorder.is_applied(&("blog".into(), "0001".into())));
    }

    #[test]
    fn test_recorder_unapply() {
        let mut recorder = MigrationRecorder::new();
        recorder.apply(("blog".into(), "0001".into()));
        recorder.unapply(&("blog".into(), "0001".into()));
        assert!(!recorder.is_applied(&("blog".into(), "0001".into())));
    }

    #[test]
    fn test_recorder_ensure_schema_sql() {
        let sqls = MigrationRecorder::ensure_schema_sql();
        assert_eq!(sqls.len(), 1);
        assert!(sqls[0].contains("CREATE TABLE"));
        assert!(sqls[0].contains("django_migrations"));
    }

    #[test]
    fn test_recorder_record_applied_sql() {
        let sql = MigrationRecorder::record_applied_sql("blog", "0001_initial");
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("blog"));
        assert!(sql.contains("0001_initial"));
    }

    #[test]
    fn test_recorder_record_unapplied_sql() {
        let sql = MigrationRecorder::record_unapplied_sql("blog", "0001_initial");
        assert!(sql.contains("DELETE FROM"));
        assert!(sql.contains("blog"));
        assert!(sql.contains("0001_initial"));
    }

    #[test]
    fn test_recorder_default() {
        let recorder = MigrationRecorder::default();
        assert!(recorder.applied().is_empty());
    }

    // ── MigrationExecutor tests ─────────────────────────────────────

    #[test]
    fn test_executor_make_plan_all_unapplied() {
        let mut graph = MigrationGraph::new();
        graph.add_node("blog", "0001", true);
        graph.add_node("blog", "0002", false);
        graph
            .add_dependency(
                ("blog".into(), "0002".into()),
                ("blog".into(), "0001".into()),
            )
            .unwrap();

        let executor = MigrationExecutor::new(Box::new(PostgresSchemaEditor));
        let plan = executor.make_plan(&graph, None).unwrap();
        assert_eq!(plan.len(), 2);
        assert!(!plan.steps[0].backwards);
        assert!(!plan.steps[1].backwards);
    }

    #[test]
    fn test_executor_make_plan_partially_applied() {
        let mut graph = MigrationGraph::new();
        graph.add_node("blog", "0001", true);
        graph.add_node("blog", "0002", false);
        graph
            .add_dependency(
                ("blog".into(), "0002".into()),
                ("blog".into(), "0001".into()),
            )
            .unwrap();

        let mut recorder = MigrationRecorder::new();
        recorder.apply(("blog".into(), "0001".into()));

        let executor = MigrationExecutor::with_recorder(Box::new(PostgresSchemaEditor), recorder);
        let plan = executor.make_plan(&graph, None).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan.steps[0].migration.1, "0002");
    }

    #[test]
    fn test_executor_make_plan_all_applied() {
        let mut graph = MigrationGraph::new();
        graph.add_node("blog", "0001", true);

        let mut recorder = MigrationRecorder::new();
        recorder.apply(("blog".into(), "0001".into()));

        let executor = MigrationExecutor::with_recorder(Box::new(PostgresSchemaEditor), recorder);
        let plan = executor.make_plan(&graph, None).unwrap();
        assert!(plan.is_empty());
    }

    #[test]
    fn test_executor_make_plan_target() {
        let mut graph = MigrationGraph::new();
        graph.add_node("blog", "0001", true);
        graph.add_node("blog", "0002", false);
        graph.add_node("blog", "0003", false);
        graph
            .add_dependency(
                ("blog".into(), "0002".into()),
                ("blog".into(), "0001".into()),
            )
            .unwrap();
        graph
            .add_dependency(
                ("blog".into(), "0003".into()),
                ("blog".into(), "0002".into()),
            )
            .unwrap();

        let executor = MigrationExecutor::new(Box::new(PostgresSchemaEditor));
        let target = ("blog".into(), "0002".into());
        let plan = executor.make_plan(&graph, Some(&target)).unwrap();
        // Should apply 0001 and 0002 (not 0003)
        assert_eq!(plan.len(), 2);
    }

    #[test]
    fn test_executor_make_plan_target_not_found() {
        let graph = MigrationGraph::new();
        let executor = MigrationExecutor::new(Box::new(PostgresSchemaEditor));
        let target = ("blog".into(), "0099".into());
        let result = executor.make_plan(&graph, Some(&target));
        assert!(result.is_err());
    }

    #[test]
    fn test_executor_execute_plan_create_model() {
        let mut executor = MigrationExecutor::new(Box::new(PostgresSchemaEditor));

        let mut plan = MigrationPlan::new();
        plan.add_step(MigrationStep::forward("blog", "0001"));

        let ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
            name: "post".into(),
            fields: vec![
                MigrationFieldDef::new("id", FieldType::BigAutoField).primary_key(),
                MigrationFieldDef::new("title", FieldType::CharField).max_length(200),
            ],
            options: ModelOptions::default(),
        })];

        let mut operations = std::collections::HashMap::new();
        operations.insert(("blog".into(), "0001".into()), ops);

        let state = ProjectState::new();
        let sqls = executor.execute_plan(&plan, &operations, &state).unwrap();
        assert!(!sqls.is_empty());
        assert!(sqls[0].contains("CREATE TABLE"));
        assert!(executor
            .recorder()
            .is_applied(&("blog".into(), "0001".into())));
    }

    #[test]
    fn test_executor_execute_plan_multiple_steps() {
        let mut executor = MigrationExecutor::new(Box::new(PostgresSchemaEditor));

        let mut plan = MigrationPlan::new();
        plan.add_step(MigrationStep::forward("blog", "0001"));
        plan.add_step(MigrationStep::forward("blog", "0002"));

        let ops1: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
            name: "post".into(),
            fields: vec![MigrationFieldDef::new("id", FieldType::BigAutoField).primary_key()],
            options: ModelOptions::default(),
        })];

        let ops2: Vec<Box<dyn Operation>> = vec![Box::new(AddField {
            model_name: "post".into(),
            field: MigrationFieldDef::new("title", FieldType::CharField).max_length(200),
        })];

        let mut operations = std::collections::HashMap::new();
        operations.insert(("blog".into(), "0001".into()), ops1);
        operations.insert(("blog".into(), "0002".into()), ops2);

        let state = ProjectState::new();
        let sqls = executor.execute_plan(&plan, &operations, &state).unwrap();
        assert!(sqls.len() >= 2);
    }

    #[test]
    fn test_executor_execute_plan_backwards() {
        let mut executor = MigrationExecutor::new(Box::new(PostgresSchemaEditor));

        // First apply
        let mut plan = MigrationPlan::new();
        plan.add_step(MigrationStep::forward("blog", "0001"));

        let ops: Vec<Box<dyn Operation>> = vec![Box::new(RunSQL {
            sql_forwards: "CREATE TABLE test (id INT)".into(),
            sql_backwards: "DROP TABLE test".into(),
        })];

        let mut operations = std::collections::HashMap::new();
        operations.insert(("blog".into(), "0001".into()), ops);

        let state = ProjectState::new();
        executor.execute_plan(&plan, &operations, &state).unwrap();
        assert!(executor
            .recorder()
            .is_applied(&("blog".into(), "0001".into())));

        // Now reverse
        let mut plan2 = MigrationPlan::new();
        plan2.add_step(MigrationStep::backward("blog", "0001"));

        let ops2: Vec<Box<dyn Operation>> = vec![Box::new(RunSQL {
            sql_forwards: "CREATE TABLE test (id INT)".into(),
            sql_backwards: "DROP TABLE test".into(),
        })];
        let mut operations2 = std::collections::HashMap::new();
        operations2.insert(("blog".into(), "0001".into()), ops2);

        let sqls = executor.execute_plan(&plan2, &operations2, &state).unwrap();
        assert!(sqls.contains(&"DROP TABLE test".to_string()));
        assert!(!executor
            .recorder()
            .is_applied(&("blog".into(), "0001".into())));
    }

    #[test]
    fn test_executor_execute_plan_missing_ops() {
        let mut executor = MigrationExecutor::new(Box::new(PostgresSchemaEditor));
        let mut plan = MigrationPlan::new();
        plan.add_step(MigrationStep::forward("blog", "0001"));
        let operations = std::collections::HashMap::new();
        let state = ProjectState::new();
        let result = executor.execute_plan(&plan, &operations, &state);
        assert!(result.is_err());
    }
}
