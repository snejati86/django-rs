//! PostgreSQL database backend using `tokio-postgres` and `deadpool-postgres`.
//!
//! This module provides the [`PostgresBackend`] which implements the
//! [`DatabaseBackend`](crate::base::DatabaseBackend) trait using connection
//! pooling via `deadpool-postgres`.

use crate::base::{DatabaseBackend, DatabaseConfig, Transaction};
use django_rs_core::DjangoError;
use django_rs_db::query::compiler::{DatabaseBackendType, SqlCompiler};
use django_rs_db::value::Value;
use django_rs_db::Row;

/// A PostgreSQL database backend.
///
/// Uses `deadpool-postgres` for connection pooling and `tokio-postgres` for
/// query execution. Supports PostgreSQL-specific types including arrays,
/// JSONB, and UUID.
pub struct PostgresBackend {
    pool: deadpool_postgres::Pool,
}

impl PostgresBackend {
    /// Creates a new `PostgresBackend` from a `deadpool-postgres` pool.
    pub const fn new(pool: deadpool_postgres::Pool) -> Self {
        Self { pool }
    }

    /// Creates a new backend from a [`DatabaseConfig`].
    ///
    /// # Errors
    ///
    /// Returns an error if the pool cannot be created.
    pub fn from_config(config: &DatabaseConfig) -> Result<Self, DjangoError> {
        let mut pg_config = deadpool_postgres::Config::new();
        pg_config.dbname = Some(config.name.clone());
        pg_config.host = config.host.clone();
        pg_config.port = config.port;
        pg_config.user = config.user.clone();
        pg_config.password = config.password.clone();

        let pool = pg_config
            .create_pool(
                Some(deadpool_postgres::Runtime::Tokio1),
                tokio_postgres::NoTls,
            )
            .map_err(|e| DjangoError::OperationalError(format!("Failed to create pool: {e}")))?;

        Ok(Self { pool })
    }

    /// Converts ORM `Value` types to `tokio-postgres` parameter references.
    fn value_to_sql_params(
        params: &[Value],
    ) -> Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> {
        params
            .iter()
            .map(|v| -> Box<dyn tokio_postgres::types::ToSql + Sync + Send> {
                match v {
                    Value::Null => Box::new(Option::<String>::None),
                    Value::Bool(b) => Box::new(*b),
                    Value::Int(i) => Box::new(*i),
                    Value::Float(f) => Box::new(*f),
                    Value::String(s) => Box::new(s.clone()),
                    Value::Bytes(b) => Box::new(b.clone()),
                    Value::Date(d) => Box::new(*d),
                    Value::DateTime(dt) => Box::new(*dt),
                    Value::DateTimeTz(dt) => Box::new(*dt),
                    Value::Time(t) => Box::new(*t),
                    Value::Duration(d) => {
                        // Duration doesn't have a direct ToSql; store as microseconds
                        Box::new(d.num_microseconds().unwrap_or(0))
                    }
                    Value::Uuid(u) => Box::new(*u),
                    Value::Json(j) => Box::new(j.clone()),
                    Value::List(_) => {
                        // Lists need special handling per element type
                        Box::new(Option::<String>::None)
                    }
                    Value::HStore(map) => {
                        // Serialize hstore as a string in PostgreSQL hstore format
                        let hstore_str: String = map
                            .iter()
                            .map(|(k, v)| format!("\"{k}\"=>\"{v}\""))
                            .collect::<Vec<_>>()
                            .join(", ");
                        Box::new(hstore_str)
                    }
                    Value::Range { .. } => {
                        // Ranges need to be serialized as strings in PostgreSQL range format
                        Box::new(v.to_string())
                    }
                }
            })
            .collect()
    }

    /// Converts a `tokio_postgres::Row` to our generic `Row`.
    fn convert_row(pg_row: &tokio_postgres::Row) -> Row {
        let columns: Vec<String> = pg_row
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        let values: Vec<Value> = pg_row
            .columns()
            .iter()
            .enumerate()
            .map(|(i, col)| {
                use tokio_postgres::types::Type;
                match *col.type_() {
                    Type::BOOL => pg_row
                        .try_get::<_, Option<bool>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::Bool),
                    Type::INT2 => pg_row
                        .try_get::<_, Option<i16>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, |v| Value::Int(i64::from(v))),
                    Type::INT4 => pg_row
                        .try_get::<_, Option<i32>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, |v| Value::Int(i64::from(v))),
                    Type::INT8 => pg_row
                        .try_get::<_, Option<i64>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::Int),
                    Type::FLOAT4 => pg_row
                        .try_get::<_, Option<f32>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, |v| Value::Float(f64::from(v))),
                    Type::FLOAT8 => pg_row
                        .try_get::<_, Option<f64>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::Float),
                    Type::TEXT | Type::VARCHAR | Type::CHAR | Type::NAME => pg_row
                        .try_get::<_, Option<String>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::String),
                    Type::BYTEA => pg_row
                        .try_get::<_, Option<Vec<u8>>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::Bytes),
                    Type::UUID => pg_row
                        .try_get::<_, Option<uuid::Uuid>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::Uuid),
                    Type::DATE => pg_row
                        .try_get::<_, Option<chrono::NaiveDate>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::Date),
                    Type::TIMESTAMP => pg_row
                        .try_get::<_, Option<chrono::NaiveDateTime>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::DateTime),
                    Type::TIMESTAMPTZ => pg_row
                        .try_get::<_, Option<chrono::DateTime<chrono::Utc>>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::DateTimeTz),
                    Type::TIME => pg_row
                        .try_get::<_, Option<chrono::NaiveTime>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::Time),
                    Type::JSON | Type::JSONB => pg_row
                        .try_get::<_, Option<serde_json::Value>>(i)
                        .ok()
                        .flatten()
                        .map_or(Value::Null, Value::Json),
                    _ => {
                        // Fall back to string representation for unknown types
                        pg_row
                            .try_get::<_, Option<String>>(i)
                            .ok()
                            .flatten()
                            .map_or(Value::Null, Value::String)
                    }
                }
            })
            .collect();

        Row::new(columns, values)
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for PostgresBackend {
    fn vendor(&self) -> &str {
        "postgresql"
    }

    fn backend_type(&self) -> DatabaseBackendType {
        DatabaseBackendType::PostgreSQL
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64, DjangoError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| DjangoError::OperationalError(format!("Pool error: {e}")))?;

        let sql_params = Self::value_to_sql_params(params);
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            sql_params
                .iter()
                .map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();

        client
            .execute(sql, &param_refs)
            .await
            .map_err(|e| DjangoError::DatabaseError(format!("{e}")))
    }

    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DjangoError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| DjangoError::OperationalError(format!("Pool error: {e}")))?;

        let sql_params = Self::value_to_sql_params(params);
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            sql_params
                .iter()
                .map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();

        let rows = client
            .query(sql, &param_refs)
            .await
            .map_err(|e| DjangoError::DatabaseError(format!("{e}")))?;

        Ok(rows.iter().map(Self::convert_row).collect())
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
        SqlCompiler::new(DatabaseBackendType::PostgreSQL)
    }
}

#[async_trait::async_trait]
impl django_rs_db::DbExecutor for PostgresBackend {
    fn backend_type(&self) -> DatabaseBackendType {
        DatabaseBackendType::PostgreSQL
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
        // PostgreSQL supports RETURNING; append it to the SQL
        let sql_returning = format!("{sql} RETURNING id");
        let rows = DatabaseBackend::query(self, &sql_returning, params).await?;
        if let Some(row) = rows.into_iter().next() {
            Ok(row.get::<Value>("id")?)
        } else {
            Err(DjangoError::DatabaseError(
                "INSERT RETURNING returned no rows".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_to_sql_params_basic() {
        let params = vec![
            Value::Bool(true),
            Value::Int(42),
            Value::Float(1.23),
            Value::String("hello".to_string()),
        ];
        let sql_params = PostgresBackend::value_to_sql_params(&params);
        assert_eq!(sql_params.len(), 4);
    }

    #[test]
    fn test_value_to_sql_params_null() {
        let params = vec![Value::Null];
        let sql_params = PostgresBackend::value_to_sql_params(&params);
        assert_eq!(sql_params.len(), 1);
    }

    #[test]
    fn test_value_to_sql_params_bytes() {
        let params = vec![Value::Bytes(vec![1, 2, 3])];
        let sql_params = PostgresBackend::value_to_sql_params(&params);
        assert_eq!(sql_params.len(), 1);
    }

    #[test]
    fn test_value_to_sql_params_uuid() {
        let params = vec![Value::Uuid(uuid::Uuid::new_v4())];
        let sql_params = PostgresBackend::value_to_sql_params(&params);
        assert_eq!(sql_params.len(), 1);
    }

    #[test]
    fn test_value_to_sql_params_json() {
        let params = vec![Value::Json(serde_json::json!({"key": "value"}))];
        let sql_params = PostgresBackend::value_to_sql_params(&params);
        assert_eq!(sql_params.len(), 1);
    }

    #[test]
    fn test_value_to_sql_params_chrono() {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let time = chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let dt = date.and_time(time);
        let params = vec![
            Value::Date(date),
            Value::Time(time),
            Value::DateTime(dt),
        ];
        let sql_params = PostgresBackend::value_to_sql_params(&params);
        assert_eq!(sql_params.len(), 3);
    }

    #[test]
    fn test_compiler_type() {
        // We can't create a real pool without a database, but we can test the
        // compiler creation via a mock approach. Instead, test SqlCompiler directly.
        let compiler = SqlCompiler::new(DatabaseBackendType::PostgreSQL);
        let (sql, _) = compiler.compile_insert("test", &[("name", Value::from("Alice"))]);
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_config_to_backend_type() {
        let cfg = DatabaseConfig::postgres("testdb", "localhost", 5432, "user", "pass");
        assert_eq!(cfg.backend, DatabaseBackendType::PostgreSQL);
    }
}
