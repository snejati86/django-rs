//! Query counting assertion for database tests.
//!
//! Provides [`assert_num_queries`] which counts the number of SQL queries
//! executed during an async closure and asserts that the count matches an
//! expected value. This is essential for detecting N+1 query problems.
//!
//! ## Example
//!
//! ```rust,no_run
//! use django_rs_test::test_database::TestDatabase;
//! use django_rs_test::assert_queries::assert_num_queries;
//! use django_rs_db::DbExecutor;
//! use django_rs_db::value::Value;
//!
//! async fn example() {
//!     let db = TestDatabase::new();
//!     db.execute_raw("CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT)")
//!         .await
//!         .unwrap();
//!
//!     assert_num_queries(&db, 1, || async {
//!         db.execute_sql("INSERT INTO t (val) VALUES (?)", &[Value::from("x")])
//!             .await
//!             .unwrap();
//!     })
//!     .await;
//! }
//! ```

use std::future::Future;

use crate::test_database::TestDatabase;

/// Asserts that exactly `expected_count` SQL queries are executed during the
/// async closure.
///
/// Resets the query counter on the [`TestDatabase`] before executing the closure,
/// then checks the counter after execution.
///
/// # Panics
///
/// Panics if the number of queries does not match `expected_count`.
pub async fn assert_num_queries<F, Fut>(db: &TestDatabase, expected_count: usize, f: F)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    db.reset_query_count();
    f().await;
    let actual = db.query_count();
    assert_eq!(
        actual, expected_count,
        "Expected {expected_count} SQL queries, but {actual} were executed"
    );
}

/// Asserts that at most `max_count` SQL queries are executed during the async
/// closure.
///
/// Useful when the exact count is not important but you want to prevent query
/// count regression.
///
/// # Panics
///
/// Panics if more than `max_count` queries are executed.
pub async fn assert_max_queries<F, Fut>(db: &TestDatabase, max_count: usize, f: F)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    db.reset_query_count();
    f().await;
    let actual = db.query_count();
    assert!(
        actual <= max_count,
        "Expected at most {max_count} SQL queries, but {actual} were executed"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use django_rs_db::value::Value;
    use django_rs_db::DbExecutor;

    #[tokio::test]
    async fn test_assert_num_queries_passes() {
        let db = TestDatabase::new();
        db.execute_raw("CREATE TABLE nq (id INTEGER PRIMARY KEY, val TEXT)")
            .await
            .unwrap();

        assert_num_queries(&db, 2, || async {
            db.execute_sql("INSERT INTO nq (val) VALUES (?)", &[Value::from("a")])
                .await
                .unwrap();
            db.execute_sql("INSERT INTO nq (val) VALUES (?)", &[Value::from("b")])
                .await
                .unwrap();
        })
        .await;
    }

    #[tokio::test]
    async fn test_assert_num_queries_zero() {
        let db = TestDatabase::new();
        assert_num_queries(&db, 0, || async {
            // No queries
        })
        .await;
    }

    #[tokio::test]
    #[should_panic(expected = "Expected 1 SQL queries, but 2 were executed")]
    async fn test_assert_num_queries_fails_too_many() {
        let db = TestDatabase::new();
        db.execute_raw("CREATE TABLE nqf (id INTEGER PRIMARY KEY, val TEXT)")
            .await
            .unwrap();

        assert_num_queries(&db, 1, || async {
            db.execute_sql("INSERT INTO nqf (val) VALUES (?)", &[Value::from("a")])
                .await
                .unwrap();
            db.execute_sql("INSERT INTO nqf (val) VALUES (?)", &[Value::from("b")])
                .await
                .unwrap();
        })
        .await;
    }

    #[tokio::test]
    #[should_panic(expected = "Expected 3 SQL queries, but 1 were executed")]
    async fn test_assert_num_queries_fails_too_few() {
        let db = TestDatabase::new();
        db.execute_raw("CREATE TABLE nqf2 (id INTEGER PRIMARY KEY, val TEXT)")
            .await
            .unwrap();

        assert_num_queries(&db, 3, || async {
            db.execute_sql("INSERT INTO nqf2 (val) VALUES (?)", &[Value::from("a")])
                .await
                .unwrap();
        })
        .await;
    }

    #[tokio::test]
    async fn test_assert_max_queries_passes() {
        let db = TestDatabase::new();
        db.execute_raw("CREATE TABLE mq (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();

        assert_max_queries(&db, 3, || async {
            db.execute_sql("INSERT INTO mq (id) VALUES (?)", &[Value::from(1)])
                .await
                .unwrap();
        })
        .await;
    }

    #[tokio::test]
    #[should_panic(expected = "Expected at most 1 SQL queries, but 2 were executed")]
    async fn test_assert_max_queries_fails() {
        let db = TestDatabase::new();
        db.execute_raw("CREATE TABLE mqf (id INTEGER PRIMARY KEY, val TEXT)")
            .await
            .unwrap();

        assert_max_queries(&db, 1, || async {
            db.execute_sql("INSERT INTO mqf (val) VALUES (?)", &[Value::from("a")])
                .await
                .unwrap();
            db.execute_sql("INSERT INTO mqf (val) VALUES (?)", &[Value::from("b")])
                .await
                .unwrap();
        })
        .await;
    }

    #[tokio::test]
    async fn test_counter_resets_between_assertions() {
        let db = TestDatabase::new();
        db.execute_raw("CREATE TABLE cr (id INTEGER PRIMARY KEY, val TEXT)")
            .await
            .unwrap();

        // First assertion
        assert_num_queries(&db, 1, || async {
            db.execute_sql("INSERT INTO cr (val) VALUES (?)", &[Value::from("a")])
                .await
                .unwrap();
        })
        .await;

        // Second assertion should count from zero
        assert_num_queries(&db, 1, || async {
            db.query("SELECT * FROM cr", &[]).await.unwrap();
        })
        .await;
    }
}
