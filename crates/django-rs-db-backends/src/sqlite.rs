//! SQLite database backend using `rusqlite`.
//!
//! This module provides the [`SqliteBackend`] which implements the
//! [`DatabaseBackend`](crate::base::DatabaseBackend) trait using `rusqlite`
//! wrapped in `tokio::task::spawn_blocking` for async compatibility.
//!
//! Features:
//! - WAL mode enabled by default for better concurrent read performance
//! - In-memory database support via `:memory:` path (great for testing)
//! - Simple `Mutex`-based concurrency control

use crate::base::{DatabaseBackend, Transaction};
use django_rs_core::DjangoError;
use django_rs_db::query::compiler::{DatabaseBackendType, SqlCompiler};
use django_rs_db::value::Value;
use django_rs_db::Row;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A SQLite database backend.
///
/// Uses `rusqlite` for database access with a `Mutex`-based concurrency
/// model. All operations are run via `tokio::task::spawn_blocking` to
/// avoid blocking the async runtime.
pub struct SqliteBackend {
    /// The path to the database file (or ":memory:").
    path: PathBuf,
    /// The connection, guarded by an async mutex.
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl SqliteBackend {
    /// Opens a new SQLite database at the given path.
    ///
    /// If the path is `:memory:`, an in-memory database is created.
    /// WAL journal mode is enabled by default for file-based databases.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, DjangoError> {
        let path = path.into();
        let conn = if path.to_str() == Some(":memory:") {
            rusqlite::Connection::open_in_memory()
        } else {
            rusqlite::Connection::open(&path)
        }
        .map_err(|e| DjangoError::OperationalError(format!("SQLite open failed: {e}")))?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| {
                DjangoError::OperationalError(format!("Failed to set pragmas: {e}"))
            })?;

        Ok(Self {
            path,
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens an in-memory database (convenience constructor).
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be created.
    pub fn memory() -> Result<Self, DjangoError> {
        Self::open(":memory:")
    }

    /// Returns the database file path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Binds ORM `Value` types to a `rusqlite` statement.
    fn bind_params(
        stmt: &mut rusqlite::Statement<'_>,
        params: &[Value],
    ) -> Result<(), DjangoError> {
        for (i, param) in params.iter().enumerate() {
            let idx = i + 1;
            match param {
                Value::Null => stmt.raw_bind_parameter(idx, rusqlite::types::Null),
                Value::Bool(b) => stmt.raw_bind_parameter(idx, b),
                Value::Int(v) => stmt.raw_bind_parameter(idx, v),
                Value::Float(v) => stmt.raw_bind_parameter(idx, v),
                Value::String(s) => stmt.raw_bind_parameter(idx, s.as_str()),
                Value::Bytes(b) => stmt.raw_bind_parameter(idx, b.as_slice()),
                Value::Date(d) => stmt.raw_bind_parameter(idx, d.to_string().as_str()),
                Value::DateTime(dt) => {
                    stmt.raw_bind_parameter(idx, dt.to_string().as_str())
                }
                Value::DateTimeTz(dt) => {
                    stmt.raw_bind_parameter(idx, dt.to_string().as_str())
                }
                Value::Time(t) => stmt.raw_bind_parameter(idx, t.to_string().as_str()),
                Value::Duration(d) => {
                    stmt.raw_bind_parameter(idx, d.num_microseconds().unwrap_or(0))
                }
                Value::Uuid(u) => stmt.raw_bind_parameter(idx, u.to_string().as_str()),
                Value::Json(j) => {
                    stmt.raw_bind_parameter(idx, j.to_string().as_str())
                }
                Value::List(vals) => {
                    let json = serde_json::to_string(
                        &vals
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or_default();
                    stmt.raw_bind_parameter(idx, json.as_str())
                }
            }
            .map_err(|e| DjangoError::DatabaseError(format!("Bind error: {e}")))?;
        }
        Ok(())
    }

    /// Converts a `rusqlite::Row` to our generic `Row`.
    fn convert_row(
        sqlite_row: &rusqlite::Row<'_>,
        column_names: &[String],
    ) -> Result<Row, DjangoError> {
        let values: Vec<Value> = column_names
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let val_ref = sqlite_row.get_ref(i).unwrap_or(rusqlite::types::ValueRef::Null);
                match val_ref {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(v) => Value::Int(v),
                    rusqlite::types::ValueRef::Real(v) => Value::Float(v),
                    rusqlite::types::ValueRef::Text(b) => {
                        Value::String(String::from_utf8_lossy(b).to_string())
                    }
                    rusqlite::types::ValueRef::Blob(b) => Value::Bytes(b.to_vec()),
                }
            })
            .collect();

        Ok(Row::new(column_names.to_vec(), values))
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for SqliteBackend {
    fn vendor(&self) -> &str {
        "sqlite"
    }

    fn backend_type(&self) -> DatabaseBackendType {
        DatabaseBackendType::SQLite
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64, DjangoError> {
        let conn = self.conn.clone();
        let sql = sql.to_string();
        let params = params.to_vec();

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| DjangoError::DatabaseError(format!("{e}")))?;
            Self::bind_params(&mut stmt, &params)?;
            let count = stmt
                .raw_execute()
                .map_err(|e| DjangoError::DatabaseError(format!("{e}")))?;
            Ok(count as u64)
        })
        .await
        .map_err(|e| DjangoError::DatabaseError(format!("Task join error: {e}")))?
    }

    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DjangoError> {
        let conn = self.conn.clone();
        let sql = sql.to_string();
        let params = params.to_vec();

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| DjangoError::DatabaseError(format!("{e}")))?;

            let column_names: Vec<String> = stmt
                .column_names()
                .into_iter()
                .map(String::from)
                .collect();

            Self::bind_params(&mut stmt, &params)?;

            let mut raw_rows = stmt
                .raw_query();

            let mut rows = Vec::new();
            while let Some(row) = raw_rows
                .next()
                .map_err(|e| DjangoError::DatabaseError(format!("{e}")))?
            {
                rows.push(Self::convert_row(row, &column_names)?);
            }

            Ok(rows)
        })
        .await
        .map_err(|e| DjangoError::DatabaseError(format!("Task join error: {e}")))?
    }

    async fn query_one(&self, sql: &str, params: &[Value]) -> Result<Row, DjangoError> {
        let rows = self.query(sql, params).await?;
        match rows.len() {
            0 => Err(DjangoError::DoesNotExist("No rows returned".to_string())),
            1 => Ok(rows.into_iter().next().unwrap()),
            _ => Err(DjangoError::MultipleObjectsReturned(format!(
                "Expected 1 row, got {}",
                rows.len()
            ))),
        }
    }

    async fn begin_transaction(&self) -> Result<Transaction, DjangoError> {
        self.execute("BEGIN", &[]).await?;
        Ok(Transaction::new(Box::new(())))
    }

    async fn commit(&self) -> Result<(), DjangoError> {
        self.execute("COMMIT", &[]).await?;
        Ok(())
    }

    async fn rollback(&self) -> Result<(), DjangoError> {
        self.execute("ROLLBACK", &[]).await?;
        Ok(())
    }

    fn compiler(&self) -> SqlCompiler {
        SqlCompiler::new(DatabaseBackendType::SQLite)
    }
}

#[async_trait::async_trait]
impl django_rs_db::DbExecutor for SqliteBackend {
    fn backend_type(&self) -> DatabaseBackendType {
        DatabaseBackendType::SQLite
    }

    async fn execute_sql(&self, sql: &str, params: &[Value]) -> Result<u64, DjangoError> {
        self.execute(sql, params).await
    }

    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DjangoError> {
        DatabaseBackend::query(self, sql, params).await
    }

    async fn query_one(&self, sql: &str, params: &[Value]) -> Result<Row, DjangoError> {
        DatabaseBackend::query_one(self, sql, params).await
    }

    async fn insert_returning_id(
        &self,
        sql: &str,
        params: &[Value],
    ) -> Result<Value, DjangoError> {
        let conn = self.conn.clone();
        let sql = sql.to_string();
        let params = params.to_vec();

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| DjangoError::DatabaseError(format!("{e}")))?;
            Self::bind_params(&mut stmt, &params)?;
            stmt.raw_execute()
                .map_err(|e| DjangoError::DatabaseError(format!("{e}")))?;
            let id = conn.last_insert_rowid();
            Ok(Value::Int(id))
        })
        .await
        .map_err(|e| DjangoError::DatabaseError(format!("Task join error: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use django_rs_db::query::compiler::DatabaseBackendType;

    #[tokio::test]
    async fn test_sqlite_memory_open() {
        let backend = SqliteBackend::memory().unwrap();
        assert_eq!(backend.vendor(), "sqlite");
        assert_eq!(backend.backend_type(), DatabaseBackendType::SQLite);
    }

    #[tokio::test]
    async fn test_sqlite_create_table() {
        let backend = SqliteBackend::memory().unwrap();
        let result = backend
            .execute(
                "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
                &[],
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sqlite_insert_and_query() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)",
                &[],
            )
            .await
            .unwrap();

        backend
            .execute(
                "INSERT INTO users (name, age) VALUES (?, ?)",
                &[Value::from("Alice"), Value::from(30)],
            )
            .await
            .unwrap();

        let rows = backend
            .query("SELECT id, name, age FROM users", &[])
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get::<String>("name").unwrap(), "Alice");
        assert_eq!(rows[0].get::<i64>("age").unwrap(), 30);
    }

    #[tokio::test]
    async fn test_sqlite_query_one() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute("CREATE TABLE test (id INTEGER PRIMARY KEY, val TEXT)", &[])
            .await
            .unwrap();

        backend
            .execute(
                "INSERT INTO test (val) VALUES (?)",
                &[Value::from("hello")],
            )
            .await
            .unwrap();

        let row = backend
            .query_one("SELECT val FROM test WHERE id = ?", &[Value::from(1)])
            .await
            .unwrap();

        assert_eq!(row.get::<String>("val").unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_sqlite_query_one_not_found() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute("CREATE TABLE test (id INTEGER PRIMARY KEY)", &[])
            .await
            .unwrap();

        let result = backend
            .query_one("SELECT id FROM test WHERE id = ?", &[Value::from(999)])
            .await;

        assert!(result.is_err());
        assert!(matches!(result, Err(DjangoError::DoesNotExist(_))));
    }

    #[tokio::test]
    async fn test_sqlite_query_one_multiple() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute("CREATE TABLE test (id INTEGER PRIMARY KEY, val TEXT)", &[])
            .await
            .unwrap();

        backend
            .execute("INSERT INTO test (val) VALUES (?)", &[Value::from("a")])
            .await
            .unwrap();
        backend
            .execute("INSERT INTO test (val) VALUES (?)", &[Value::from("b")])
            .await
            .unwrap();

        let result = backend.query_one("SELECT val FROM test", &[]).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(DjangoError::MultipleObjectsReturned(_))));
    }

    #[tokio::test]
    async fn test_sqlite_null_handling() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, bio TEXT)",
                &[],
            )
            .await
            .unwrap();

        backend
            .execute(
                "INSERT INTO test (name, bio) VALUES (?, ?)",
                &[Value::from("Alice"), Value::Null],
            )
            .await
            .unwrap();

        let row = backend
            .query_one("SELECT name, bio FROM test WHERE id = ?", &[Value::from(1)])
            .await
            .unwrap();

        assert_eq!(row.get::<String>("name").unwrap(), "Alice");
        let bio: Option<String> = row.get("bio").unwrap();
        assert_eq!(bio, None);
    }

    #[tokio::test]
    async fn test_sqlite_float_handling() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE test (id INTEGER PRIMARY KEY, price REAL)",
                &[],
            )
            .await
            .unwrap();

        backend
            .execute(
                "INSERT INTO test (price) VALUES (?)",
                &[Value::from(19.99)],
            )
            .await
            .unwrap();

        let row = backend
            .query_one("SELECT price FROM test WHERE id = ?", &[Value::from(1)])
            .await
            .unwrap();

        let price: f64 = row.get("price").unwrap();
        assert!((price - 19.99).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_sqlite_blob_handling() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE test (id INTEGER PRIMARY KEY, data BLOB)",
                &[],
            )
            .await
            .unwrap();

        let blob = vec![0xDE_u8, 0xAD, 0xBE, 0xEF];
        backend
            .execute(
                "INSERT INTO test (data) VALUES (?)",
                &[Value::Bytes(blob.clone())],
            )
            .await
            .unwrap();

        let rows = backend
            .query("SELECT data FROM test", &[])
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        // The blob should round-trip correctly
        if let Value::Bytes(data) = rows[0].get_value("data").unwrap() {
            assert_eq!(data, &blob);
        } else {
            panic!("Expected Bytes value");
        }
    }

    #[tokio::test]
    async fn test_sqlite_multiple_inserts() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                &[],
            )
            .await
            .unwrap();

        for name in &["Alice", "Bob", "Charlie"] {
            backend
                .execute(
                    "INSERT INTO users (name) VALUES (?)",
                    &[Value::from(*name)],
                )
                .await
                .unwrap();
        }

        let rows = backend
            .query("SELECT name FROM users ORDER BY name", &[])
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].get::<String>("name").unwrap(), "Alice");
        assert_eq!(rows[1].get::<String>("name").unwrap(), "Bob");
        assert_eq!(rows[2].get::<String>("name").unwrap(), "Charlie");
    }

    #[tokio::test]
    async fn test_sqlite_update() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                &[],
            )
            .await
            .unwrap();

        backend
            .execute(
                "INSERT INTO users (name) VALUES (?)",
                &[Value::from("Alice")],
            )
            .await
            .unwrap();

        let affected = backend
            .execute(
                "UPDATE users SET name = ? WHERE id = ?",
                &[Value::from("Alice Updated"), Value::from(1)],
            )
            .await
            .unwrap();

        assert_eq!(affected, 1);

        let row = backend
            .query_one("SELECT name FROM users WHERE id = ?", &[Value::from(1)])
            .await
            .unwrap();
        assert_eq!(row.get::<String>("name").unwrap(), "Alice Updated");
    }

    #[tokio::test]
    async fn test_sqlite_delete() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                &[],
            )
            .await
            .unwrap();

        backend
            .execute(
                "INSERT INTO users (name) VALUES (?)",
                &[Value::from("Alice")],
            )
            .await
            .unwrap();

        let affected = backend
            .execute("DELETE FROM users WHERE id = ?", &[Value::from(1)])
            .await
            .unwrap();

        assert_eq!(affected, 1);

        let rows = backend.query("SELECT * FROM users", &[]).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_compiler() {
        let backend = SqliteBackend::memory().unwrap();
        let compiler = backend.compiler();
        let (sql, _) = compiler.compile_insert("test", &[("name", Value::from("Alice"))]);
        assert!(sql.contains("?"));
        assert!(!sql.contains('$'));
    }

    #[tokio::test]
    async fn test_sqlite_path() {
        let backend = SqliteBackend::memory().unwrap();
        assert_eq!(backend.path().to_str().unwrap(), ":memory:");
    }

    #[tokio::test]
    async fn test_sqlite_compiled_sql_execution() {
        // Test that SQL generated by the compiler actually works in SQLite
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL)",
                &[],
            )
            .await
            .unwrap();

        let compiler = backend.compiler();

        // Test INSERT
        let (sql, params) = compiler.compile_insert(
            "products",
            &[
                ("name", Value::from("Widget")),
                ("price", Value::from(9.99)),
            ],
        );
        backend.execute(&sql, &params).await.unwrap();

        // Test SELECT
        let (sql, params) = compiler.compile_select(&django_rs_db::Query::new("products"));
        let rows = backend.query(&sql, &params).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get::<String>("name").unwrap(), "Widget");
    }

    #[tokio::test]
    async fn test_sqlite_compiled_select_with_where() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)",
                &[],
            )
            .await
            .unwrap();

        // Insert test data
        for (name, age) in &[("Alice", 30_i32), ("Bob", 25_i32), ("Charlie", 35_i32)] {
            backend
                .execute(
                    "INSERT INTO users (name, age) VALUES (?, ?)",
                    &[Value::from(*name), Value::from(*age)],
                )
                .await
                .unwrap();
        }

        let compiler = backend.compiler();

        // Build a filtered query
        let mut query = django_rs_db::Query::new("users");
        query.where_clause = Some(django_rs_db::WhereNode::Condition {
            column: "age".to_string(),
            lookup: django_rs_db::Lookup::Gt(Value::from(28)),
        });
        query.order_by = vec![django_rs_db::OrderBy::asc("name")];

        let (sql, params) = compiler.compile_select(&query);
        let rows = backend.query(&sql, &params).await.unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get::<String>("name").unwrap(), "Alice");
        assert_eq!(rows[1].get::<String>("name").unwrap(), "Charlie");
    }

    #[tokio::test]
    async fn test_sqlite_compiled_update() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                &[],
            )
            .await
            .unwrap();
        backend
            .execute(
                "INSERT INTO users (name) VALUES (?)",
                &[Value::from("Alice")],
            )
            .await
            .unwrap();

        let compiler = backend.compiler();
        let where_clause = django_rs_db::WhereNode::Condition {
            column: "id".to_string(),
            lookup: django_rs_db::Lookup::Exact(Value::from(1)),
        };
        let (sql, params) =
            compiler.compile_update("users", &[("name", Value::from("Updated"))], &where_clause);

        let affected = backend.execute(&sql, &params).await.unwrap();
        assert_eq!(affected, 1);

        let row = backend
            .query_one("SELECT name FROM users WHERE id = 1", &[])
            .await
            .unwrap();
        assert_eq!(row.get::<String>("name").unwrap(), "Updated");
    }

    #[tokio::test]
    async fn test_sqlite_compiled_delete() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                &[],
            )
            .await
            .unwrap();
        backend
            .execute(
                "INSERT INTO users (name) VALUES (?)",
                &[Value::from("Alice")],
            )
            .await
            .unwrap();

        let compiler = backend.compiler();
        let where_clause = django_rs_db::WhereNode::Condition {
            column: "id".to_string(),
            lookup: django_rs_db::Lookup::Exact(Value::from(1)),
        };
        let (sql, params) = compiler.compile_delete("users", &where_clause);
        let affected = backend.execute(&sql, &params).await.unwrap();
        assert_eq!(affected, 1);

        let rows = backend.query("SELECT * FROM users", &[]).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_transaction_commit() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute("CREATE TABLE test (id INTEGER PRIMARY KEY, val TEXT)", &[])
            .await
            .unwrap();

        // Begin transaction
        let _txn = backend.begin_transaction().await.unwrap();
        backend
            .execute("INSERT INTO test (val) VALUES (?)", &[Value::from("hello")])
            .await
            .unwrap();
        backend.execute("COMMIT", &[]).await.unwrap();

        let rows = backend.query("SELECT val FROM test", &[]).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn test_sqlite_empty_query() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute("CREATE TABLE test (id INTEGER PRIMARY KEY)", &[])
            .await
            .unwrap();

        let rows = backend.query("SELECT * FROM test", &[]).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_row_get_by_index() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE test (id INTEGER PRIMARY KEY, a TEXT, b TEXT)",
                &[],
            )
            .await
            .unwrap();
        backend
            .execute(
                "INSERT INTO test (a, b) VALUES (?, ?)",
                &[Value::from("first"), Value::from("second")],
            )
            .await
            .unwrap();

        let row = backend
            .query_one("SELECT a, b FROM test", &[])
            .await
            .unwrap();
        assert_eq!(row.get_by_index::<String>(0).unwrap(), "first");
        assert_eq!(row.get_by_index::<String>(1).unwrap(), "second");
    }

    #[tokio::test]
    async fn test_sqlite_boolean_values() {
        let backend = SqliteBackend::memory().unwrap();
        backend
            .execute(
                "CREATE TABLE flags (id INTEGER PRIMARY KEY, active INTEGER)",
                &[],
            )
            .await
            .unwrap();

        backend
            .execute(
                "INSERT INTO flags (active) VALUES (?)",
                &[Value::Bool(true)],
            )
            .await
            .unwrap();

        let row = backend
            .query_one("SELECT active FROM flags WHERE id = ?", &[Value::from(1)])
            .await
            .unwrap();
        // SQLite stores booleans as integers
        assert_eq!(row.get::<i64>("active").unwrap(), 1);
    }
}
