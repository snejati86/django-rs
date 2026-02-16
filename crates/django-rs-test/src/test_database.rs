//! Test database utilities for django-rs.
//!
//! Provides [`TestDatabase`], an in-memory SQLite database wrapper for use in
//! tests. It implements [`DbExecutor`] so it can be used with all ORM
//! operations, and adds helper methods for setting up tables from model
//! metadata and counting executed queries.
//!
//! ## Example
//!
//! ```rust,no_run
//! use django_rs_test::test_database::TestDatabase;
//!
//! async fn example() {
//!     let db = TestDatabase::new();
//!     db.execute_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
//!         .await
//!         .unwrap();
//! }
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use django_rs_core::DjangoResult;
use django_rs_db::model::ModelMeta;
use django_rs_db::query::compiler::{DatabaseBackendType, Row};
use django_rs_db::value::Value;
use django_rs_db::DbExecutor;
use django_rs_db_backends::sqlite::SqliteBackend;

/// An in-memory SQLite database for testing.
///
/// Wraps a [`SqliteBackend`] with an `Arc` for thread-safe sharing and adds a
/// query counter for use with [`assert_num_queries`](crate::assert_num_queries).
///
/// The database is created fresh in memory for each `TestDatabase::new()` call,
/// providing complete test isolation.
#[derive(Clone)]
pub struct TestDatabase {
    backend: Arc<SqliteBackend>,
    query_count: Arc<AtomicUsize>,
}

impl TestDatabase {
    /// Creates a new in-memory SQLite test database.
    ///
    /// # Panics
    ///
    /// Panics if the in-memory database cannot be created.
    pub fn new() -> Self {
        let backend = SqliteBackend::memory().expect("Failed to create in-memory SQLite database");
        Self {
            backend: Arc::new(backend),
            query_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Creates a table from the given [`ModelMeta`].
    ///
    /// Generates a `CREATE TABLE` statement from the field definitions in the
    /// model metadata and executes it against the test database.
    ///
    /// # Errors
    ///
    /// Returns an error if the SQL execution fails.
    pub async fn setup_table(&self, meta: &ModelMeta) -> DjangoResult<()> {
        let sql = Self::create_table_sql(meta);
        self.execute_raw(&sql).await?;
        Ok(())
    }

    /// Drops all user-created tables in the database.
    ///
    /// Queries `sqlite_master` for all table names and drops them one by one.
    ///
    /// # Errors
    ///
    /// Returns an error if the SQL execution fails.
    pub async fn teardown(&self) -> DjangoResult<()> {
        let rows = self
            .backend
            .query(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                &[],
            )
            .await?;

        for row in &rows {
            let table_name: String = row.get("name")?;
            self.backend
                .execute_sql(&format!("DROP TABLE IF EXISTS \"{table_name}\""), &[])
                .await?;
        }
        Ok(())
    }

    /// Executes a raw SQL string with no parameters.
    ///
    /// Increments the query counter.
    ///
    /// # Errors
    ///
    /// Returns an error if the SQL execution fails.
    pub async fn execute_raw(&self, sql: &str) -> DjangoResult<u64> {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        self.backend.execute_sql(sql, &[]).await
    }

    /// Returns the current query count.
    pub fn query_count(&self) -> usize {
        self.query_count.load(Ordering::Relaxed)
    }

    /// Resets the query counter to zero.
    pub fn reset_query_count(&self) {
        self.query_count.store(0, Ordering::Relaxed);
    }

    /// Returns a reference to the inner `SqliteBackend`.
    pub fn backend(&self) -> &SqliteBackend {
        &self.backend
    }

    /// Generates a `CREATE TABLE IF NOT EXISTS` SQL statement from model metadata.
    fn create_table_sql(meta: &ModelMeta) -> String {
        use django_rs_db::fields::FieldType;

        let table_name = &meta.db_table;
        let mut col_defs: Vec<String> = Vec::new();

        for field in &meta.fields {
            let type_str = match &field.field_type {
                FieldType::AutoField
                | FieldType::BigAutoField
                | FieldType::IntegerField
                | FieldType::BigIntegerField
                | FieldType::SmallIntegerField
                | FieldType::BooleanField
                | FieldType::ForeignKey { .. }
                | FieldType::OneToOneField { .. } => "INTEGER",
                FieldType::FloatField | FieldType::DecimalField { .. } => "REAL",
                FieldType::BinaryField => "BLOB",
                FieldType::ManyToManyField { .. } => continue,
                // All other field types (CharField, TextField, DateField,
                // UuidField, JsonField, etc.) map to TEXT in SQLite.
                _ => "TEXT",
            };

            let mut parts = vec![format!("\"{}\" {type_str}", field.column)];

            if field.primary_key {
                parts.push("PRIMARY KEY".to_string());
                if matches!(
                    field.field_type,
                    FieldType::AutoField | FieldType::BigAutoField
                ) {
                    parts.push("AUTOINCREMENT".to_string());
                }
            } else if !field.null {
                parts.push("NOT NULL".to_string());
            }

            if field.unique && !field.primary_key {
                parts.push("UNIQUE".to_string());
            }

            col_defs.push(parts.join(" "));
        }

        let body = col_defs.join(", ");
        format!("CREATE TABLE IF NOT EXISTS \"{table_name}\" ({body})")
    }
}

impl Default for TestDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DbExecutor for TestDatabase {
    fn backend_type(&self) -> DatabaseBackendType {
        DatabaseBackendType::SQLite
    }

    async fn execute_sql(&self, sql: &str, params: &[Value]) -> DjangoResult<u64> {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        self.backend.execute_sql(sql, params).await
    }

    async fn query(&self, sql: &str, params: &[Value]) -> DjangoResult<Vec<Row>> {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        self.backend.query(sql, params).await
    }

    async fn query_one(&self, sql: &str, params: &[Value]) -> DjangoResult<Row> {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        self.backend.query_one(sql, params).await
    }

    async fn insert_returning_id(&self, sql: &str, params: &[Value]) -> DjangoResult<Value> {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        self.backend.insert_returning_id(sql, params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use django_rs_db::fields::{FieldDef, FieldType};
    use django_rs_db::query::compiler::InheritanceType;

    fn sample_model_meta() -> ModelMeta {
        ModelMeta {
            app_label: "test",
            model_name: "article",
            db_table: "test_article".to_string(),
            verbose_name: "article".to_string(),
            verbose_name_plural: "articles".to_string(),
            ordering: vec![],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("title", FieldType::CharField).max_length(200),
                FieldDef::new("body", FieldType::TextField).nullable(),
            ],
            constraints: vec![],
            inheritance_type: InheritanceType::None,
        }
    }

    #[tokio::test]
    async fn test_new_creates_database() {
        let db = TestDatabase::new();
        assert_eq!(db.backend_type(), DatabaseBackendType::SQLite);
    }

    #[tokio::test]
    async fn test_default_creates_database() {
        let db = TestDatabase::default();
        assert_eq!(db.backend_type(), DatabaseBackendType::SQLite);
    }

    #[tokio::test]
    async fn test_execute_raw() {
        let db = TestDatabase::new();
        db.execute_raw("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .await
            .unwrap();
        let result = db
            .execute_sql(
                "INSERT INTO test (name) VALUES (?)",
                &[Value::from("hello")],
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_setup_table_from_meta() {
        let db = TestDatabase::new();
        let meta = sample_model_meta();
        db.setup_table(&meta).await.unwrap();

        // Insert a row to verify the table exists and has the right schema
        db.execute_sql(
            "INSERT INTO test_article (title) VALUES (?)",
            &[Value::from("Test Article")],
        )
        .await
        .unwrap();

        let rows = db
            .query("SELECT id, title, body FROM test_article", &[])
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get::<String>("title").unwrap(), "Test Article");
    }

    #[tokio::test]
    async fn test_teardown_drops_tables() {
        let db = TestDatabase::new();
        db.execute_raw("CREATE TABLE a (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();
        db.execute_raw("CREATE TABLE b (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();

        db.teardown().await.unwrap();

        // Tables should be gone -- inserting should fail
        let result = db
            .execute_sql("INSERT INTO a (id) VALUES (?)", &[Value::from(1)])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_query_counter() {
        let db = TestDatabase::new();
        assert_eq!(db.query_count(), 0);

        db.execute_raw("CREATE TABLE counter_test (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();
        assert_eq!(db.query_count(), 1);

        db.execute_sql(
            "INSERT INTO counter_test (id) VALUES (?)",
            &[Value::from(1)],
        )
        .await
        .unwrap();
        assert_eq!(db.query_count(), 2);

        db.query("SELECT * FROM counter_test", &[]).await.unwrap();
        assert_eq!(db.query_count(), 3);

        db.reset_query_count();
        assert_eq!(db.query_count(), 0);
    }

    #[tokio::test]
    async fn test_query_one_counts() {
        let db = TestDatabase::new();
        db.execute_raw("CREATE TABLE qo (id INTEGER PRIMARY KEY, val TEXT)")
            .await
            .unwrap();
        db.execute_sql("INSERT INTO qo (val) VALUES (?)", &[Value::from("x")])
            .await
            .unwrap();
        db.reset_query_count();

        let _row = db
            .query_one("SELECT val FROM qo WHERE id = ?", &[Value::from(1)])
            .await
            .unwrap();
        assert_eq!(db.query_count(), 1);
    }

    #[tokio::test]
    async fn test_insert_returning_id_counts() {
        let db = TestDatabase::new();
        db.execute_raw("CREATE TABLE irc (id INTEGER PRIMARY KEY, val TEXT)")
            .await
            .unwrap();
        db.reset_query_count();

        let pk = db
            .insert_returning_id(
                "INSERT INTO irc (val) VALUES (?)",
                &[Value::from("test")],
            )
            .await
            .unwrap();
        assert_eq!(pk, Value::Int(1));
        assert_eq!(db.query_count(), 1);
    }

    #[tokio::test]
    async fn test_clone_shares_state() {
        let db = TestDatabase::new();
        let db2 = db.clone();
        db.execute_raw("CREATE TABLE shared (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();

        // The clone should see the same table
        let result = db2.query("SELECT * FROM shared", &[]).await;
        assert!(result.is_ok());

        // Query counters should be shared
        assert_eq!(db.query_count(), db2.query_count());
    }
}
