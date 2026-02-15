//! Raw SQL query support.
//!
//! This module provides the ability to execute raw SQL queries and map results
//! to model instances or plain rows. It mirrors Django's `Manager.raw()` and
//! `cursor.execute()` functionality.
//!
//! All raw SQL queries use parameterized queries to prevent SQL injection.
//!
//! # Examples
//!
//! ```ignore
//! use django_rs_db::query::raw::{RawQuerySet, RawSql};
//! use django_rs_db::value::Value;
//!
//! // Create a raw query that maps to a model
//! let raw = RawQuerySet::<MyModel>::new(
//!     "SELECT * FROM users WHERE id = $1",
//!     vec![Value::Int(1)],
//! );
//! ```

use crate::executor::DbExecutor;
use crate::model::Model;
use crate::query::compiler::Row;
use crate::value::Value;
use django_rs_core::{DjangoError, DjangoResult};
use std::marker::PhantomData;

/// A raw SQL query that returns model instances.
///
/// `RawQuerySet` is the equivalent of Django's `RawQuerySet`, returned by
/// `Model.objects.raw()`. It executes a raw SQL query and maps the results
/// to model instances via `M::from_row()`.
///
/// The query MUST use parameterized placeholders (e.g., `$1` for PostgreSQL,
/// `?` for SQLite/MySQL) to prevent SQL injection.
pub struct RawQuerySet<M: Model> {
    /// The raw SQL query string.
    sql: String,
    /// Parameterized query values.
    params: Vec<Value>,
    /// Optional column-to-field mapping overrides.
    translations: Vec<(String, String)>,
    _phantom: PhantomData<M>,
}

impl<M: Model> RawQuerySet<M> {
    /// Creates a new raw query set.
    ///
    /// The SQL should be a SELECT query that returns columns matching
    /// the model's fields. Use parameterized placeholders for values.
    pub fn new(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
            translations: Vec::new(),
            _phantom: PhantomData,
        }
    }

    /// Adds column-to-field name translations.
    ///
    /// Use this when the raw SQL column names don't match the model field names.
    /// Each pair is (sql_column_name, model_field_name).
    #[must_use]
    pub fn translate(mut self, translations: Vec<(String, String)>) -> Self {
        self.translations = translations;
        self
    }

    /// Returns a reference to the SQL query string.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Returns a reference to the query parameters.
    pub fn params(&self) -> &[Value] {
        &self.params
    }

    /// Executes the raw query and returns model instances.
    ///
    /// Each row returned by the query is mapped to a model instance via
    /// `M::from_row()`. If translations are set, column names are remapped
    /// before constructing the model.
    pub async fn execute(&self, db: &dyn DbExecutor) -> DjangoResult<Vec<M>> {
        let rows = db.query(&self.sql, &self.params).await?;

        if self.translations.is_empty() {
            rows.iter().map(M::from_row).collect()
        } else {
            // Remap column names based on translations
            rows.iter()
                .map(|row| {
                    let translated = self.translate_row(row);
                    M::from_row(&translated)
                })
                .collect()
        }
    }

    /// Executes the raw query and returns the first model instance, or None.
    pub async fn first(&self, db: &dyn DbExecutor) -> DjangoResult<Option<M>> {
        let rows = db.query(&self.sql, &self.params).await?;
        match rows.into_iter().next() {
            Some(row) => {
                if self.translations.is_empty() {
                    Ok(Some(M::from_row(&row)?))
                } else {
                    let translated = self.translate_row(&row);
                    Ok(Some(M::from_row(&translated)?))
                }
            }
            None => Ok(None),
        }
    }

    /// Applies column name translations to a row.
    fn translate_row(&self, row: &Row) -> Row {
        let columns: Vec<String> = row
            .columns()
            .iter()
            .map(|col| {
                self.translations
                    .iter()
                    .find(|(from, _)| from == col)
                    .map_or_else(|| col.clone(), |(_, to)| to.clone())
            })
            .collect();

        // Extract values by index
        let values: Vec<Value> = (0..row.len())
            .map(|i| row.get_by_index::<Value>(i).unwrap_or(Value::Null))
            .collect();

        Row::new(columns, values)
    }
}

/// A direct SQL execution interface for queries that don't map to models.
///
/// `RawSql` is the equivalent of Django's `connection.cursor()` + `cursor.execute()`.
/// It provides direct SQL execution returning raw rows rather than model instances.
pub struct RawSql {
    /// The SQL statement to execute.
    sql: String,
    /// Parameterized values.
    params: Vec<Value>,
}

impl RawSql {
    /// Creates a new raw SQL statement.
    ///
    /// Use parameterized placeholders for all user-provided values.
    pub fn new(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
        }
    }

    /// Returns a reference to the SQL string.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Returns a reference to the query parameters.
    pub fn params(&self) -> &[Value] {
        &self.params
    }

    /// Executes the SQL as a query and returns all result rows.
    ///
    /// Use this for SELECT statements or any statement that returns rows.
    pub async fn fetch_all(&self, db: &dyn DbExecutor) -> DjangoResult<Vec<Row>> {
        db.query(&self.sql, &self.params).await
    }

    /// Executes the SQL as a query and returns the first row, or None.
    pub async fn fetch_one(&self, db: &dyn DbExecutor) -> DjangoResult<Option<Row>> {
        let rows = db.query(&self.sql, &self.params).await?;
        Ok(rows.into_iter().next())
    }

    /// Executes the SQL as a statement (INSERT, UPDATE, DELETE).
    ///
    /// Returns the number of affected rows.
    pub async fn execute(&self, db: &dyn DbExecutor) -> DjangoResult<u64> {
        db.execute_sql(&self.sql, &self.params).await
    }

    /// Executes multiple SQL statements in sequence.
    ///
    /// Each statement uses the same parameters. Returns the total number
    /// of affected rows across all statements.
    pub async fn execute_many(
        statements: &[RawSql],
        db: &dyn DbExecutor,
    ) -> DjangoResult<u64> {
        let mut total = 0u64;
        for stmt in statements {
            total += stmt.execute(db).await?;
        }
        Ok(total)
    }
}

/// Validates that a raw SQL string uses parameterized queries rather than
/// string interpolation for values. This is a best-effort check.
///
/// Returns `Ok(())` if the query appears safe, or an error if it contains
/// patterns that suggest string interpolation.
pub fn validate_raw_sql(sql: &str) -> DjangoResult<()> {
    // Check for common SQL injection patterns
    // This is a heuristic check, not a full SQL parser
    let lower = sql.to_lowercase();

    // Check for unparameterized string literals that might be user input
    // (This is intentionally conservative — we only warn about suspicious patterns)
    if lower.contains("'; ") || lower.contains("\"; ") {
        return Err(DjangoError::SuspiciousOperation(
            "Raw SQL query contains patterns that may indicate SQL injection. \
             Use parameterized queries instead of string interpolation."
                .to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::{FieldDef, FieldType};
    use crate::model::ModelMeta;
    use crate::query::compiler::{DatabaseBackendType, OrderBy};
    use tokio::sync::Mutex as TokioMutex;

    // Test model
    struct TestUser {
        id: i64,
        name: String,
    }

    impl Model for TestUser {
        fn meta() -> &'static ModelMeta {
            use std::sync::LazyLock;
            static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
                app_label: "test",
                model_name: "testuser",
                db_table: "test_user".to_string(),
                verbose_name: "user".to_string(),
                verbose_name_plural: "users".to_string(),
                ordering: vec![OrderBy::asc("name")],
                unique_together: vec![],
                indexes: vec![],
                abstract_model: false,
                fields: vec![
                    FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                    FieldDef::new("name", FieldType::CharField).max_length(100),
                ],
                constraints: vec![],
            });
            &META
        }
        fn table_name() -> &'static str {
            "test_user"
        }
        fn app_label() -> &'static str {
            "test"
        }
        fn pk(&self) -> Option<&Value> {
            if self.id == 0 {
                None
            } else {
                Some(&Value::Int(0))
            }
        }
        fn set_pk(&mut self, value: Value) {
            if let Value::Int(id) = value {
                self.id = id;
            }
        }
        fn field_values(&self) -> Vec<(&'static str, Value)> {
            vec![
                ("id", Value::Int(self.id)),
                ("name", Value::String(self.name.clone())),
            ]
        }
        fn from_row(row: &Row) -> Result<Self, DjangoError> {
            Ok(TestUser {
                id: row.get("id")?,
                name: row.get("name")?,
            })
        }
    }

    /// Mock database that returns predefined rows.
    struct MockDb {
        rows: TokioMutex<Vec<Row>>,
        executed: TokioMutex<Vec<(String, Vec<Value>)>>,
    }

    impl MockDb {
        fn new(rows: Vec<Row>) -> Self {
            Self {
                rows: TokioMutex::new(rows),
                executed: TokioMutex::new(Vec::new()),
            }
        }

        async fn executed(&self) -> Vec<(String, Vec<Value>)> {
            self.executed.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl DbExecutor for MockDb {
        fn backend_type(&self) -> DatabaseBackendType {
            DatabaseBackendType::PostgreSQL
        }

        async fn execute_sql(&self, sql: &str, params: &[Value]) -> DjangoResult<u64> {
            self.executed
                .lock()
                .await
                .push((sql.to_string(), params.to_vec()));
            Ok(1)
        }

        async fn query(&self, sql: &str, params: &[Value]) -> DjangoResult<Vec<Row>> {
            self.executed
                .lock()
                .await
                .push((sql.to_string(), params.to_vec()));
            Ok(self.rows.lock().await.clone())
        }

        async fn query_one(&self, sql: &str, params: &[Value]) -> DjangoResult<Row> {
            self.executed
                .lock()
                .await
                .push((sql.to_string(), params.to_vec()));
            let rows = self.rows.lock().await;
            rows.first()
                .cloned()
                .ok_or_else(|| DjangoError::DoesNotExist("no rows".to_string()))
        }
    }

    #[tokio::test]
    async fn test_raw_query_set_execute() {
        let rows = vec![
            Row::new(
                vec!["id".to_string(), "name".to_string()],
                vec![Value::Int(1), Value::String("Alice".to_string())],
            ),
            Row::new(
                vec!["id".to_string(), "name".to_string()],
                vec![Value::Int(2), Value::String("Bob".to_string())],
            ),
        ];
        let db = MockDb::new(rows);

        let raw = RawQuerySet::<TestUser>::new(
            "SELECT * FROM test_user WHERE name LIKE $1",
            vec![Value::String("%A%".to_string())],
        );

        let results = raw.execute(&db).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, 1);
        assert_eq!(results[0].name, "Alice");
        assert_eq!(results[1].id, 2);
        assert_eq!(results[1].name, "Bob");

        // Check that parameters were passed correctly
        let executed = db.executed().await;
        assert_eq!(executed.len(), 1);
        assert_eq!(executed[0].0, "SELECT * FROM test_user WHERE name LIKE $1");
        assert_eq!(executed[0].1, vec![Value::String("%A%".to_string())]);
    }

    #[tokio::test]
    async fn test_raw_query_set_first() {
        let rows = vec![Row::new(
            vec!["id".to_string(), "name".to_string()],
            vec![Value::Int(1), Value::String("Alice".to_string())],
        )];
        let db = MockDb::new(rows);

        let raw =
            RawQuerySet::<TestUser>::new("SELECT * FROM test_user LIMIT 1", vec![]);

        let result = raw.first(&db).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "Alice");
    }

    #[tokio::test]
    async fn test_raw_query_set_first_empty() {
        let db = MockDb::new(vec![]);

        let raw = RawQuerySet::<TestUser>::new("SELECT * FROM test_user LIMIT 1", vec![]);

        let result = raw.first(&db).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_raw_query_set_with_translations() {
        let rows = vec![Row::new(
            vec!["user_id".to_string(), "user_name".to_string()],
            vec![Value::Int(1), Value::String("Alice".to_string())],
        )];
        let db = MockDb::new(rows);

        let raw = RawQuerySet::<TestUser>::new(
            "SELECT user_id, user_name FROM custom_view",
            vec![],
        )
        .translate(vec![
            ("user_id".to_string(), "id".to_string()),
            ("user_name".to_string(), "name".to_string()),
        ]);

        let results = raw.execute(&db).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 1);
        assert_eq!(results[0].name, "Alice");
    }

    #[tokio::test]
    async fn test_raw_sql_fetch_all() {
        let rows = vec![
            Row::new(vec!["count".to_string()], vec![Value::Int(42)]),
        ];
        let db = MockDb::new(rows);

        let raw = RawSql::new("SELECT COUNT(*) AS count FROM test_user", vec![]);
        let results = raw.fetch_all(&db).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].get::<i64>("count").unwrap(), 42);
    }

    #[tokio::test]
    async fn test_raw_sql_fetch_one() {
        let rows = vec![
            Row::new(vec!["val".to_string()], vec![Value::Int(1)]),
        ];
        let db = MockDb::new(rows);

        let raw = RawSql::new("SELECT 1 AS val", vec![]);
        let result = raw.fetch_one(&db).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().get::<i64>("val").unwrap(), 1);
    }

    #[tokio::test]
    async fn test_raw_sql_fetch_one_empty() {
        let db = MockDb::new(vec![]);

        let raw = RawSql::new("SELECT 1 AS val WHERE 1=0", vec![]);
        let result = raw.fetch_one(&db).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_raw_sql_execute() {
        let db = MockDb::new(vec![]);

        let raw = RawSql::new(
            "UPDATE test_user SET name = $1 WHERE id = $2",
            vec![Value::String("Updated".to_string()), Value::Int(1)],
        );
        let count = raw.execute(&db).await.unwrap();
        assert_eq!(count, 1);

        let executed = db.executed().await;
        assert_eq!(
            executed[0].0,
            "UPDATE test_user SET name = $1 WHERE id = $2"
        );
        assert_eq!(executed[0].1.len(), 2);
    }

    #[tokio::test]
    async fn test_raw_sql_execute_many() {
        let db = MockDb::new(vec![]);

        let stmts = vec![
            RawSql::new("INSERT INTO t VALUES ($1)", vec![Value::Int(1)]),
            RawSql::new("INSERT INTO t VALUES ($1)", vec![Value::Int(2)]),
            RawSql::new("INSERT INTO t VALUES ($1)", vec![Value::Int(3)]),
        ];

        let total = RawSql::execute_many(&stmts, &db).await.unwrap();
        assert_eq!(total, 3);

        let executed = db.executed().await;
        assert_eq!(executed.len(), 3);
    }

    #[test]
    fn test_raw_query_set_accessors() {
        let raw = RawQuerySet::<TestUser>::new(
            "SELECT * FROM test_user WHERE id = $1",
            vec![Value::Int(1)],
        );
        assert_eq!(raw.sql(), "SELECT * FROM test_user WHERE id = $1");
        assert_eq!(raw.params(), &[Value::Int(1)]);
    }

    #[test]
    fn test_raw_sql_accessors() {
        let raw = RawSql::new("SELECT 1", vec![]);
        assert_eq!(raw.sql(), "SELECT 1");
        assert!(raw.params().is_empty());
    }

    #[test]
    fn test_validate_raw_sql_safe() {
        assert!(validate_raw_sql("SELECT * FROM users WHERE id = $1").is_ok());
        assert!(validate_raw_sql("INSERT INTO users (name) VALUES (?)").is_ok());
    }

    #[test]
    fn test_validate_raw_sql_suspicious() {
        assert!(validate_raw_sql("SELECT * FROM users WHERE name = ''; DROP TABLE users").is_err());
    }

    #[tokio::test]
    async fn test_raw_query_parameterized() {
        let rows = vec![Row::new(
            vec!["id".to_string(), "name".to_string()],
            vec![Value::Int(1), Value::String("Alice".to_string())],
        )];
        let db = MockDb::new(rows);

        // Ensure parameters are passed through, not interpolated
        let raw = RawQuerySet::<TestUser>::new(
            "SELECT * FROM test_user WHERE id = $1 AND name = $2",
            vec![Value::Int(1), Value::String("Alice".to_string())],
        );

        let results = raw.execute(&db).await.unwrap();
        assert_eq!(results.len(), 1);

        let executed = db.executed().await;
        assert_eq!(executed[0].1.len(), 2);
        assert_eq!(executed[0].1[0], Value::Int(1));
        assert_eq!(executed[0].1[1], Value::String("Alice".to_string()));
    }
}
