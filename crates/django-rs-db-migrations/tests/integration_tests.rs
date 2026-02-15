//! Integration tests for the migration execution pipeline.
//!
//! These tests create SQLite in-memory databases and execute real DDL against
//! them, verifying that:
//! - Tables are created/dropped correctly
//! - Columns are added/removed
//! - The `django_migrations` table tracks applied migrations
//! - Forward and backward execution works
//! - Fake migrations record without executing
//! - Migration file serialization round-trips correctly

use std::collections::HashMap;

use django_rs_db::fields::FieldType;
use django_rs_db::value::Value;
use django_rs_db_backends::{DatabaseBackend, SqliteBackend};
use django_rs_db_migrations::autodetect::{MigrationFieldDef, ModelOptions, ProjectState};
use django_rs_db_migrations::executor::{MigrationExecutor, MigrationPlan, MigrationRecorder, MigrationStep};
use django_rs_db_migrations::operations::{AddField, CreateModel, DeleteModel, RemoveField, RunSQL};
use django_rs_db_migrations::schema_editor::SqliteSchemaEditor;
use django_rs_db_migrations::serializer::{
    generate_migration_name, migration_file_path, next_migration_number, SerializableMigration,
    SerializableOperation,
};
use django_rs_db_migrations::Operation;

fn make_field(name: &str, ft: FieldType) -> MigrationFieldDef {
    MigrationFieldDef::new(name, ft)
}

fn sqlite_executor() -> MigrationExecutor {
    MigrationExecutor::new(Box::new(SqliteSchemaEditor))
}

// ── 1. Create table and verify in sqlite_master ─────────────────────────

#[tokio::test]
async fn test_execute_create_model_creates_table() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0001_initial"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![
            make_field("id", FieldType::BigAutoField).primary_key(),
            make_field("title", FieldType::CharField).max_length(200),
        ],
        options: ModelOptions::default(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0001_initial".into()), ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Verify the table was created
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='blog_post'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String>("name").unwrap(), "blog_post");
}

// ── 2. Verify django_migrations table records ───────────────────────────

#[tokio::test]
async fn test_execute_records_in_django_migrations() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0001_initial"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
        options: ModelOptions::default(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0001_initial".into()), ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Verify django_migrations table exists and has the record
    let rows = backend
        .query(
            "SELECT app, name FROM django_migrations WHERE app = 'blog' AND name = '0001_initial'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String>("app").unwrap(), "blog");
    assert_eq!(rows[0].get::<String>("name").unwrap(), "0001_initial");
}

// ── 3. Multiple migrations in sequence ──────────────────────────────────

#[tokio::test]
async fn test_execute_multiple_migrations_in_sequence() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    // First migration: create table
    let mut plan1 = MigrationPlan::new();
    plan1.add_step(MigrationStep::forward("blog", "0001_initial"));

    let ops1: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
        options: ModelOptions::default(),
    })];

    let mut operations1 = HashMap::new();
    operations1.insert(("blog".into(), "0001_initial".into()), ops1);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan1, &operations1, &state, &backend, false)
        .await
        .unwrap();

    // Second migration: add column
    let mut plan2 = MigrationPlan::new();
    plan2.add_step(MigrationStep::forward("blog", "0002_add_title"));

    let ops2: Vec<Box<dyn Operation>> = vec![Box::new(AddField {
        model_name: "post".into(),
        field: make_field("title", FieldType::CharField).max_length(200),
    })];

    let mut operations2 = HashMap::new();
    operations2.insert(("blog".into(), "0002_add_title".into()), ops2);

    // Build state with the model from migration 1
    let mut state2 = ProjectState::new();
    state2.add_model(django_rs_db_migrations::ModelState::new(
        "blog",
        "post",
        vec![make_field("id", FieldType::BigAutoField).primary_key()],
    ));

    executor
        .execute_against_db(&plan2, &operations2, &state2, &backend, false)
        .await
        .unwrap();

    // Verify: insert a row with a title and read it back
    backend
        .execute(
            "INSERT INTO blog_post (title) VALUES (?)",
            &[Value::from("Hello World")],
        )
        .await
        .unwrap();

    let rows = backend
        .query("SELECT title FROM blog_post", &[])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String>("title").unwrap(), "Hello World");

    // Verify both migrations recorded
    let mig_rows = backend
        .query("SELECT app, name FROM django_migrations ORDER BY id", &[])
        .await
        .unwrap();
    assert_eq!(mig_rows.len(), 2);
}

// ── 4. Backward migration (drop table) ─────────────────────────────────

#[tokio::test]
async fn test_execute_backward_drops_table() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    // Forward: create table
    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0001_initial"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
        options: ModelOptions::default(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0001_initial".into()), ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Verify table exists
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='blog_post'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // Backward: drop table
    let mut plan_back = MigrationPlan::new();
    plan_back.add_step(MigrationStep::backward("blog", "0001_initial"));

    let ops_back: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
        options: ModelOptions::default(),
    })];

    let mut operations_back = HashMap::new();
    operations_back.insert(("blog".into(), "0001_initial".into()), ops_back);

    executor
        .execute_against_db(&plan_back, &operations_back, &state, &backend, false)
        .await
        .unwrap();

    // Verify table dropped
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='blog_post'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);

    // Verify migration record removed
    let mig_rows = backend
        .query(
            "SELECT * FROM django_migrations WHERE app = 'blog' AND name = '0001_initial'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(mig_rows.len(), 0);
}

// ── 5. Fake migration ──────────────────────────────────────────────────

#[tokio::test]
async fn test_fake_migration_records_without_executing() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0001_initial"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
        options: ModelOptions::default(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0001_initial".into()), ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, true) // fake=true
        .await
        .unwrap();

    // Table should NOT exist (SQL was not executed)
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='blog_post'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);

    // But the migration should be recorded
    let mig_rows = backend
        .query(
            "SELECT * FROM django_migrations WHERE app = 'blog' AND name = '0001_initial'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(mig_rows.len(), 1);

    // In-memory recorder should also reflect it
    assert!(executor
        .recorder()
        .is_applied(&("blog".into(), "0001_initial".into())));
}

// ── 6. RunSQL operation executes against real database ───────────────────

#[tokio::test]
async fn test_run_sql_executes() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("app", "0001_run_sql"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(RunSQL {
        sql_forwards: "CREATE TABLE app_log (id INTEGER PRIMARY KEY, msg TEXT)".into(),
        sql_backwards: "DROP TABLE app_log".into(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("app".into(), "0001_run_sql".into()), ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Verify table exists
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='app_log'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

// ── 7. RunSQL backward ──────────────────────────────────────────────────

#[tokio::test]
async fn test_run_sql_backward_executes() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    // Forward
    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("app", "0001"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(RunSQL {
        sql_forwards: "CREATE TABLE app_log (id INTEGER PRIMARY KEY, msg TEXT)".into(),
        sql_backwards: "DROP TABLE app_log".into(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("app".into(), "0001".into()), ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Backward
    let mut plan_back = MigrationPlan::new();
    plan_back.add_step(MigrationStep::backward("app", "0001"));

    let ops_back: Vec<Box<dyn Operation>> = vec![Box::new(RunSQL {
        sql_forwards: "CREATE TABLE app_log (id INTEGER PRIMARY KEY, msg TEXT)".into(),
        sql_backwards: "DROP TABLE app_log".into(),
    })];
    let mut ops_back_map = HashMap::new();
    ops_back_map.insert(("app".into(), "0001".into()), ops_back);

    executor
        .execute_against_db(&plan_back, &ops_back_map, &state, &backend, false)
        .await
        .unwrap();

    // Table should be gone
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='app_log'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);
}

// ── 8. MigrationRecorder load_from_db ───────────────────────────────────

#[tokio::test]
async fn test_recorder_load_from_db() {
    let backend = SqliteBackend::memory().unwrap();

    // Create and populate the django_migrations table manually
    let recorder = MigrationRecorder::new();
    recorder.ensure_table(&backend).await.unwrap();
    recorder
        .record_to_db(&backend, "blog", "0001_initial")
        .await
        .unwrap();
    recorder
        .record_to_db(&backend, "auth", "0001_initial")
        .await
        .unwrap();

    // Create a fresh recorder and load from db
    let mut recorder2 = MigrationRecorder::new();
    recorder2.load_from_db(&backend).await.unwrap();

    assert!(recorder2.is_applied(&("blog".into(), "0001_initial".into())));
    assert!(recorder2.is_applied(&("auth".into(), "0001_initial".into())));
    assert!(!recorder2.is_applied(&("blog".into(), "0002".into())));
}

// ── 9. MigrationRecorder ensure_table is idempotent ─────────────────────

#[tokio::test]
async fn test_recorder_ensure_table_idempotent() {
    let backend = SqliteBackend::memory().unwrap();
    let recorder = MigrationRecorder::new();

    // Call twice - should not fail
    recorder.ensure_table(&backend).await.unwrap();
    recorder.ensure_table(&backend).await.unwrap();

    // Table should exist
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='django_migrations'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

// ── 10. MigrationRecorder unrecord_from_db ──────────────────────────────

#[tokio::test]
async fn test_recorder_unrecord_from_db() {
    let backend = SqliteBackend::memory().unwrap();
    let recorder = MigrationRecorder::new();
    recorder.ensure_table(&backend).await.unwrap();
    recorder
        .record_to_db(&backend, "blog", "0001_initial")
        .await
        .unwrap();

    // Verify it's there
    let rows = backend
        .query("SELECT * FROM django_migrations", &[])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // Unrecord
    recorder
        .unrecord_from_db(&backend, "blog", "0001_initial")
        .await
        .unwrap();

    let rows = backend
        .query("SELECT * FROM django_migrations", &[])
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);
}

// ── 11. Serialization round-trip ────────────────────────────────────────

#[test]
fn test_serialization_roundtrip_create_model() {
    let migration = SerializableMigration {
        app_label: "blog".into(),
        name: "0001_initial".into(),
        dependencies: vec![],
        initial: true,
        operations: vec![SerializableOperation::CreateModel {
            name: "post".into(),
            fields: vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200),
                make_field("body", FieldType::TextField).nullable(),
            ],
            options: ModelOptions::default(),
        }],
    };

    let json = migration.to_json().unwrap();
    let deserialized = SerializableMigration::from_json(&json).unwrap();

    assert_eq!(deserialized.app_label, migration.app_label);
    assert_eq!(deserialized.name, migration.name);
    assert!(deserialized.initial);
    assert_eq!(deserialized.operations.len(), 1);

    // Convert to operations and verify
    let ops = deserialized.to_operations();
    assert_eq!(ops.len(), 1);
    assert!(ops[0].describe().contains("Create model"));
}

// ── 12. Serialization round-trip: multiple ops ──────────────────────────

#[test]
fn test_serialization_roundtrip_multiple_operations() {
    let migration = SerializableMigration {
        app_label: "myapp".into(),
        name: "0002_changes".into(),
        dependencies: vec![("myapp".into(), "0001_initial".into())],
        initial: false,
        operations: vec![
            SerializableOperation::AddField {
                model_name: "post".into(),
                field: make_field("slug", FieldType::SlugField).max_length(100),
            },
            SerializableOperation::RemoveField {
                model_name: "post".into(),
                field_name: "old_field".into(),
            },
            SerializableOperation::RunSQL {
                sql_forwards: "CREATE INDEX idx ON blog_post(slug)".into(),
                sql_backwards: "DROP INDEX idx".into(),
            },
        ],
    };

    let json = migration.to_json().unwrap();
    let deserialized = SerializableMigration::from_json(&json).unwrap();

    assert_eq!(deserialized.operations.len(), 3);
    assert_eq!(deserialized.dependencies.len(), 1);
    assert_eq!(
        deserialized.dependencies[0],
        ("myapp".into(), "0001_initial".into())
    );
}

// ── 13. File write / read round-trip ────────────────────────────────────

#[test]
fn test_file_write_read_roundtrip() {
    let dir = std::env::temp_dir().join(format!(
        "django_rs_integ_test_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let migration = SerializableMigration {
        app_label: "shop".into(),
        name: "0001_initial".into(),
        dependencies: vec![],
        initial: true,
        operations: vec![
            SerializableOperation::CreateModel {
                name: "product".into(),
                fields: vec![
                    make_field("id", FieldType::BigAutoField).primary_key(),
                    make_field("name", FieldType::CharField).max_length(255),
                    make_field("price", FieldType::FloatField),
                ],
                options: ModelOptions::default(),
            },
        ],
    };

    let path = migration_file_path(&dir, "shop", "0001_initial");
    migration.write_to_file(&path).unwrap();

    let loaded = SerializableMigration::read_from_file(&path).unwrap();
    assert_eq!(loaded.app_label, "shop");
    assert_eq!(loaded.name, "0001_initial");
    assert!(loaded.initial);
    assert_eq!(loaded.operations.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// ── 14. Generate migration name ─────────────────────────────────────────

#[test]
fn test_generate_migration_name_formats() {
    assert_eq!(generate_migration_name(1, Some("initial")), "0001_initial");
    assert_eq!(
        generate_migration_name(42, Some("add_user_email")),
        "0042_add_user_email"
    );
    // Auto name
    let auto = generate_migration_name(3, None);
    assert!(auto.starts_with("0003_auto_"));
}

// ── 15. next_migration_number ───────────────────────────────────────────

#[test]
fn test_next_migration_number_empty() {
    let num = next_migration_number(std::path::Path::new("/nonexistent"), "app");
    assert_eq!(num, 1);
}

#[test]
fn test_next_migration_number_sequential() {
    let dir = std::env::temp_dir().join(format!(
        "django_rs_integ_num_{}",
        std::process::id()
    ));
    let app_dir = dir.join("myapp");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::write(app_dir.join("0001_initial.json"), "{}").unwrap();
    std::fs::write(app_dir.join("0002_add_stuff.json"), "{}").unwrap();
    std::fs::write(app_dir.join("0003_more.json"), "{}").unwrap();

    assert_eq!(next_migration_number(&dir, "myapp"), 4);

    let _ = std::fs::remove_dir_all(&dir);
}

// ── 16. Execute plan returns SQL ────────────────────────────────────────

#[tokio::test]
async fn test_execute_against_db_returns_sql() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0001"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
        options: ModelOptions::default(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0001".into()), ops);

    let state = ProjectState::new();
    let sqls = executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    assert!(!sqls.is_empty());
    assert!(sqls[0].contains("CREATE TABLE"));
}

// ── 17. Multiple apps in one plan ───────────────────────────────────────

#[tokio::test]
async fn test_execute_multiple_apps() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("auth", "0001_initial"));
    plan.add_step(MigrationStep::forward("blog", "0001_initial"));

    let auth_ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "user".into(),
        fields: vec![
            make_field("id", FieldType::BigAutoField).primary_key(),
            make_field("username", FieldType::CharField).max_length(150),
        ],
        options: ModelOptions::default(),
    })];

    let blog_ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![
            make_field("id", FieldType::BigAutoField).primary_key(),
            make_field("title", FieldType::CharField).max_length(200),
        ],
        options: ModelOptions::default(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("auth".into(), "0001_initial".into()), auth_ops);
    operations.insert(("blog".into(), "0001_initial".into()), blog_ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Both tables should exist
    let tables = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('auth_user', 'blog_post') ORDER BY name",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0].get::<String>("name").unwrap(), "auth_user");
    assert_eq!(tables[1].get::<String>("name").unwrap(), "blog_post");

    // Both migrations recorded
    let mig_rows = backend
        .query("SELECT * FROM django_migrations", &[])
        .await
        .unwrap();
    assert_eq!(mig_rows.len(), 2);
}

// ── 18. In-memory recorder syncs with DB ────────────────────────────────

#[tokio::test]
async fn test_recorder_sync_between_memory_and_db() {
    let backend = SqliteBackend::memory().unwrap();
    let mut recorder = MigrationRecorder::new();
    recorder.ensure_table(&backend).await.unwrap();

    // Record via DB
    recorder
        .record_to_db(&backend, "app", "0001")
        .await
        .unwrap();
    recorder.apply(("app".into(), "0001".into()));

    // Fresh recorder loads from DB
    let mut recorder2 = MigrationRecorder::new();
    recorder2.load_from_db(&backend).await.unwrap();

    assert!(recorder2.is_applied(&("app".into(), "0001".into())));
}

// ── 19. DeleteModel operation executes against real DB ───────────────────

#[tokio::test]
async fn test_delete_model_drops_table() {
    let backend = SqliteBackend::memory().unwrap();

    // First create the table manually
    backend
        .execute(
            "CREATE TABLE blog_post (id INTEGER PRIMARY KEY, title TEXT)",
            &[],
        )
        .await
        .unwrap();

    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0002_delete"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(DeleteModel {
        name: "post".into(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0002_delete".into()), ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Table should be gone
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='blog_post'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);
}

// ── 20. RemoveField operation ───────────────────────────────────────────

#[tokio::test]
async fn test_remove_field_drops_column() {
    let backend = SqliteBackend::memory().unwrap();

    // Create a table with title column
    backend
        .execute(
            "CREATE TABLE blog_post (id INTEGER PRIMARY KEY, title TEXT NOT NULL, body TEXT)",
            &[],
        )
        .await
        .unwrap();

    let mut executor = sqlite_executor();
    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0002_remove_body"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(RemoveField {
        model_name: "post".into(),
        field_name: "body".into(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0002_remove_body".into()), ops);

    // Build state with the full model
    let mut state = ProjectState::new();
    state.add_model(django_rs_db_migrations::ModelState::new(
        "blog",
        "post",
        vec![
            make_field("id", FieldType::BigAutoField).primary_key(),
            make_field("title", FieldType::CharField).max_length(200),
            make_field("body", FieldType::TextField),
        ],
    ));

    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Verify: insert without body column should work, and "body" should not be available
    // SQLite DROP COLUMN was added in 3.35.0 - check if it works
    let columns = backend
        .query("PRAGMA table_info(blog_post)", &[])
        .await
        .unwrap();
    let col_names: Vec<String> = columns
        .iter()
        .map(|r| r.get::<String>("name").unwrap())
        .collect();
    // body should have been removed
    assert!(!col_names.contains(&"body".to_string()));
    assert!(col_names.contains(&"title".to_string()));
}

// ── 21. Execute plan with missing operations returns error ──────────────

#[tokio::test]
async fn test_execute_against_db_missing_ops_error() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0001"));

    let operations = HashMap::new(); // empty!

    let state = ProjectState::new();
    let result = executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await;

    assert!(result.is_err());
}

// ── 22. Serializable operations convert to real operations ──────────────

#[test]
fn test_serializable_ops_to_real_operations() {
    let ops = vec![
        SerializableOperation::CreateModel {
            name: "user".into(),
            fields: vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("name", FieldType::CharField).max_length(100),
            ],
            options: ModelOptions::default(),
        },
        SerializableOperation::AddField {
            model_name: "user".into(),
            field: make_field("email", FieldType::EmailField).max_length(254),
        },
        SerializableOperation::RemoveField {
            model_name: "user".into(),
            field_name: "temp".into(),
        },
        SerializableOperation::RenameField {
            model_name: "user".into(),
            old_name: "name".into(),
            new_name: "full_name".into(),
        },
        SerializableOperation::DeleteModel {
            name: "old_table".into(),
        },
    ];

    for op in &ops {
        let real_op = op.to_operation();
        assert!(!real_op.describe().is_empty());
    }
}

// ── 23. Full end-to-end: serialize, write, load, execute ────────────────

#[tokio::test]
async fn test_end_to_end_serialize_load_execute() {
    let dir = std::env::temp_dir().join(format!(
        "django_rs_e2e_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    // Step 1: Create a serializable migration
    let migration = SerializableMigration {
        app_label: "shop".into(),
        name: "0001_initial".into(),
        dependencies: vec![],
        initial: true,
        operations: vec![SerializableOperation::CreateModel {
            name: "product".into(),
            fields: vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("name", FieldType::CharField).max_length(255),
            ],
            options: ModelOptions::default(),
        }],
    };

    // Step 2: Write to file
    let path = migration_file_path(&dir, "shop", "0001_initial");
    migration.write_to_file(&path).unwrap();

    // Step 3: Load via MigrationLoader
    let mut loader = django_rs_db_migrations::MigrationLoader::new(&dir);
    let graph = loader.load().unwrap();
    assert_eq!(graph.len(), 1);
    assert!(graph.contains(&("shop".into(), "0001_initial".into())));

    // Step 4: Read back the serialized ops and execute
    let loaded = SerializableMigration::read_from_file(&path).unwrap();
    let real_ops = loaded.to_operations();

    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("shop", "0001_initial"));

    let mut operations = HashMap::new();
    operations.insert(("shop".into(), "0001_initial".into()), real_ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Verify table created
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='shop_product'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// ── 24. Recorder load from empty db ─────────────────────────────────────

#[tokio::test]
async fn test_recorder_load_from_empty_db() {
    let backend = SqliteBackend::memory().unwrap();
    let mut recorder = MigrationRecorder::new();
    recorder.load_from_db(&backend).await.unwrap();
    assert!(recorder.applied().is_empty());
}

// ── 25. Multiple records and load ───────────────────────────────────────

#[tokio::test]
async fn test_recorder_multiple_records() {
    let backend = SqliteBackend::memory().unwrap();
    let recorder = MigrationRecorder::new();
    recorder.ensure_table(&backend).await.unwrap();

    for i in 1..=5 {
        recorder
            .record_to_db(&backend, "app", &format!("{i:04}_step"))
            .await
            .unwrap();
    }

    let rows = backend
        .query("SELECT COUNT(*) as cnt FROM django_migrations", &[])
        .await
        .unwrap();
    assert_eq!(rows[0].get::<i64>("cnt").unwrap(), 5);
}

// ── 26. Backward AddField re-adds column ────────────────────────────────

#[tokio::test]
async fn test_add_field_backward_drops_column() {
    let backend = SqliteBackend::memory().unwrap();

    // Start with a table that has the column
    backend
        .execute(
            "CREATE TABLE blog_post (id INTEGER PRIMARY KEY, title TEXT NOT NULL)",
            &[],
        )
        .await
        .unwrap();

    let mut executor = sqlite_executor();

    // Mark as applied
    executor
        .recorder_mut()
        .apply(("blog".into(), "0002_add_title".into()));

    // Now reverse it
    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::backward("blog", "0002_add_title"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(AddField {
        model_name: "post".into(),
        field: make_field("title", FieldType::CharField).max_length(200),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0002_add_title".into()), ops);

    let state = ProjectState::new();
    let sqls = executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Should have generated DROP COLUMN SQL
    assert!(sqls.iter().any(|s| s.contains("DROP COLUMN")));
}

// ── 27. Schema SQL variants ─────────────────────────────────────────────

#[test]
fn test_ensure_schema_sql_sqlite() {
    let sql = MigrationRecorder::ensure_schema_sql_sqlite();
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS"));
    assert!(sql.contains("django_migrations"));
    assert!(sql.contains("INTEGER PRIMARY KEY AUTOINCREMENT"));
}

#[test]
fn test_ensure_schema_sql_pg() {
    let sqls = MigrationRecorder::ensure_schema_sql();
    assert_eq!(sqls.len(), 1);
    assert!(sqls[0].contains("BIGSERIAL"));
}

// ── 28. Serialization of all field types ────────────────────────────────

#[test]
fn test_serialize_all_field_types() {
    let fields = vec![
        make_field("f1", FieldType::AutoField),
        make_field("f2", FieldType::BigAutoField),
        make_field("f3", FieldType::CharField),
        make_field("f4", FieldType::TextField),
        make_field("f5", FieldType::IntegerField),
        make_field("f6", FieldType::BigIntegerField),
        make_field("f7", FieldType::SmallIntegerField),
        make_field("f8", FieldType::FloatField),
        make_field(
            "f9",
            FieldType::DecimalField {
                max_digits: 10,
                decimal_places: 2,
            },
        ),
        make_field("f10", FieldType::BooleanField),
        make_field("f11", FieldType::DateField),
        make_field("f12", FieldType::DateTimeField),
        make_field("f13", FieldType::TimeField),
        make_field("f14", FieldType::DurationField),
        make_field("f15", FieldType::UuidField),
        make_field("f16", FieldType::BinaryField),
        make_field("f17", FieldType::JsonField),
        make_field("f18", FieldType::EmailField),
        make_field("f19", FieldType::UrlField),
        make_field("f20", FieldType::SlugField),
        make_field("f21", FieldType::IpAddressField),
        make_field("f22", FieldType::FilePathField),
    ];

    for field in &fields {
        let json = serde_json::to_string(field).unwrap();
        let deserialized: MigrationFieldDef = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, field.name);
    }
}

// ── 29. Serialization of relational field types ─────────────────────────

#[test]
fn test_serialize_relational_fields() {
    use django_rs_db::fields::OnDelete;

    let fields = vec![
        make_field(
            "author_id",
            FieldType::ForeignKey {
                to: "auth.User".into(),
                on_delete: OnDelete::Cascade,
                related_name: Some("posts".into()),
            },
        ),
        make_field(
            "profile_id",
            FieldType::OneToOneField {
                to: "auth.Profile".into(),
                on_delete: OnDelete::SetNull,
                related_name: None,
            },
        ),
        make_field(
            "tags",
            FieldType::ManyToManyField {
                to: "tagging.Tag".into(),
                through: Some("blog.PostTag".into()),
                related_name: Some("posts".into()),
            },
        ),
    ];

    for field in &fields {
        let json = serde_json::to_string(field).unwrap();
        let deserialized: MigrationFieldDef = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, field.name);
        assert!(deserialized.is_relation());
    }
}

// ── 30. Deserialization error on invalid JSON ───────────────────────────

#[test]
fn test_deserialization_error_on_invalid_json() {
    let result = SerializableMigration::from_json("not json at all");
    assert!(result.is_err());
}

// ── 31. Empty migration plan executes without error ─────────────────────

#[tokio::test]
async fn test_execute_empty_plan() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let plan = MigrationPlan::new();
    let operations = HashMap::new();
    let state = ProjectState::new();

    let sqls = executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    assert!(sqls.is_empty());
}

// ── 32. Execute two-step plan in single call ────────────────────────────

#[tokio::test]
async fn test_execute_two_step_plan() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0001_initial"));
    plan.add_step(MigrationStep::forward("blog", "0002_add_body"));

    let ops1: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![
            make_field("id", FieldType::BigAutoField).primary_key(),
            make_field("title", FieldType::CharField).max_length(200),
        ],
        options: ModelOptions::default(),
    })];

    let ops2: Vec<Box<dyn Operation>> = vec![Box::new(AddField {
        model_name: "post".into(),
        field: make_field("body", FieldType::TextField).nullable(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0001_initial".into()), ops1);
    operations.insert(("blog".into(), "0002_add_body".into()), ops2);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Verify both columns exist
    let columns = backend
        .query("PRAGMA table_info(blog_post)", &[])
        .await
        .unwrap();
    let col_names: Vec<String> = columns
        .iter()
        .map(|r| r.get::<String>("name").unwrap())
        .collect();
    assert!(col_names.contains(&"title".to_string()));
    assert!(col_names.contains(&"body".to_string()));

    // Both migrations recorded
    assert!(executor
        .recorder()
        .is_applied(&("blog".into(), "0001_initial".into())));
    assert!(executor
        .recorder()
        .is_applied(&("blog".into(), "0002_add_body".into())));
}

// ── 33. Data survives through migration ─────────────────────────────────

#[tokio::test]
async fn test_data_survives_add_field_migration() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    // Create table
    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0001"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![
            make_field("id", FieldType::BigAutoField).primary_key(),
            make_field("title", FieldType::CharField).max_length(200),
        ],
        options: ModelOptions::default(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0001".into()), ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Insert data
    backend
        .execute(
            "INSERT INTO blog_post (title) VALUES (?)",
            &[Value::from("First Post")],
        )
        .await
        .unwrap();

    // Add a nullable column
    let mut plan2 = MigrationPlan::new();
    plan2.add_step(MigrationStep::forward("blog", "0002"));

    let ops2: Vec<Box<dyn Operation>> = vec![Box::new(AddField {
        model_name: "post".into(),
        field: make_field("body", FieldType::TextField).nullable(),
    })];

    let mut ops2_map = HashMap::new();
    ops2_map.insert(("blog".into(), "0002".into()), ops2);

    let mut state2 = ProjectState::new();
    state2.add_model(django_rs_db_migrations::ModelState::new(
        "blog",
        "post",
        vec![
            make_field("id", FieldType::BigAutoField).primary_key(),
            make_field("title", FieldType::CharField).max_length(200),
        ],
    ));

    executor
        .execute_against_db(&plan2, &ops2_map, &state2, &backend, false)
        .await
        .unwrap();

    // Original data should still be there
    let rows = backend
        .query("SELECT title FROM blog_post", &[])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String>("title").unwrap(), "First Post");
}

// ── 34. Recorder record_applied_sql format ──────────────────────────────

#[test]
fn test_record_applied_sql_format() {
    let sql = MigrationRecorder::record_applied_sql("myapp", "0001_initial");
    assert!(sql.starts_with("INSERT INTO"));
    assert!(sql.contains("myapp"));
    assert!(sql.contains("0001_initial"));
    assert!(sql.contains("CURRENT_TIMESTAMP"));
}

// ── 35. Recorder record_unapplied_sql format ────────────────────────────

#[test]
fn test_record_unapplied_sql_format() {
    let sql = MigrationRecorder::record_unapplied_sql("myapp", "0001_initial");
    assert!(sql.starts_with("DELETE FROM"));
    assert!(sql.contains("myapp"));
    assert!(sql.contains("0001_initial"));
}

// ── 36. Fake backward migration ─────────────────────────────────────────

#[tokio::test]
async fn test_fake_backward_migration() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    // Create table for real
    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("blog", "0001"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
        options: ModelOptions::default(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("blog".into(), "0001".into()), ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Now fake backward
    let mut plan_back = MigrationPlan::new();
    plan_back.add_step(MigrationStep::backward("blog", "0001"));

    let ops_back: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "post".into(),
        fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
        options: ModelOptions::default(),
    })];
    let mut ops_map = HashMap::new();
    ops_map.insert(("blog".into(), "0001".into()), ops_back);

    executor
        .execute_against_db(&plan_back, &ops_map, &state, &backend, true) // fake=true
        .await
        .unwrap();

    // Table should STILL exist (SQL was not executed)
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='blog_post'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // But migration should be unrecorded
    let mig_rows = backend
        .query(
            "SELECT * FROM django_migrations WHERE app = 'blog' AND name = '0001'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(mig_rows.len(), 0);
}

// ── 37. Serializable migration with dependencies ────────────────────────

#[test]
fn test_serializable_migration_with_dependencies() {
    let migration = SerializableMigration {
        app_label: "blog".into(),
        name: "0002_add_author".into(),
        dependencies: vec![
            ("blog".into(), "0001_initial".into()),
            ("auth".into(), "0001_initial".into()),
        ],
        initial: false,
        operations: vec![],
    };

    let json = migration.to_json().unwrap();
    let loaded = SerializableMigration::from_json(&json).unwrap();
    assert_eq!(loaded.dependencies.len(), 2);
}

// ── 38. Verify data insertion after migration ───────────────────────────

#[tokio::test]
async fn test_insert_data_into_migrated_table() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("shop", "0001"));

    let ops: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "product".into(),
        fields: vec![
            make_field("id", FieldType::BigAutoField).primary_key(),
            make_field("name", FieldType::CharField).max_length(255),
            make_field("active", FieldType::BooleanField),
        ],
        options: ModelOptions::default(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("shop".into(), "0001".into()), ops);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Insert and query
    backend
        .execute(
            "INSERT INTO shop_product (name, active) VALUES (?, ?)",
            &[Value::from("Widget"), Value::Bool(true)],
        )
        .await
        .unwrap();

    let rows = backend
        .query("SELECT name, active FROM shop_product", &[])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String>("name").unwrap(), "Widget");
}

// ── 39. Generate migration name with zero-padding ───────────────────────

#[test]
fn test_migration_name_zero_padding() {
    assert!(generate_migration_name(1, Some("init")).starts_with("0001_"));
    assert!(generate_migration_name(10, Some("x")).starts_with("0010_"));
    assert!(generate_migration_name(100, Some("x")).starts_with("0100_"));
    assert!(generate_migration_name(9999, Some("x")).starts_with("9999_"));
}

// ── 40. CreateModel and DeleteModel in same plan ────────────────────────

#[tokio::test]
async fn test_create_then_delete_in_plan() {
    let backend = SqliteBackend::memory().unwrap();
    let mut executor = sqlite_executor();

    let mut plan = MigrationPlan::new();
    plan.add_step(MigrationStep::forward("temp", "0001_create"));
    plan.add_step(MigrationStep::forward("temp", "0002_delete"));

    let ops1: Vec<Box<dyn Operation>> = vec![Box::new(CreateModel {
        name: "temp_table".into(),
        fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
        options: ModelOptions::default(),
    })];

    let ops2: Vec<Box<dyn Operation>> = vec![Box::new(DeleteModel {
        name: "temp_table".into(),
    })];

    let mut operations = HashMap::new();
    operations.insert(("temp".into(), "0001_create".into()), ops1);
    operations.insert(("temp".into(), "0002_delete".into()), ops2);

    let state = ProjectState::new();
    executor
        .execute_against_db(&plan, &operations, &state, &backend, false)
        .await
        .unwrap();

    // Table should not exist (created then dropped)
    let rows = backend
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='temp_temp_table'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);

    // Both migrations should be recorded
    let mig_rows = backend
        .query("SELECT * FROM django_migrations WHERE app = 'temp'", &[])
        .await
        .unwrap();
    assert_eq!(mig_rows.len(), 2);
}

// ── 41. Serialization of migration with AlterUniqueTogether ─────────────

#[test]
fn test_serialize_alter_unique_together() {
    let migration = SerializableMigration {
        app_label: "blog".into(),
        name: "0003_unique".into(),
        dependencies: vec![("blog".into(), "0002_changes".into())],
        initial: false,
        operations: vec![SerializableOperation::AlterUniqueTogether {
            model_name: "post".into(),
            unique_together: vec![vec!["author".into(), "slug".into()]],
        }],
    };

    let json = migration.to_json().unwrap();
    let loaded = SerializableMigration::from_json(&json).unwrap();
    if let SerializableOperation::AlterUniqueTogether {
        unique_together, ..
    } = &loaded.operations[0]
    {
        assert_eq!(unique_together.len(), 1);
        assert_eq!(unique_together[0], vec!["author", "slug"]);
    } else {
        panic!("Expected AlterUniqueTogether");
    }
}

// ── 42. Migration file path construction ────────────────────────────────

#[test]
fn test_migration_file_path_construction() {
    use std::path::{Path, PathBuf};

    let path = migration_file_path(Path::new("/app/migrations"), "blog", "0001_initial");
    assert_eq!(
        path,
        PathBuf::from("/app/migrations/blog/0001_initial.json")
    );
}

// ── 43. Serialization with default values ───────────────────────────────

#[test]
fn test_serialize_field_with_defaults() {
    let migration = SerializableMigration {
        app_label: "app".into(),
        name: "0001".into(),
        dependencies: vec![],
        initial: true,
        operations: vec![SerializableOperation::CreateModel {
            name: "config".into(),
            fields: vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("active", FieldType::BooleanField).default(Value::Bool(true)),
                make_field("count", FieldType::IntegerField).default(Value::Int(0)),
                make_field("label", FieldType::CharField)
                    .max_length(50)
                    .default(Value::String("default".into())),
            ],
            options: ModelOptions::default(),
        }],
    };

    let json = migration.to_json().unwrap();
    let loaded = SerializableMigration::from_json(&json).unwrap();

    if let SerializableOperation::CreateModel { fields, .. } = &loaded.operations[0] {
        let active = fields.iter().find(|f| f.name == "active").unwrap();
        assert_eq!(active.default, Some(Value::Bool(true)));

        let count = fields.iter().find(|f| f.name == "count").unwrap();
        assert_eq!(count.default, Some(Value::Int(0)));

        let label = fields.iter().find(|f| f.name == "label").unwrap();
        assert_eq!(
            label.default,
            Some(Value::String("default".into()))
        );
    } else {
        panic!("Expected CreateModel");
    }
}
