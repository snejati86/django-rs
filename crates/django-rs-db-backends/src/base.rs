//! Base database backend trait and common types.
//!
//! This module defines the [`DatabaseBackend`] trait that all backend
//! implementations must satisfy, along with the [`Transaction`] wrapper
//! for managing database transactions.

use django_rs_core::DjangoError;
use django_rs_db::query::compiler::{DatabaseBackendType, SqlCompiler};
use django_rs_db::value::Value;
use django_rs_db::Row;

/// A database transaction wrapper.
///
/// Transactions are obtained from [`DatabaseBackend::begin_transaction`] and
/// must be explicitly committed or rolled back. Dropping a transaction without
/// committing will roll it back automatically.
pub struct Transaction {
    /// Whether this transaction has been committed.
    pub committed: bool,
    /// An opaque handle to the backend-specific transaction state.
    /// We use a boxed trait object so each backend can store its own type.
    _inner: Box<dyn std::any::Any + Send>,
}

impl Transaction {
    /// Creates a new transaction wrapper.
    pub fn new(inner: Box<dyn std::any::Any + Send>) -> Self {
        Self {
            committed: false,
            _inner: inner,
        }
    }

    /// Marks this transaction as committed.
    pub fn set_committed(&mut self) {
        self.committed = true;
    }
}

/// The core trait for database backends.
///
/// Each database engine (PostgreSQL, SQLite, MySQL) implements this trait to
/// provide a uniform interface for executing SQL, managing transactions, and
/// obtaining a SQL compiler configured for the backend's dialect.
///
/// All methods are async because database operations are inherently I/O-bound.
/// Even backends that use synchronous drivers (like `rusqlite`) wrap operations
/// in `spawn_blocking` to maintain the async interface.
#[async_trait::async_trait]
pub trait DatabaseBackend: Send + Sync {
    /// Returns the vendor name (e.g., "postgresql", "sqlite", "mysql").
    fn vendor(&self) -> &str;

    /// Returns the backend type enum for use with the SQL compiler.
    fn backend_type(&self) -> DatabaseBackendType;

    /// Executes a SQL statement that does not return rows.
    ///
    /// Returns the number of rows affected.
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64, DjangoError>;

    /// Executes a SQL query and returns all result rows.
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DjangoError>;

    /// Executes a SQL query and returns exactly one row.
    ///
    /// Returns [`DjangoError::DoesNotExist`] if no rows are returned, or
    /// [`DjangoError::MultipleObjectsReturned`] if more than one row is returned.
    async fn query_one(&self, sql: &str, params: &[Value]) -> Result<Row, DjangoError>;

    /// Begins a new database transaction.
    async fn begin_transaction(&self) -> Result<Transaction, DjangoError>;

    /// Commits the current transaction.
    async fn commit(&self) -> Result<(), DjangoError>;

    /// Rolls back the current transaction.
    async fn rollback(&self) -> Result<(), DjangoError>;

    /// Returns a SQL compiler configured for this backend's dialect.
    fn compiler(&self) -> SqlCompiler;
}

/// Configuration for connecting to a database.
///
/// This struct holds the connection parameters needed to establish a connection
/// to any supported database backend.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// The backend type.
    pub backend: DatabaseBackendType,
    /// The database name or file path.
    pub name: String,
    /// The database host (for network-based backends).
    pub host: Option<String>,
    /// The database port.
    pub port: Option<u16>,
    /// The database user.
    pub user: Option<String>,
    /// The database password.
    pub password: Option<String>,
    /// Additional connection options.
    pub options: std::collections::HashMap<String, String>,
}

impl DatabaseConfig {
    /// Creates a configuration for an in-memory SQLite database.
    pub fn sqlite_memory() -> Self {
        Self {
            backend: DatabaseBackendType::SQLite,
            name: ":memory:".to_string(),
            host: None,
            port: None,
            user: None,
            password: None,
            options: std::collections::HashMap::new(),
        }
    }

    /// Creates a configuration for a SQLite file database.
    pub fn sqlite_file(path: impl Into<String>) -> Self {
        Self {
            backend: DatabaseBackendType::SQLite,
            name: path.into(),
            host: None,
            port: None,
            user: None,
            password: None,
            options: std::collections::HashMap::new(),
        }
    }

    /// Creates a configuration for a PostgreSQL database.
    pub fn postgres(
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            backend: DatabaseBackendType::PostgreSQL,
            name: name.into(),
            host: Some(host.into()),
            port: Some(port),
            user: Some(user.into()),
            password: Some(password.into()),
            options: std::collections::HashMap::new(),
        }
    }

    /// Creates a configuration for a MySQL database.
    pub fn mysql(
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            backend: DatabaseBackendType::MySQL,
            name: name.into(),
            host: Some(host.into()),
            port: Some(port),
            user: Some(user.into()),
            password: Some(password.into()),
            options: std::collections::HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_new() {
        let txn = Transaction::new(Box::new(()));
        assert!(!txn.committed);
    }

    #[test]
    fn test_transaction_committed() {
        let mut txn = Transaction::new(Box::new(()));
        txn.set_committed();
        assert!(txn.committed);
    }

    #[test]
    fn test_database_config_sqlite_memory() {
        let cfg = DatabaseConfig::sqlite_memory();
        assert_eq!(cfg.backend, DatabaseBackendType::SQLite);
        assert_eq!(cfg.name, ":memory:");
        assert!(cfg.host.is_none());
    }

    #[test]
    fn test_database_config_sqlite_file() {
        let cfg = DatabaseConfig::sqlite_file("/tmp/test.db");
        assert_eq!(cfg.backend, DatabaseBackendType::SQLite);
        assert_eq!(cfg.name, "/tmp/test.db");
    }

    #[test]
    fn test_database_config_postgres() {
        let cfg = DatabaseConfig::postgres("mydb", "localhost", 5432, "user", "pass");
        assert_eq!(cfg.backend, DatabaseBackendType::PostgreSQL);
        assert_eq!(cfg.name, "mydb");
        assert_eq!(cfg.host.as_deref(), Some("localhost"));
        assert_eq!(cfg.port, Some(5432));
        assert_eq!(cfg.user.as_deref(), Some("user"));
        assert_eq!(cfg.password.as_deref(), Some("pass"));
    }

    #[test]
    fn test_database_config_mysql() {
        let cfg = DatabaseConfig::mysql("mydb", "localhost", 3306, "root", "secret");
        assert_eq!(cfg.backend, DatabaseBackendType::MySQL);
        assert_eq!(cfg.port, Some(3306));
    }
}
