//! MySQL database backend using `mysql_async`.
//!
//! This module provides the [`MySqlBackend`] which implements the
//! [`DatabaseBackend`](crate::base::DatabaseBackend) trait using `mysql_async`
//! for fully asynchronous MySQL operations with connection pooling.

use crate::base::{DatabaseBackend, DatabaseConfig, Transaction};
use django_rs_core::DjangoError;
use django_rs_db::query::compiler::{DatabaseBackendType, SqlCompiler};
use django_rs_db::value::Value;
use django_rs_db::Row;

/// A MySQL database backend.
///
/// Uses `mysql_async` for fully asynchronous database access with built-in
/// connection pooling.
pub struct MySqlBackend {
    pool: mysql_async::Pool,
}

impl MySqlBackend {
    /// Creates a new `MySqlBackend` from a `mysql_async::Pool`.
    pub const fn new(pool: mysql_async::Pool) -> Self {
        Self { pool }
    }

    /// Creates a new backend from a connection URL.
    ///
    /// The URL should be in the format:
    /// `mysql://user:password@host:port/database`
    pub fn from_url(url: &str) -> Result<Self, DjangoError> {
        let opts = mysql_async::Opts::from_url(url)
            .map_err(|e| DjangoError::OperationalError(format!("Invalid MySQL URL: {e}")))?;
        Ok(Self {
            pool: mysql_async::Pool::new(opts),
        })
    }

    /// Creates a new backend from a [`DatabaseConfig`].
    pub fn from_config(config: &DatabaseConfig) -> Result<Self, DjangoError> {
        let host = config.host.as_deref().unwrap_or("localhost");
        let port = config.port.unwrap_or(3306);
        let user = config.user.as_deref().unwrap_or("root");
        let password = config.password.as_deref().unwrap_or("");
        let url = format!("mysql://{user}:{password}@{host}:{port}/{}", config.name);
        Self::from_url(&url)
    }

    /// Converts ORM `Value` types to `mysql_async` parameter values.
    fn values_to_params(params: &[Value]) -> Vec<mysql_async::Value> {
        params
            .iter()
            .map(|v| match v {
                Value::Null => mysql_async::Value::NULL,
                Value::Bool(b) => mysql_async::Value::from(*b),
                Value::Int(i) => mysql_async::Value::from(*i),
                Value::Float(f) => mysql_async::Value::from(*f),
                Value::String(s) => mysql_async::Value::from(s.as_str()),
                Value::Bytes(b) => mysql_async::Value::from(b.as_slice()),
                Value::Date(d) => mysql_async::Value::from(d.to_string()),
                Value::DateTime(dt) => mysql_async::Value::from(dt.to_string()),
                Value::DateTimeTz(dt) => mysql_async::Value::from(dt.to_string()),
                Value::Time(t) => mysql_async::Value::from(t.to_string()),
                Value::Duration(d) => mysql_async::Value::from(d.num_microseconds().unwrap_or(0)),
                Value::Uuid(u) => mysql_async::Value::from(u.to_string()),
                Value::Json(j) => mysql_async::Value::from(j.to_string()),
                Value::List(vals) => {
                    let json = serde_json::to_string(
                        &vals.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
                    )
                    .unwrap_or_default();
                    mysql_async::Value::from(json)
                }
                Value::HStore(map) => {
                    let json = serde_json::to_string(map).unwrap_or_default();
                    mysql_async::Value::from(json)
                }
                Value::Range { .. } => mysql_async::Value::from(v.to_string()),
            })
            .collect()
    }

    /// Converts a `mysql_async::Row` to our generic `Row`.
    fn convert_row(mysql_row: mysql_async::Row) -> Row {
        let columns: Vec<String> = mysql_row
            .columns_ref()
            .iter()
            .map(|c| c.name_str().to_string())
            .collect();

        let values: Vec<Value> = (0..columns.len())
            .map(|i| {
                let val: Option<mysql_async::Value> = mysql_row.get(i);
                match val {
                    None | Some(mysql_async::Value::NULL) => Value::Null,
                    Some(mysql_async::Value::Bytes(b)) => {
                        // Try to interpret as UTF-8 string first
                        match String::from_utf8(b.clone()) {
                            Ok(s) => Value::String(s),
                            Err(_) => Value::Bytes(b),
                        }
                    }
                    Some(mysql_async::Value::Int(i)) => Value::Int(i),
                    Some(mysql_async::Value::UInt(u)) => Value::Int(u as i64),
                    Some(mysql_async::Value::Float(f)) => Value::Float(f as f64),
                    Some(mysql_async::Value::Double(d)) => Value::Float(d),
                    Some(other) => Value::String(format!("{other:?}")),
                }
            })
            .collect();

        Row::new(columns, values)
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for MySqlBackend {
    fn vendor(&self) -> &str {
        "mysql"
    }

    fn backend_type(&self) -> DatabaseBackendType {
        DatabaseBackendType::MySQL
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64, DjangoError> {
        use mysql_async::prelude::Queryable;

        let mut conn =
            self.pool.get_conn().await.map_err(|e| {
                DjangoError::OperationalError(format!("MySQL connection error: {e}"))
            })?;

        let mysql_params = Self::values_to_params(params);
        let result = conn
            .exec_drop(sql, mysql_params)
            .await
            .map_err(|e| DjangoError::DatabaseError(format!("{e}")))?;

        let _ = result;
        Ok(conn.affected_rows())
    }

    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DjangoError> {
        use mysql_async::prelude::Queryable;

        let mut conn =
            self.pool.get_conn().await.map_err(|e| {
                DjangoError::OperationalError(format!("MySQL connection error: {e}"))
            })?;

        let mysql_params = Self::values_to_params(params);
        let rows: Vec<mysql_async::Row> = conn
            .exec(sql, mysql_params)
            .await
            .map_err(|e| DjangoError::DatabaseError(format!("{e}")))?;

        Ok(rows.into_iter().map(Self::convert_row).collect())
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
        SqlCompiler::new(DatabaseBackendType::MySQL)
    }
}

#[async_trait::async_trait]
impl django_rs_db::DbExecutor for MySqlBackend {
    fn backend_type(&self) -> DatabaseBackendType {
        DatabaseBackendType::MySQL
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

    async fn insert_returning_id(&self, sql: &str, params: &[Value]) -> Result<Value, DjangoError> {
        use mysql_async::prelude::Queryable;

        let mut conn =
            self.pool.get_conn().await.map_err(|e| {
                DjangoError::OperationalError(format!("MySQL connection error: {e}"))
            })?;

        let mysql_params = Self::values_to_params(params);
        conn.exec_drop(sql, mysql_params)
            .await
            .map_err(|e| DjangoError::DatabaseError(format!("{e}")))?;

        let last_id = conn.last_insert_id().unwrap_or(0);
        Ok(Value::Int(last_id as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_values_to_params_basic() {
        let params = vec![
            Value::Bool(true),
            Value::Int(42),
            Value::Float(1.23),
            Value::String("hello".to_string()),
        ];
        let mysql_params = MySqlBackend::values_to_params(&params);
        assert_eq!(mysql_params.len(), 4);
    }

    #[test]
    fn test_values_to_params_null() {
        let params = vec![Value::Null];
        let mysql_params = MySqlBackend::values_to_params(&params);
        assert_eq!(mysql_params.len(), 1);
        assert_eq!(mysql_params[0], mysql_async::Value::NULL);
    }

    #[test]
    fn test_values_to_params_bytes() {
        let params = vec![Value::Bytes(vec![1, 2, 3])];
        let mysql_params = MySqlBackend::values_to_params(&params);
        assert_eq!(mysql_params.len(), 1);
    }

    #[test]
    fn test_values_to_params_uuid() {
        let u = uuid::Uuid::new_v4();
        let params = vec![Value::Uuid(u)];
        let mysql_params = MySqlBackend::values_to_params(&params);
        assert_eq!(mysql_params.len(), 1);
    }

    #[test]
    fn test_values_to_params_json() {
        let params = vec![Value::Json(serde_json::json!({"key": "val"}))];
        let mysql_params = MySqlBackend::values_to_params(&params);
        assert_eq!(mysql_params.len(), 1);
    }

    #[test]
    fn test_values_to_params_chrono() {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let time = chrono::NaiveTime::from_hms_opt(10, 30, 0).unwrap();
        let params = vec![Value::Date(date), Value::Time(time)];
        let mysql_params = MySqlBackend::values_to_params(&params);
        assert_eq!(mysql_params.len(), 2);
    }

    #[test]
    fn test_compiler_type() {
        let compiler = SqlCompiler::new(DatabaseBackendType::MySQL);
        let (sql, _) = compiler.compile_insert("test", &[("name", Value::from("Alice"))]);
        assert!(sql.contains("?"));
        assert!(!sql.contains('$'));
    }

    #[test]
    fn test_config_to_backend() {
        let cfg = DatabaseConfig::mysql("testdb", "localhost", 3306, "root", "pass");
        assert_eq!(cfg.backend, DatabaseBackendType::MySQL);
        assert_eq!(cfg.port, Some(3306));
    }

    #[test]
    fn test_values_to_params_list() {
        let params = vec![Value::List(vec![Value::from(1), Value::from(2)])];
        let mysql_params = MySqlBackend::values_to_params(&params);
        assert_eq!(mysql_params.len(), 1);
    }

    #[test]
    fn test_values_to_params_duration() {
        let dur = chrono::Duration::seconds(3600);
        let params = vec![Value::Duration(dur)];
        let mysql_params = MySqlBackend::values_to_params(&params);
        assert_eq!(mysql_params.len(), 1);
    }
}
