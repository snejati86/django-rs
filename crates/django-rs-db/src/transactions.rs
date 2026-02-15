//! Transaction support for the ORM.
//!
//! This module provides transaction management including atomic blocks,
//! savepoints, on_commit callbacks, and transaction isolation levels.
//! It mirrors Django's `django.db.transaction` module.
//!
//! # Architecture
//!
//! Transactions are managed through the [`TransactionManager`] which wraps a
//! [`DbExecutor`] and provides transaction state tracking. The [`atomic()`]
//! function provides the primary entry point, accepting a closure that runs
//! within a transaction context.
//!
//! Nested calls to `atomic()` create savepoints rather than nested transactions,
//! matching Django's behavior.
//!
//! # Examples
//!
//! ```
//! use django_rs_db::transactions::{atomic, IsolationLevel};
//! use django_rs_db::executor::DbExecutor;
//!
//! // Basic usage with atomic()
//! // atomic(db, |txn| async move {
//! //     txn.execute_sql("INSERT INTO ...", &[]).await?;
//! //     Ok(())
//! // }).await?;
//! ```

use crate::executor::DbExecutor;
use crate::value::Value;
use crate::query::compiler::{DatabaseBackendType, Row};
use django_rs_core::{DjangoError, DjangoResult};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Counter for generating unique savepoint names.
static SAVEPOINT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Transaction isolation levels supported by the major database backends.
///
/// These correspond to the standard SQL isolation levels:
/// - `ReadUncommitted`: Allows dirty reads (not supported by all backends)
/// - `ReadCommitted`: Default for PostgreSQL; prevents dirty reads
/// - `RepeatableRead`: Default for MySQL InnoDB; prevents non-repeatable reads
/// - `Serializable`: Strictest isolation; prevents phantom reads
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    /// READ UNCOMMITTED - lowest isolation level.
    ReadUncommitted,
    /// READ COMMITTED - prevents dirty reads. PostgreSQL default.
    ReadCommitted,
    /// REPEATABLE READ - prevents non-repeatable reads. MySQL default.
    RepeatableRead,
    /// SERIALIZABLE - strictest isolation level.
    Serializable,
}

impl IsolationLevel {
    /// Returns the SQL syntax for setting this isolation level.
    pub fn as_sql(&self) -> &'static str {
        match self {
            Self::ReadUncommitted => "READ UNCOMMITTED",
            Self::ReadCommitted => "READ COMMITTED",
            Self::RepeatableRead => "REPEATABLE READ",
            Self::Serializable => "SERIALIZABLE",
        }
    }

    /// Returns the full SET TRANSACTION SQL for the given backend.
    pub fn set_sql(&self, backend: DatabaseBackendType) -> String {
        match backend {
            DatabaseBackendType::SQLite => {
                // SQLite doesn't support SET TRANSACTION ISOLATION LEVEL
                // but supports PRAGMA journal_mode and read_uncommitted
                match self {
                    Self::ReadUncommitted => "PRAGMA read_uncommitted = 1".to_string(),
                    _ => "PRAGMA read_uncommitted = 0".to_string(),
                }
            }
            DatabaseBackendType::PostgreSQL | DatabaseBackendType::MySQL => {
                format!("SET TRANSACTION ISOLATION LEVEL {}", self.as_sql())
            }
        }
    }
}

/// State of a savepoint within a transaction.
#[derive(Debug, Clone)]
pub struct Savepoint {
    /// The unique name of this savepoint.
    pub name: String,
    /// Whether this savepoint has been released.
    pub released: bool,
    /// Whether this savepoint has been rolled back.
    pub rolled_back: bool,
}

impl Savepoint {
    /// Creates a new savepoint with an auto-generated unique name.
    pub fn new() -> Self {
        let id = SAVEPOINT_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            name: format!("sp_{id}"),
            released: false,
            rolled_back: false,
        }
    }

    /// Creates a new savepoint with a custom name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            released: false,
            rolled_back: false,
        }
    }
}

impl Default for Savepoint {
    fn default() -> Self {
        Self::new()
    }
}

/// A list of callbacks to be executed after a transaction commits.
type OnCommitCallbacks = Vec<Box<dyn FnOnce() + Send + 'static>>;

/// Manages transaction state for a database connection.
///
/// `TransactionManager` wraps a `DbExecutor` and tracks the current
/// transaction nesting depth, savepoints, and on_commit callbacks.
pub struct TransactionManager<'a> {
    /// The underlying database executor.
    db: &'a dyn DbExecutor,
    /// Current nesting depth (0 = no transaction, 1 = outermost, 2+ = savepoint).
    depth: Arc<Mutex<u32>>,
    /// Stack of active savepoints (for nested atomic blocks).
    savepoints: Arc<Mutex<Vec<Savepoint>>>,
    /// Callbacks registered to run after the outermost transaction commits.
    on_commit_callbacks: Arc<Mutex<OnCommitCallbacks>>,
}

impl<'a> TransactionManager<'a> {
    /// Creates a new transaction manager for the given executor.
    pub fn new(db: &'a dyn DbExecutor) -> Self {
        Self {
            db,
            depth: Arc::new(Mutex::new(0)),
            savepoints: Arc::new(Mutex::new(Vec::new())),
            on_commit_callbacks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns the current transaction nesting depth.
    pub async fn depth(&self) -> u32 {
        *self.depth.lock().await
    }

    /// Returns a reference to the underlying executor.
    pub fn executor(&self) -> &dyn DbExecutor {
        self.db
    }

    /// Begins a new transaction or creates a savepoint if already in a transaction.
    ///
    /// This is called automatically by [`atomic()`] and should not normally
    /// be called directly.
    pub async fn begin(&self) -> DjangoResult<()> {
        let mut depth = self.depth.lock().await;
        if *depth == 0 {
            // Start a new transaction
            self.db.execute_sql("BEGIN", &[]).await?;
        } else {
            // Create a savepoint for nested transaction
            let sp = Savepoint::new();
            let sql = format!("SAVEPOINT {}", sp.name);
            self.db.execute_sql(&sql, &[]).await?;
            self.savepoints.lock().await.push(sp);
        }
        *depth += 1;
        Ok(())
    }

    /// Begins a transaction with a specific isolation level.
    pub async fn begin_with_isolation(&self, level: IsolationLevel) -> DjangoResult<()> {
        let mut depth = self.depth.lock().await;
        if *depth == 0 {
            let backend = self.db.backend_type();
            // For SQLite, set pragma before beginning
            if backend == DatabaseBackendType::SQLite {
                self.db.execute_sql(&level.set_sql(backend), &[]).await?;
                self.db.execute_sql("BEGIN", &[]).await?;
            } else {
                // For PG/MySQL, begin then set isolation
                self.db.execute_sql("BEGIN", &[]).await?;
                self.db.execute_sql(&level.set_sql(backend), &[]).await?;
            }
        } else {
            // Nested: savepoints inherit the outer isolation level
            let sp = Savepoint::new();
            let sql = format!("SAVEPOINT {}", sp.name);
            self.db.execute_sql(&sql, &[]).await?;
            self.savepoints.lock().await.push(sp);
        }
        *depth += 1;
        Ok(())
    }

    /// Commits the current transaction or releases the current savepoint.
    pub async fn commit(&self) -> DjangoResult<()> {
        let mut depth = self.depth.lock().await;
        if *depth == 0 {
            return Err(DjangoError::DatabaseError(
                "Cannot commit: not in a transaction".to_string(),
            ));
        }

        if *depth == 1 {
            // Commit the outermost transaction
            self.db.execute_sql("COMMIT", &[]).await?;
            *depth = 0;

            // Run on_commit callbacks
            let callbacks: Vec<Box<dyn FnOnce() + Send + 'static>> = {
                let mut cbs = self.on_commit_callbacks.lock().await;
                std::mem::take(&mut *cbs)
            };
            for cb in callbacks {
                cb();
            }
        } else {
            // Release the savepoint
            let mut savepoints = self.savepoints.lock().await;
            if let Some(mut sp) = savepoints.pop() {
                let sql = format!("RELEASE SAVEPOINT {}", sp.name);
                self.db.execute_sql(&sql, &[]).await?;
                sp.released = true;
            }
            *depth -= 1;
        }

        Ok(())
    }

    /// Rolls back the current transaction or savepoint.
    pub async fn rollback(&self) -> DjangoResult<()> {
        let mut depth = self.depth.lock().await;
        if *depth == 0 {
            return Err(DjangoError::DatabaseError(
                "Cannot rollback: not in a transaction".to_string(),
            ));
        }

        if *depth == 1 {
            // Rollback the entire transaction
            self.db.execute_sql("ROLLBACK", &[]).await?;
            *depth = 0;
            // Clear on_commit callbacks since transaction was rolled back
            self.on_commit_callbacks.lock().await.clear();
        } else {
            // Rollback to savepoint
            let mut savepoints = self.savepoints.lock().await;
            if let Some(mut sp) = savepoints.pop() {
                let sql = format!("ROLLBACK TO SAVEPOINT {}", sp.name);
                self.db.execute_sql(&sql, &[]).await?;
                sp.rolled_back = true;
            }
            *depth -= 1;
        }

        Ok(())
    }

    /// Creates a named savepoint within the current transaction.
    ///
    /// Returns the savepoint for later release or rollback.
    pub async fn create_savepoint(&self, name: impl Into<String>) -> DjangoResult<Savepoint> {
        let depth = self.depth.lock().await;
        if *depth == 0 {
            return Err(DjangoError::DatabaseError(
                "Cannot create savepoint: not in a transaction".to_string(),
            ));
        }

        let sp = Savepoint::with_name(name);
        let sql = format!("SAVEPOINT {}", sp.name);
        self.db.execute_sql(&sql, &[]).await?;
        self.savepoints.lock().await.push(sp.clone());
        Ok(sp)
    }

    /// Releases a named savepoint.
    pub async fn release_savepoint(&self, name: &str) -> DjangoResult<()> {
        let sql = format!("RELEASE SAVEPOINT {name}");
        self.db.execute_sql(&sql, &[]).await?;

        let mut savepoints = self.savepoints.lock().await;
        if let Some(sp) = savepoints.iter_mut().find(|s| s.name == name) {
            sp.released = true;
        }
        Ok(())
    }

    /// Rolls back to a named savepoint.
    pub async fn rollback_to_savepoint(&self, name: &str) -> DjangoResult<()> {
        let sql = format!("ROLLBACK TO SAVEPOINT {name}");
        self.db.execute_sql(&sql, &[]).await?;

        let mut savepoints = self.savepoints.lock().await;
        if let Some(sp) = savepoints.iter_mut().find(|s| s.name == name) {
            sp.rolled_back = true;
        }
        Ok(())
    }

    /// Registers a callback to run after the outermost transaction commits.
    ///
    /// If no transaction is active, the callback is executed immediately.
    /// If the transaction is rolled back, the callback is discarded.
    pub async fn on_commit<F>(&self, callback: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let depth = self.depth.lock().await;
        if *depth == 0 {
            // No transaction active — run immediately
            drop(depth);
            callback();
        } else {
            // Register for later execution
            self.on_commit_callbacks.lock().await.push(Box::new(callback));
        }
    }

    /// Returns the number of pending on_commit callbacks.
    pub async fn pending_callbacks(&self) -> usize {
        self.on_commit_callbacks.lock().await.len()
    }
}

#[async_trait::async_trait]
impl DbExecutor for TransactionManager<'_> {
    fn backend_type(&self) -> DatabaseBackendType {
        self.db.backend_type()
    }

    async fn execute_sql(&self, sql: &str, params: &[Value]) -> DjangoResult<u64> {
        self.db.execute_sql(sql, params).await
    }

    async fn query(&self, sql: &str, params: &[Value]) -> DjangoResult<Vec<Row>> {
        self.db.query(sql, params).await
    }

    async fn query_one(&self, sql: &str, params: &[Value]) -> DjangoResult<Row> {
        self.db.query_one(sql, params).await
    }

    async fn insert_returning_id(&self, sql: &str, params: &[Value]) -> DjangoResult<Value> {
        self.db.insert_returning_id(sql, params).await
    }
}

/// Executes a closure within a database transaction.
///
/// If the closure returns `Ok`, the transaction is committed. If it returns
/// `Err`, the transaction is rolled back. Nested calls create savepoints.
///
/// This is the primary API for transaction management, equivalent to
/// Django's `transaction.atomic()`.
///
/// # Examples
///
/// ```ignore
/// use django_rs_db::transactions::atomic;
///
/// let result = atomic(db, |txn| Box::pin(async move {
///     txn.execute_sql("INSERT INTO users (name) VALUES ($1)", &[&name]).await?;
///     Ok("created")
/// })).await?;
/// ```
pub async fn atomic<'a, F, Fut, T>(db: &'a dyn DbExecutor, f: F) -> DjangoResult<T>
where
    F: FnOnce(Arc<TransactionManager<'a>>) -> Fut,
    Fut: std::future::Future<Output = DjangoResult<T>>,
{
    let txn = Arc::new(TransactionManager::new(db));
    txn.begin().await?;

    match f(Arc::clone(&txn)).await {
        Ok(result) => {
            txn.commit().await?;
            Ok(result)
        }
        Err(e) => {
            // Attempt to rollback; if rollback fails, return the original error
            let _ = txn.rollback().await;
            Err(e)
        }
    }
}

/// Executes a closure within a transaction with a specific isolation level.
///
/// Works like [`atomic()`] but sets the transaction isolation level before
/// the first statement executes.
pub async fn atomic_with_isolation<'a, F, Fut, T>(
    db: &'a dyn DbExecutor,
    level: IsolationLevel,
    f: F,
) -> DjangoResult<T>
where
    F: FnOnce(Arc<TransactionManager<'a>>) -> Fut,
    Fut: std::future::Future<Output = DjangoResult<T>>,
{
    let txn = Arc::new(TransactionManager::new(db));
    txn.begin_with_isolation(level).await?;

    match f(Arc::clone(&txn)).await {
        Ok(result) => {
            txn.commit().await?;
            Ok(result)
        }
        Err(e) => {
            let _ = txn.rollback().await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    /// A mock database executor that records SQL statements.
    struct MockDb {
        backend: DatabaseBackendType,
        statements: TokioMutex<Vec<String>>,
    }

    impl MockDb {
        fn new(backend: DatabaseBackendType) -> Self {
            Self {
                backend,
                statements: TokioMutex::new(Vec::new()),
            }
        }

        async fn statements(&self) -> Vec<String> {
            self.statements.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl DbExecutor for MockDb {
        fn backend_type(&self) -> DatabaseBackendType {
            self.backend
        }

        async fn execute_sql(&self, sql: &str, _params: &[Value]) -> DjangoResult<u64> {
            self.statements.lock().await.push(sql.to_string());
            Ok(1)
        }

        async fn query(&self, sql: &str, _params: &[Value]) -> DjangoResult<Vec<Row>> {
            self.statements.lock().await.push(sql.to_string());
            Ok(vec![])
        }

        async fn query_one(&self, sql: &str, _params: &[Value]) -> DjangoResult<Row> {
            self.statements.lock().await.push(sql.to_string());
            Ok(Row::new(vec!["id".to_string()], vec![Value::Int(1)]))
        }

        async fn insert_returning_id(
            &self,
            sql: &str,
            _params: &[Value],
        ) -> DjangoResult<Value> {
            self.statements.lock().await.push(sql.to_string());
            Ok(Value::Int(1))
        }
    }

    #[tokio::test]
    async fn test_basic_transaction_commit() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let result = atomic(&db, |txn| async move {
            txn.execute_sql("INSERT INTO t (a) VALUES (1)", &[]).await?;
            Ok(42)
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        let stmts = db.statements().await;
        assert_eq!(stmts[0], "BEGIN");
        assert_eq!(stmts[1], "INSERT INTO t (a) VALUES (1)");
        assert_eq!(stmts[2], "COMMIT");
    }

    #[tokio::test]
    async fn test_basic_transaction_rollback() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let result: DjangoResult<()> = atomic(&db, |_txn| async move {
            Err(DjangoError::DatabaseError("test error".to_string()))
        })
        .await;

        assert!(result.is_err());

        let stmts = db.statements().await;
        assert_eq!(stmts[0], "BEGIN");
        assert_eq!(stmts[1], "ROLLBACK");
    }

    #[tokio::test]
    async fn test_nested_transaction_creates_savepoint() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);

        let result = atomic(&db, |txn| async move {
            txn.execute_sql("INSERT INTO t VALUES (1)", &[]).await?;

            // Nested atomic — should create a savepoint
            let txn2 = Arc::clone(&txn);
            txn2.begin().await?;
            txn2.execute_sql("INSERT INTO t VALUES (2)", &[]).await?;
            txn2.commit().await?;

            Ok(())
        })
        .await;

        assert!(result.is_ok());

        let stmts = db.statements().await;
        assert_eq!(stmts[0], "BEGIN");
        assert_eq!(stmts[1], "INSERT INTO t VALUES (1)");
        assert!(stmts[2].starts_with("SAVEPOINT sp_"));
        assert_eq!(stmts[3], "INSERT INTO t VALUES (2)");
        assert!(stmts[4].starts_with("RELEASE SAVEPOINT sp_"));
        assert_eq!(stmts[5], "COMMIT");
    }

    #[tokio::test]
    async fn test_nested_transaction_rollback_to_savepoint() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);

        let result = atomic(&db, |txn| async move {
            txn.execute_sql("INSERT INTO t VALUES (1)", &[]).await?;

            // Nested atomic — should create a savepoint
            let txn2 = Arc::clone(&txn);
            txn2.begin().await?;
            txn2.execute_sql("INSERT INTO t VALUES (2)", &[]).await?;
            // Rollback the inner savepoint
            txn2.rollback().await?;

            Ok(())
        })
        .await;

        assert!(result.is_ok());

        let stmts = db.statements().await;
        assert_eq!(stmts[0], "BEGIN");
        assert_eq!(stmts[1], "INSERT INTO t VALUES (1)");
        assert!(stmts[2].starts_with("SAVEPOINT sp_"));
        assert_eq!(stmts[3], "INSERT INTO t VALUES (2)");
        assert!(stmts[4].starts_with("ROLLBACK TO SAVEPOINT sp_"));
        assert_eq!(stmts[5], "COMMIT");
    }

    #[tokio::test]
    async fn test_on_commit_callback_runs_on_commit() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let result = atomic(&db, |txn| {
            let counter = counter_clone;
            async move {
                txn.on_commit(move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                })
                .await;
                Ok(())
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_on_commit_callback_not_run_on_rollback() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let result: DjangoResult<()> = atomic(&db, |txn| {
            let counter = counter_clone;
            async move {
                txn.on_commit(move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                })
                .await;
                Err(DjangoError::DatabaseError("fail".to_string()))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_on_commit_runs_immediately_outside_transaction() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let txn = TransactionManager::new(&db);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        txn.on_commit(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        })
        .await;

        // Should have run immediately since no transaction is active
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_isolation_level_postgresql() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);

        let result = atomic_with_isolation(&db, IsolationLevel::Serializable, |txn| async move {
            txn.execute_sql("SELECT 1", &[]).await?;
            Ok(())
        })
        .await;

        assert!(result.is_ok());

        let stmts = db.statements().await;
        assert_eq!(stmts[0], "BEGIN");
        assert_eq!(
            stmts[1],
            "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE"
        );
        assert_eq!(stmts[2], "SELECT 1");
        assert_eq!(stmts[3], "COMMIT");
    }

    #[tokio::test]
    async fn test_isolation_level_sqlite() {
        let db = MockDb::new(DatabaseBackendType::SQLite);

        let result =
            atomic_with_isolation(&db, IsolationLevel::ReadUncommitted, |txn| async move {
                txn.execute_sql("SELECT 1", &[]).await?;
                Ok(())
            })
            .await;

        assert!(result.is_ok());

        let stmts = db.statements().await;
        assert_eq!(stmts[0], "PRAGMA read_uncommitted = 1");
        assert_eq!(stmts[1], "BEGIN");
        assert_eq!(stmts[2], "SELECT 1");
        assert_eq!(stmts[3], "COMMIT");
    }

    #[tokio::test]
    async fn test_named_savepoint_create_release() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let txn = TransactionManager::new(&db);
        txn.begin().await.unwrap();

        let sp = txn.create_savepoint("my_sp").await.unwrap();
        assert_eq!(sp.name, "my_sp");

        txn.release_savepoint("my_sp").await.unwrap();
        txn.commit().await.unwrap();

        let stmts = db.statements().await;
        assert_eq!(stmts[0], "BEGIN");
        assert_eq!(stmts[1], "SAVEPOINT my_sp");
        assert_eq!(stmts[2], "RELEASE SAVEPOINT my_sp");
        assert_eq!(stmts[3], "COMMIT");
    }

    #[tokio::test]
    async fn test_named_savepoint_rollback() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let txn = TransactionManager::new(&db);
        txn.begin().await.unwrap();

        txn.create_savepoint("my_sp").await.unwrap();
        txn.execute_sql("INSERT INTO t VALUES (1)", &[]).await.unwrap();
        txn.rollback_to_savepoint("my_sp").await.unwrap();
        txn.commit().await.unwrap();

        let stmts = db.statements().await;
        assert_eq!(stmts[0], "BEGIN");
        assert_eq!(stmts[1], "SAVEPOINT my_sp");
        assert_eq!(stmts[2], "INSERT INTO t VALUES (1)");
        assert_eq!(stmts[3], "ROLLBACK TO SAVEPOINT my_sp");
        assert_eq!(stmts[4], "COMMIT");
    }

    #[tokio::test]
    async fn test_commit_without_transaction_errors() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let txn = TransactionManager::new(&db);

        let result = txn.commit().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rollback_without_transaction_errors() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let txn = TransactionManager::new(&db);

        let result = txn.rollback().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_savepoint_without_transaction_errors() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let txn = TransactionManager::new(&db);

        let result = txn.create_savepoint("sp1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transaction_depth_tracking() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let txn = TransactionManager::new(&db);

        assert_eq!(txn.depth().await, 0);

        txn.begin().await.unwrap();
        assert_eq!(txn.depth().await, 1);

        txn.begin().await.unwrap(); // nested
        assert_eq!(txn.depth().await, 2);

        txn.commit().await.unwrap(); // release savepoint
        assert_eq!(txn.depth().await, 1);

        txn.commit().await.unwrap(); // commit transaction
        assert_eq!(txn.depth().await, 0);
    }

    #[tokio::test]
    async fn test_transaction_manager_as_executor() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let txn = TransactionManager::new(&db);

        assert_eq!(txn.backend_type(), DatabaseBackendType::PostgreSQL);

        let result = txn.execute_sql("SELECT 1", &[]).await;
        assert!(result.is_ok());

        let result = txn.query("SELECT 1", &[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_on_commit_callbacks() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let results = Arc::new(std::sync::Mutex::new(Vec::<i32>::new()));

        let r1 = Arc::clone(&results);
        let r2 = Arc::clone(&results);
        let r3 = Arc::clone(&results);

        let outcome = atomic(&db, |txn| async move {
            txn.on_commit(move || { r1.lock().unwrap().push(1); }).await;
            txn.on_commit(move || { r2.lock().unwrap().push(2); }).await;
            txn.on_commit(move || { r3.lock().unwrap().push(3); }).await;
            Ok(())
        })
        .await;

        assert!(outcome.is_ok());
        let collected = results.lock().unwrap().clone();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_pending_callbacks_count() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let txn = TransactionManager::new(&db);

        txn.begin().await.unwrap();
        assert_eq!(txn.pending_callbacks().await, 0);

        txn.on_commit(|| {}).await;
        assert_eq!(txn.pending_callbacks().await, 1);

        txn.on_commit(|| {}).await;
        assert_eq!(txn.pending_callbacks().await, 2);

        txn.commit().await.unwrap();
        assert_eq!(txn.pending_callbacks().await, 0);
    }

    #[test]
    fn test_isolation_level_sql_strings() {
        assert_eq!(IsolationLevel::ReadUncommitted.as_sql(), "READ UNCOMMITTED");
        assert_eq!(IsolationLevel::ReadCommitted.as_sql(), "READ COMMITTED");
        assert_eq!(IsolationLevel::RepeatableRead.as_sql(), "REPEATABLE READ");
        assert_eq!(IsolationLevel::Serializable.as_sql(), "SERIALIZABLE");
    }

    #[test]
    fn test_isolation_level_set_sql() {
        assert_eq!(
            IsolationLevel::Serializable.set_sql(DatabaseBackendType::PostgreSQL),
            "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE"
        );
        assert_eq!(
            IsolationLevel::ReadCommitted.set_sql(DatabaseBackendType::MySQL),
            "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
        );
        assert_eq!(
            IsolationLevel::ReadUncommitted.set_sql(DatabaseBackendType::SQLite),
            "PRAGMA read_uncommitted = 1"
        );
        assert_eq!(
            IsolationLevel::Serializable.set_sql(DatabaseBackendType::SQLite),
            "PRAGMA read_uncommitted = 0"
        );
    }

    #[test]
    fn test_savepoint_auto_name() {
        let sp1 = Savepoint::new();
        let sp2 = Savepoint::new();
        assert_ne!(sp1.name, sp2.name);
        assert!(sp1.name.starts_with("sp_"));
        assert!(!sp1.released);
        assert!(!sp1.rolled_back);
    }

    #[test]
    fn test_savepoint_with_name() {
        let sp = Savepoint::with_name("custom_sp");
        assert_eq!(sp.name, "custom_sp");
    }

    #[test]
    fn test_savepoint_default() {
        let sp = Savepoint::default();
        assert!(sp.name.starts_with("sp_"));
    }

    #[tokio::test]
    async fn test_deeply_nested_savepoints() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);
        let txn = TransactionManager::new(&db);

        txn.begin().await.unwrap(); // depth 1
        txn.begin().await.unwrap(); // depth 2 (savepoint)
        txn.begin().await.unwrap(); // depth 3 (savepoint)
        assert_eq!(txn.depth().await, 3);

        txn.rollback().await.unwrap(); // rollback to sp at depth 3
        assert_eq!(txn.depth().await, 2);

        txn.commit().await.unwrap(); // release sp at depth 2
        assert_eq!(txn.depth().await, 1);

        txn.commit().await.unwrap(); // commit
        assert_eq!(txn.depth().await, 0);

        let stmts = db.statements().await;
        assert_eq!(stmts[0], "BEGIN");
        assert!(stmts[1].starts_with("SAVEPOINT"));
        assert!(stmts[2].starts_with("SAVEPOINT"));
        assert!(stmts[3].starts_with("ROLLBACK TO SAVEPOINT"));
        assert!(stmts[4].starts_with("RELEASE SAVEPOINT"));
        assert_eq!(stmts[5], "COMMIT");
    }
}
