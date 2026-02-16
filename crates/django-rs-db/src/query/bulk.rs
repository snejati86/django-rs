//! Bulk database operations.
//!
//! This module provides bulk create, bulk update, get_or_create, and
//! update_or_create operations. These are the equivalent of Django's
//! `QuerySet.bulk_create()`, `QuerySet.bulk_update()`,
//! `QuerySet.get_or_create()`, and `QuerySet.update_or_create()`.
//!
//! # Bulk Operations
//!
//! Bulk operations minimize round trips to the database by batching multiple
//! operations into a single (or few) SQL statements.

use crate::executor::DbExecutor;
use crate::model::Model;
use crate::query::compiler::{DatabaseBackendType, SqlCompiler, WhereNode};
use crate::query::lookups::Lookup;
use crate::value::Value;
use django_rs_core::{DjangoError, DjangoResult};

/// Options for `bulk_create` operations.
#[derive(Debug, Clone, Default)]
pub struct BulkCreateOptions {
    /// Number of objects to create per batch. None means all at once.
    pub batch_size: Option<usize>,
    /// If true, ignore rows that violate unique constraints.
    pub ignore_conflicts: bool,
    /// If true, update conflicting rows instead of ignoring them (upsert).
    /// Requires `update_fields` to be set.
    pub update_conflicts: bool,
    /// Fields to update on conflict (for upsert). Only used when
    /// `update_conflicts` is true.
    pub update_fields: Vec<&'static str>,
    /// Unique fields that define the conflict target. Required when
    /// `update_conflicts` or `ignore_conflicts` is true.
    pub unique_fields: Vec<&'static str>,
}

/// Options for `bulk_update` operations.
#[derive(Debug, Clone, Default)]
pub struct BulkUpdateOptions {
    /// Number of objects to update per batch. None means all at once.
    pub batch_size: Option<usize>,
}

/// Compiles a multi-row INSERT statement for bulk_create.
///
/// Generates SQL like:
/// ```sql
/// INSERT INTO "table" ("col1", "col2") VALUES ($1, $2), ($3, $4), ...
/// ```
///
/// With optional conflict handling:
/// ```sql
/// ... ON CONFLICT ("unique_col") DO NOTHING
/// ... ON CONFLICT ("unique_col") DO UPDATE SET "col1" = EXCLUDED."col1"
/// ```
pub fn compile_bulk_insert(
    table: &str,
    rows: &[Vec<(&str, Value)>],
    options: &BulkCreateOptions,
    backend: DatabaseBackendType,
) -> (String, Vec<Value>) {
    if rows.is_empty() {
        return (String::new(), Vec::new());
    }

    let mut params = Vec::new();

    // Get column names from the first row
    let columns: Vec<&str> = rows[0].iter().map(|(name, _)| *name).collect();
    let col_list: String = columns
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let mut sql = format!("INSERT INTO \"{table}\" ({col_list}) VALUES ");

    // Build value rows
    let mut row_strings = Vec::new();
    for row in rows {
        let placeholders: Vec<String> = row
            .iter()
            .map(|(_, val)| {
                params.push(val.clone());
                match backend {
                    DatabaseBackendType::PostgreSQL => format!("${}", params.len()),
                    DatabaseBackendType::SQLite | DatabaseBackendType::MySQL => "?".to_string(),
                }
            })
            .collect();
        row_strings.push(format!("({})", placeholders.join(", ")));
    }
    sql.push_str(&row_strings.join(", "));

    // Add conflict handling
    if options.ignore_conflicts || options.update_conflicts {
        match backend {
            DatabaseBackendType::PostgreSQL | DatabaseBackendType::SQLite => {
                if options.unique_fields.is_empty() {
                    sql.push_str(" ON CONFLICT");
                } else {
                    let unique_cols: String = options
                        .unique_fields
                        .iter()
                        .map(|f| format!("\"{f}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    sql.push_str(&format!(" ON CONFLICT ({unique_cols})"));
                }

                if options.update_conflicts && !options.update_fields.is_empty() {
                    let set_parts: String = options
                        .update_fields
                        .iter()
                        .map(|f| format!("\"{f}\" = EXCLUDED.\"{f}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    sql.push_str(&format!(" DO UPDATE SET {set_parts}"));
                } else {
                    sql.push_str(" DO NOTHING");
                }
            }
            DatabaseBackendType::MySQL => {
                if options.update_conflicts && !options.update_fields.is_empty() {
                    let set_parts: String = options
                        .update_fields
                        .iter()
                        .map(|f| format!("\"{f}\" = VALUES(\"{f}\")"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    sql.push_str(&format!(" ON DUPLICATE KEY UPDATE {set_parts}"));
                } else if options.ignore_conflicts {
                    // For MySQL, use INSERT IGNORE
                    sql = sql.replacen("INSERT INTO", "INSERT IGNORE INTO", 1);
                }
            }
        }
    }

    (sql, params)
}

/// Compiles a batched UPDATE statement for bulk_update.
///
/// For each object, generates a separate UPDATE WHERE pk = value statement.
/// Returns a vector of (sql, params) pairs, one per batch.
pub fn compile_bulk_update(
    table: &str,
    pk_field: &str,
    objects: &[(Value, Vec<(&str, Value)>)],
    fields: &[&str],
    batch_size: Option<usize>,
    backend: DatabaseBackendType,
) -> Vec<(String, Vec<Value>)> {
    if objects.is_empty() || fields.is_empty() {
        return Vec::new();
    }

    let batch_size = batch_size.unwrap_or(objects.len());
    let compiler = SqlCompiler::new(backend);
    let mut results = Vec::new();

    for chunk in objects.chunks(batch_size) {
        for (pk_value, field_values) in chunk {
            let update_fields: Vec<(&str, Value)> = field_values
                .iter()
                .filter(|(name, _)| fields.contains(name))
                .cloned()
                .collect();

            if update_fields.is_empty() {
                continue;
            }

            let where_clause = WhereNode::Condition {
                column: pk_field.to_string(),
                lookup: Lookup::Exact(pk_value.clone()),
            };

            let (sql, params) = compiler.compile_update(table, &update_fields, &where_clause);
            results.push((sql, params));
        }
    }

    results
}

/// Executes a `bulk_create` operation.
///
/// Inserts multiple model instances in batched INSERT statements.
/// Returns the number of rows inserted.
pub async fn bulk_create<M: Model>(
    objects: &mut [M],
    options: &BulkCreateOptions,
    db: &dyn DbExecutor,
) -> DjangoResult<u64> {
    if objects.is_empty() {
        return Ok(0);
    }

    let batch_size = options.batch_size.unwrap_or(objects.len());
    let mut total_inserted = 0u64;

    for chunk in objects.chunks_mut(batch_size) {
        let rows: Vec<Vec<(&str, Value)>> = chunk.iter().map(Model::non_pk_field_values).collect();

        let (sql, params) = compile_bulk_insert(M::table_name(), &rows, options, db.backend_type());

        if sql.is_empty() {
            continue;
        }

        let affected = db.execute_sql(&sql, &params).await?;
        total_inserted += affected;
    }

    Ok(total_inserted)
}

/// Executes a `bulk_update` operation.
///
/// Updates specific fields on multiple model instances. Each object must
/// have a primary key set.
///
/// Returns the total number of rows affected.
pub async fn bulk_update<M: Model>(
    objects: &[M],
    fields: &[&str],
    options: &BulkUpdateOptions,
    db: &dyn DbExecutor,
) -> DjangoResult<u64> {
    if objects.is_empty() || fields.is_empty() {
        return Ok(0);
    }

    // Build (pk, field_values) pairs
    let pk_and_fields: Vec<(Value, Vec<(&str, Value)>)> = objects
        .iter()
        .map(|obj| {
            let pk = obj.pk().ok_or_else(|| {
                DjangoError::DatabaseError(
                    "bulk_update requires all objects to have a primary key set".to_string(),
                )
            })?;
            Ok((pk.clone(), obj.field_values()))
        })
        .collect::<DjangoResult<Vec<_>>>()?;

    let statements = compile_bulk_update(
        M::table_name(),
        M::pk_field_name(),
        &pk_and_fields,
        fields,
        options.batch_size,
        db.backend_type(),
    );

    let mut total = 0u64;
    for (sql, params) in &statements {
        total += db.execute_sql(sql, params).await?;
    }

    Ok(total)
}

/// Looks up an object with the given lookup criteria, creating one if it
/// doesn't exist.
///
/// Returns a tuple of `(object, created)` where `created` is `true` if a
/// new object was created.
///
/// This is equivalent to Django's `QuerySet.get_or_create()`.
///
/// # Arguments
///
/// * `lookup_fields` - Fields used for the lookup (WHERE clause)
/// * `defaults` - Additional fields to set when creating (not used for lookup)
/// * `db` - The database executor
pub async fn get_or_create<M: Model>(
    lookup_fields: &[(&'static str, Value)],
    defaults: &[(&'static str, Value)],
    db: &dyn DbExecutor,
) -> DjangoResult<(M, bool)> {
    let compiler = SqlCompiler::new(db.backend_type());

    // Build WHERE clause from lookup fields
    let where_nodes: Vec<WhereNode> = lookup_fields
        .iter()
        .map(|(name, val)| WhereNode::Condition {
            column: (*name).to_string(),
            lookup: Lookup::Exact(val.clone()),
        })
        .collect();

    let where_clause = if where_nodes.len() == 1 {
        where_nodes.into_iter().next().unwrap()
    } else {
        WhereNode::And(where_nodes)
    };

    // Try to SELECT first
    let mut query = crate::query::compiler::Query::new(M::table_name());
    query.where_clause = Some(where_clause.clone());
    query.limit = Some(1);

    let (select_sql, select_params) = compiler.compile_select(&query);
    let rows = db.query(&select_sql, &select_params).await?;

    if let Some(row) = rows.into_iter().next() {
        // Object exists
        let obj = M::from_row(&row)?;
        return Ok((obj, false));
    }

    // Object doesn't exist — create it
    let mut create_fields: Vec<(&str, Value)> = Vec::new();
    for (name, val) in lookup_fields {
        create_fields.push((name, val.clone()));
    }
    for (name, val) in defaults {
        create_fields.push((name, val.clone()));
    }

    let (insert_sql, insert_params) = compiler.compile_insert(M::table_name(), &create_fields);
    let pk = db.insert_returning_id(&insert_sql, &insert_params).await?;

    // Fetch the created object
    let pk_where = WhereNode::Condition {
        column: M::pk_field_name().to_string(),
        lookup: Lookup::Exact(pk),
    };
    let mut fetch_query = crate::query::compiler::Query::new(M::table_name());
    fetch_query.where_clause = Some(pk_where);
    fetch_query.limit = Some(1);

    let (fetch_sql, fetch_params) = compiler.compile_select(&fetch_query);
    let fetch_rows = db.query(&fetch_sql, &fetch_params).await?;

    if let Some(row) = fetch_rows.into_iter().next() {
        let obj = M::from_row(&row)?;
        Ok((obj, true))
    } else {
        Err(DjangoError::DatabaseError(
            "Failed to fetch newly created object".to_string(),
        ))
    }
}

/// Looks up an object with the given lookup criteria. If found, updates it
/// with the given defaults. If not found, creates it.
///
/// Returns a tuple of `(object, created)` where `created` is `true` if a
/// new object was created.
///
/// This is equivalent to Django's `QuerySet.update_or_create()`.
pub async fn update_or_create<M: Model>(
    lookup_fields: &[(&'static str, Value)],
    defaults: &[(&'static str, Value)],
    db: &dyn DbExecutor,
) -> DjangoResult<(M, bool)> {
    let compiler = SqlCompiler::new(db.backend_type());

    // Build WHERE clause from lookup fields
    let where_nodes: Vec<WhereNode> = lookup_fields
        .iter()
        .map(|(name, val)| WhereNode::Condition {
            column: (*name).to_string(),
            lookup: Lookup::Exact(val.clone()),
        })
        .collect();

    let where_clause = if where_nodes.len() == 1 {
        where_nodes.into_iter().next().unwrap()
    } else {
        WhereNode::And(where_nodes)
    };

    // Try to SELECT first
    let mut query = crate::query::compiler::Query::new(M::table_name());
    query.where_clause = Some(where_clause.clone());
    query.limit = Some(1);

    let (select_sql, select_params) = compiler.compile_select(&query);
    let rows = db.query(&select_sql, &select_params).await?;

    if let Some(row) = rows.into_iter().next() {
        // Object exists — update it with defaults
        let existing = M::from_row(&row)?;

        if !defaults.is_empty() {
            let update_fields: Vec<(&str, Value)> = defaults.to_vec();
            let (update_sql, update_params) =
                compiler.compile_update(M::table_name(), &update_fields, &where_clause);
            db.execute_sql(&update_sql, &update_params).await?;

            // Re-fetch to get updated values
            let pk_value = existing.pk().ok_or_else(|| {
                DjangoError::DatabaseError("Object has no primary key".to_string())
            })?;
            let pk_where = WhereNode::Condition {
                column: M::pk_field_name().to_string(),
                lookup: Lookup::Exact(pk_value.clone()),
            };
            let mut refetch_query = crate::query::compiler::Query::new(M::table_name());
            refetch_query.where_clause = Some(pk_where);
            refetch_query.limit = Some(1);

            let (refetch_sql, refetch_params) = compiler.compile_select(&refetch_query);
            let refetch_rows = db.query(&refetch_sql, &refetch_params).await?;

            if let Some(updated_row) = refetch_rows.into_iter().next() {
                return Ok((M::from_row(&updated_row)?, false));
            }
        }

        Ok((existing, false))
    } else {
        // Object doesn't exist — create it
        let mut create_fields: Vec<(&str, Value)> = Vec::new();
        for (name, val) in lookup_fields {
            create_fields.push((name, val.clone()));
        }
        for (name, val) in defaults {
            create_fields.push((name, val.clone()));
        }

        let (insert_sql, insert_params) = compiler.compile_insert(M::table_name(), &create_fields);
        let pk = db.insert_returning_id(&insert_sql, &insert_params).await?;

        // Fetch the created object
        let pk_where = WhereNode::Condition {
            column: M::pk_field_name().to_string(),
            lookup: Lookup::Exact(pk),
        };
        let mut fetch_query = crate::query::compiler::Query::new(M::table_name());
        fetch_query.where_clause = Some(pk_where);
        fetch_query.limit = Some(1);

        let (fetch_sql, fetch_params) = compiler.compile_select(&fetch_query);
        let fetch_rows = db.query(&fetch_sql, &fetch_params).await?;

        if let Some(row) = fetch_rows.into_iter().next() {
            Ok((M::from_row(&row)?, true))
        } else {
            Err(DjangoError::DatabaseError(
                "Failed to fetch newly created object".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::{FieldDef, FieldType};
    use crate::model::ModelMeta;
    use crate::query::compiler::{OrderBy, Row};
    use tokio::sync::Mutex as TokioMutex;

    // Test model
    #[derive(Debug)]
    struct Item {
        id: i64,
        name: String,
        price: i64,
    }

    impl Model for Item {
        fn meta() -> &'static ModelMeta {
            use std::sync::LazyLock;
            static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
                app_label: "test",
                model_name: "item",
                db_table: "test_item".to_string(),
                verbose_name: "item".to_string(),
                verbose_name_plural: "items".to_string(),
                ordering: vec![],
                unique_together: vec![],
                indexes: vec![],
                abstract_model: false,
                fields: vec![
                    FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                    FieldDef::new("name", FieldType::CharField).max_length(100),
                    FieldDef::new("price", FieldType::IntegerField),
                ],
                constraints: vec![],
                inheritance_type: crate::query::compiler::InheritanceType::None,
            });
            &META
        }
        fn table_name() -> &'static str {
            "test_item"
        }
        fn app_label() -> &'static str {
            "test"
        }
        fn pk(&self) -> Option<&Value> {
            if self.id == 0 {
                None
            } else {
                Some(&Value::Int(0)) // placeholder
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
                ("price", Value::Int(self.price)),
            ]
        }
        fn from_row(row: &Row) -> Result<Self, DjangoError> {
            Ok(Item {
                id: row.get("id")?,
                name: row.get("name")?,
                price: row.get("price")?,
            })
        }
    }

    // Mock DB with configurable responses
    struct MockDb {
        backend: DatabaseBackendType,
        statements: TokioMutex<Vec<(String, Vec<Value>)>>,
        query_responses: TokioMutex<Vec<Vec<Row>>>,
        insert_id: TokioMutex<i64>,
    }

    impl MockDb {
        fn new(backend: DatabaseBackendType) -> Self {
            Self {
                backend,
                statements: TokioMutex::new(Vec::new()),
                query_responses: TokioMutex::new(Vec::new()),
                insert_id: TokioMutex::new(1),
            }
        }

        fn with_responses(backend: DatabaseBackendType, responses: Vec<Vec<Row>>) -> Self {
            Self {
                backend,
                statements: TokioMutex::new(Vec::new()),
                query_responses: TokioMutex::new(responses),
                insert_id: TokioMutex::new(1),
            }
        }

        async fn statements(&self) -> Vec<(String, Vec<Value>)> {
            self.statements.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl DbExecutor for MockDb {
        fn backend_type(&self) -> DatabaseBackendType {
            self.backend
        }

        async fn execute_sql(&self, sql: &str, params: &[Value]) -> DjangoResult<u64> {
            self.statements
                .lock()
                .await
                .push((sql.to_string(), params.to_vec()));
            Ok(1)
        }

        async fn query(&self, sql: &str, params: &[Value]) -> DjangoResult<Vec<Row>> {
            self.statements
                .lock()
                .await
                .push((sql.to_string(), params.to_vec()));
            let mut responses = self.query_responses.lock().await;
            if responses.is_empty() {
                Ok(vec![])
            } else {
                Ok(responses.remove(0))
            }
        }

        async fn query_one(&self, sql: &str, params: &[Value]) -> DjangoResult<Row> {
            self.statements
                .lock()
                .await
                .push((sql.to_string(), params.to_vec()));
            Ok(Row::new(vec!["id".to_string()], vec![Value::Int(1)]))
        }

        async fn insert_returning_id(&self, sql: &str, params: &[Value]) -> DjangoResult<Value> {
            self.statements
                .lock()
                .await
                .push((sql.to_string(), params.to_vec()));
            let mut id = self.insert_id.lock().await;
            let current = *id;
            *id += 1;
            Ok(Value::Int(current))
        }
    }

    // ── compile_bulk_insert tests ──────────────────────────────────────

    #[test]
    fn test_bulk_insert_pg_basic() {
        let rows = vec![
            vec![("name", Value::from("Alice")), ("price", Value::from(10))],
            vec![("name", Value::from("Bob")), ("price", Value::from(20))],
        ];
        let options = BulkCreateOptions::default();
        let (sql, params) = compile_bulk_insert(
            "test_item",
            &rows,
            &options,
            DatabaseBackendType::PostgreSQL,
        );

        assert_eq!(
            sql,
            "INSERT INTO \"test_item\" (\"name\", \"price\") VALUES ($1, $2), ($3, $4)"
        );
        assert_eq!(params.len(), 4);
    }

    #[test]
    fn test_bulk_insert_sqlite_basic() {
        let rows = vec![vec![
            ("name", Value::from("Alice")),
            ("price", Value::from(10)),
        ]];
        let options = BulkCreateOptions::default();
        let (sql, params) =
            compile_bulk_insert("test_item", &rows, &options, DatabaseBackendType::SQLite);

        assert_eq!(
            sql,
            "INSERT INTO \"test_item\" (\"name\", \"price\") VALUES (?, ?)"
        );
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_bulk_insert_empty() {
        let rows: Vec<Vec<(&str, Value)>> = vec![];
        let options = BulkCreateOptions::default();
        let (sql, params) = compile_bulk_insert(
            "test_item",
            &rows,
            &options,
            DatabaseBackendType::PostgreSQL,
        );

        assert!(sql.is_empty());
        assert!(params.is_empty());
    }

    #[test]
    fn test_bulk_insert_ignore_conflicts_pg() {
        let rows = vec![vec![("name", Value::from("Alice"))]];
        let options = BulkCreateOptions {
            ignore_conflicts: true,
            unique_fields: vec!["name"],
            ..Default::default()
        };
        let (sql, _) = compile_bulk_insert(
            "test_item",
            &rows,
            &options,
            DatabaseBackendType::PostgreSQL,
        );

        assert!(sql.contains("ON CONFLICT (\"name\") DO NOTHING"));
    }

    #[test]
    fn test_bulk_insert_ignore_conflicts_mysql() {
        let rows = vec![vec![("name", Value::from("Alice"))]];
        let options = BulkCreateOptions {
            ignore_conflicts: true,
            unique_fields: vec!["name"],
            ..Default::default()
        };
        let (sql, _) =
            compile_bulk_insert("test_item", &rows, &options, DatabaseBackendType::MySQL);

        assert!(sql.starts_with("INSERT IGNORE INTO"));
    }

    #[test]
    fn test_bulk_insert_upsert_pg() {
        let rows = vec![vec![
            ("name", Value::from("Alice")),
            ("price", Value::from(10)),
        ]];
        let options = BulkCreateOptions {
            update_conflicts: true,
            update_fields: vec!["price"],
            unique_fields: vec!["name"],
            ..Default::default()
        };
        let (sql, _) = compile_bulk_insert(
            "test_item",
            &rows,
            &options,
            DatabaseBackendType::PostgreSQL,
        );

        assert!(sql.contains("ON CONFLICT (\"name\") DO UPDATE SET \"price\" = EXCLUDED.\"price\""));
    }

    #[test]
    fn test_bulk_insert_upsert_mysql() {
        let rows = vec![vec![
            ("name", Value::from("Alice")),
            ("price", Value::from(10)),
        ]];
        let options = BulkCreateOptions {
            update_conflicts: true,
            update_fields: vec!["price"],
            unique_fields: vec!["name"],
            ..Default::default()
        };
        let (sql, _) =
            compile_bulk_insert("test_item", &rows, &options, DatabaseBackendType::MySQL);

        assert!(sql.contains("ON DUPLICATE KEY UPDATE \"price\" = VALUES(\"price\")"));
    }

    #[test]
    fn test_bulk_insert_upsert_sqlite() {
        let rows = vec![vec![
            ("name", Value::from("Alice")),
            ("price", Value::from(10)),
        ]];
        let options = BulkCreateOptions {
            update_conflicts: true,
            update_fields: vec!["price"],
            unique_fields: vec!["name"],
            ..Default::default()
        };
        let (sql, _) =
            compile_bulk_insert("test_item", &rows, &options, DatabaseBackendType::SQLite);

        assert!(sql.contains("ON CONFLICT (\"name\") DO UPDATE SET \"price\" = EXCLUDED.\"price\""));
    }

    // ── compile_bulk_update tests ──────────────────────────────────────

    #[test]
    fn test_bulk_update_basic() {
        let objects = vec![
            (
                Value::Int(1),
                vec![
                    ("id", Value::Int(1)),
                    ("name", Value::from("Alice Updated")),
                    ("price", Value::from(15)),
                ],
            ),
            (
                Value::Int(2),
                vec![
                    ("id", Value::Int(2)),
                    ("name", Value::from("Bob Updated")),
                    ("price", Value::from(25)),
                ],
            ),
        ];

        let stmts = compile_bulk_update(
            "test_item",
            "id",
            &objects,
            &["name"],
            None,
            DatabaseBackendType::PostgreSQL,
        );

        assert_eq!(stmts.len(), 2);
        assert!(stmts[0]
            .0
            .contains("UPDATE \"test_item\" SET \"name\" = $1 WHERE \"id\" = $2"));
        assert!(stmts[1]
            .0
            .contains("UPDATE \"test_item\" SET \"name\" = $1 WHERE \"id\" = $2"));
    }

    #[test]
    fn test_bulk_update_empty_objects() {
        let objects: Vec<(Value, Vec<(&str, Value)>)> = vec![];
        let stmts = compile_bulk_update(
            "test_item",
            "id",
            &objects,
            &["name"],
            None,
            DatabaseBackendType::PostgreSQL,
        );
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_bulk_update_empty_fields() {
        let objects = vec![(Value::Int(1), vec![("name", Value::from("Alice"))])];
        let stmts = compile_bulk_update(
            "test_item",
            "id",
            &objects,
            &[],
            None,
            DatabaseBackendType::PostgreSQL,
        );
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_bulk_update_with_batch_size() {
        let objects = vec![
            (Value::Int(1), vec![("name", Value::from("A"))]),
            (Value::Int(2), vec![("name", Value::from("B"))]),
            (Value::Int(3), vec![("name", Value::from("C"))]),
        ];

        let stmts = compile_bulk_update(
            "test_item",
            "id",
            &objects,
            &["name"],
            Some(2),
            DatabaseBackendType::PostgreSQL,
        );

        // Should still generate 3 statements (one per object)
        assert_eq!(stmts.len(), 3);
    }

    // ── Async operation tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_bulk_create_execution() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);

        let mut items = vec![
            Item {
                id: 0,
                name: "Alice".to_string(),
                price: 10,
            },
            Item {
                id: 0,
                name: "Bob".to_string(),
                price: 20,
            },
        ];

        let options = BulkCreateOptions::default();
        let count = bulk_create(&mut items, &options, &db).await.unwrap();
        assert_eq!(count, 1); // MockDb always returns 1 for execute_sql

        let stmts = db.statements().await;
        assert_eq!(stmts.len(), 1);
        assert!(stmts[0].0.contains("INSERT INTO \"test_item\""));
        assert!(stmts[0].0.contains("VALUES"));
    }

    #[tokio::test]
    async fn test_bulk_create_with_batch_size() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);

        let mut items = vec![
            Item {
                id: 0,
                name: "A".to_string(),
                price: 10,
            },
            Item {
                id: 0,
                name: "B".to_string(),
                price: 20,
            },
            Item {
                id: 0,
                name: "C".to_string(),
                price: 30,
            },
        ];

        let options = BulkCreateOptions {
            batch_size: Some(2),
            ..Default::default()
        };
        let count = bulk_create(&mut items, &options, &db).await.unwrap();
        assert_eq!(count, 2); // Two batches, 1 each

        let stmts = db.statements().await;
        assert_eq!(stmts.len(), 2); // Two INSERT batches
    }

    #[tokio::test]
    async fn test_bulk_create_empty() {
        let db = MockDb::new(DatabaseBackendType::PostgreSQL);

        let mut items: Vec<Item> = vec![];
        let options = BulkCreateOptions::default();
        let count = bulk_create(&mut items, &options, &db).await.unwrap();
        assert_eq!(count, 0);

        let stmts = db.statements().await;
        assert!(stmts.is_empty());
    }

    #[tokio::test]
    async fn test_get_or_create_existing() {
        let existing_row = Row::new(
            vec!["id".to_string(), "name".to_string(), "price".to_string()],
            vec![
                Value::Int(1),
                Value::String("Alice".to_string()),
                Value::Int(10),
            ],
        );

        let db = MockDb::with_responses(DatabaseBackendType::PostgreSQL, vec![vec![existing_row]]);

        let (item, created): (Item, bool) = get_or_create(
            &[("name", Value::from("Alice"))],
            &[("price", Value::from(10))],
            &db,
        )
        .await
        .unwrap();

        assert!(!created);
        assert_eq!(item.name, "Alice");
        assert_eq!(item.id, 1);
    }

    #[tokio::test]
    async fn test_get_or_create_new() {
        let created_row = Row::new(
            vec!["id".to_string(), "name".to_string(), "price".to_string()],
            vec![
                Value::Int(1),
                Value::String("Bob".to_string()),
                Value::Int(20),
            ],
        );

        let db = MockDb::with_responses(
            DatabaseBackendType::PostgreSQL,
            vec![
                vec![],            // SELECT returns empty (doesn't exist)
                vec![created_row], // fetch after INSERT
            ],
        );

        let (item, created): (Item, bool) = get_or_create(
            &[("name", Value::from("Bob"))],
            &[("price", Value::from(20))],
            &db,
        )
        .await
        .unwrap();

        assert!(created);
        assert_eq!(item.name, "Bob");

        let stmts = db.statements().await;
        // Should have: SELECT, INSERT, SELECT (to fetch created)
        assert!(stmts.len() >= 2);
        assert!(stmts[0].0.contains("SELECT"));
        assert!(stmts[1].0.contains("INSERT"));
    }

    #[tokio::test]
    async fn test_update_or_create_existing() {
        let existing_row = Row::new(
            vec!["id".to_string(), "name".to_string(), "price".to_string()],
            vec![
                Value::Int(1),
                Value::String("Alice".to_string()),
                Value::Int(10),
            ],
        );
        let updated_row = Row::new(
            vec!["id".to_string(), "name".to_string(), "price".to_string()],
            vec![
                Value::Int(1),
                Value::String("Alice".to_string()),
                Value::Int(99),
            ],
        );

        let db = MockDb::with_responses(
            DatabaseBackendType::PostgreSQL,
            vec![
                vec![existing_row], // SELECT finds existing
                vec![updated_row],  // re-fetch after UPDATE
            ],
        );

        let (item, created): (Item, bool) = update_or_create(
            &[("name", Value::from("Alice"))],
            &[("price", Value::from(99))],
            &db,
        )
        .await
        .unwrap();

        assert!(!created);
        assert_eq!(item.price, 99);

        let stmts = db.statements().await;
        // Should have: SELECT, UPDATE, SELECT (re-fetch)
        assert!(stmts[0].0.contains("SELECT"));
        assert!(stmts[1].0.contains("UPDATE"));
    }

    #[tokio::test]
    async fn test_update_or_create_new() {
        let created_row = Row::new(
            vec!["id".to_string(), "name".to_string(), "price".to_string()],
            vec![
                Value::Int(1),
                Value::String("Charlie".to_string()),
                Value::Int(30),
            ],
        );

        let db = MockDb::with_responses(
            DatabaseBackendType::PostgreSQL,
            vec![
                vec![],            // SELECT returns empty
                vec![created_row], // fetch after INSERT
            ],
        );

        let (item, created): (Item, bool) = update_or_create(
            &[("name", Value::from("Charlie"))],
            &[("price", Value::from(30))],
            &db,
        )
        .await
        .unwrap();

        assert!(created);
        assert_eq!(item.name, "Charlie");
    }

    // ── Options tests ─────────────────────────────────────────────────

    #[test]
    fn test_bulk_create_options_default() {
        let opts = BulkCreateOptions::default();
        assert!(opts.batch_size.is_none());
        assert!(!opts.ignore_conflicts);
        assert!(!opts.update_conflicts);
        assert!(opts.update_fields.is_empty());
        assert!(opts.unique_fields.is_empty());
    }

    #[test]
    fn test_bulk_update_options_default() {
        let opts = BulkUpdateOptions::default();
        assert!(opts.batch_size.is_none());
    }

    #[test]
    fn test_bulk_insert_multiple_rows_pg() {
        let rows = vec![
            vec![("name", Value::from("A")), ("price", Value::from(1))],
            vec![("name", Value::from("B")), ("price", Value::from(2))],
            vec![("name", Value::from("C")), ("price", Value::from(3))],
        ];
        let options = BulkCreateOptions::default();
        let (sql, params) = compile_bulk_insert(
            "test_item",
            &rows,
            &options,
            DatabaseBackendType::PostgreSQL,
        );

        assert!(sql.contains("($1, $2), ($3, $4), ($5, $6)"));
        assert_eq!(params.len(), 6);
    }

    #[test]
    fn test_bulk_insert_multiple_rows_sqlite() {
        let rows = vec![
            vec![("name", Value::from("A")), ("price", Value::from(1))],
            vec![("name", Value::from("B")), ("price", Value::from(2))],
        ];
        let options = BulkCreateOptions::default();
        let (sql, params) =
            compile_bulk_insert("test_item", &rows, &options, DatabaseBackendType::SQLite);

        assert!(sql.contains("(?, ?), (?, ?)"));
        assert_eq!(params.len(), 4);
    }

    #[test]
    fn test_bulk_update_multiple_fields() {
        let objects = vec![(
            Value::Int(1),
            vec![
                ("id", Value::Int(1)),
                ("name", Value::from("Updated")),
                ("price", Value::from(99)),
            ],
        )];

        let stmts = compile_bulk_update(
            "test_item",
            "id",
            &objects,
            &["name", "price"],
            None,
            DatabaseBackendType::PostgreSQL,
        );

        assert_eq!(stmts.len(), 1);
        assert!(stmts[0].0.contains("\"name\" = $1"));
        assert!(stmts[0].0.contains("\"price\" = $2"));
    }
}
