//! Database executor trait and model CRUD operations.
//!
//! This module defines the [`DbExecutor`] trait that provides the minimal async
//! interface required by [`QuerySet`](crate::query::queryset::QuerySet) execution
//! methods and model CRUD operations. It also provides free functions for
//! save/create/delete/refresh operations on model instances.
//!
//! The `DbExecutor` trait is implemented by backend types (e.g., `SqliteBackend`,
//! `PostgresBackend`) in the `django-rs-db-backends` crate.
//!
//! # Lifecycle Hooks
//!
//! The CRUD operations support optional lifecycle hooks via the
//! [`ModelLifecycleHooks`] trait. If a model implements this trait, the
//! appropriate hook methods are called before and after each operation.

use crate::model::Model;
use crate::query::compiler::{DatabaseBackendType, Row, SqlCompiler, WhereNode};
use crate::query::lookups::Lookup;
use crate::value::Value;
use django_rs_core::{DjangoError, DjangoResult};

/// Minimal async database executor trait.
///
/// This is the bridge between the ORM layer (`django-rs-db`) and the concrete
/// database backends (`django-rs-db-backends`). `QuerySet` execution methods
/// and model CRUD functions accept `&dyn DbExecutor`, which backends implement.
///
/// Unlike [`DatabaseBackend`](trait@crate::query::compiler::SqlCompiler) (which
/// lives in `django-rs-db-backends`), this trait lives in the ORM crate so that
/// execution can be defined without circular dependencies.
#[async_trait::async_trait]
pub trait DbExecutor: Send + Sync {
    /// Returns the backend type for SQL compilation.
    fn backend_type(&self) -> DatabaseBackendType;

    /// Runs a SQL statement that does not return rows.
    /// Returns the number of rows affected.
    async fn execute_sql(&self, sql: &str, params: &[Value]) -> DjangoResult<u64>;

    /// Runs a SQL query and returns all result rows.
    async fn query(&self, sql: &str, params: &[Value]) -> DjangoResult<Vec<Row>>;

    /// Runs a SQL query and returns exactly one row.
    /// Returns `DoesNotExist` if no rows, `MultipleObjectsReturned` if more than one.
    async fn query_one(&self, sql: &str, params: &[Value]) -> DjangoResult<Row>;

    /// Executes an INSERT and returns the last inserted row ID.
    /// Backends provide a default implementation using `execute` + a follow-up
    /// query, but can override for efficiency.
    async fn insert_returning_id(&self, sql: &str, params: &[Value]) -> DjangoResult<Value> {
        self.execute_sql(sql, params).await?;
        // Default: query last_insert_rowid (SQLite) or LASTVAL() (PG)
        // Each backend should override this for correctness.
        let row = self.query("SELECT last_insert_rowid() AS id", &[]).await?;
        if let Some(r) = row.into_iter().next() {
            Ok(r.get::<Value>("id")?)
        } else {
            Err(DjangoError::DatabaseError(
                "Failed to retrieve last inserted ID".to_string(),
            ))
        }
    }
}

/// Optional lifecycle hooks for model CRUD operations.
///
/// Implement this trait on your model to receive callbacks before and after
/// save, create, and delete operations. This provides a model-local alternative
/// to the global signal system in `django-rs-signals`.
///
/// All methods have default no-op implementations, so you only need to override
/// the hooks you care about.
pub trait ModelLifecycleHooks: Model {
    /// Called before a save (INSERT or UPDATE) operation.
    /// Return `Err` to abort the operation.
    fn on_pre_save(&self) -> DjangoResult<()> {
        Ok(())
    }

    /// Called after a successful save operation.
    fn on_post_save(&self) {
        // no-op
    }

    /// Called before a delete operation.
    /// Return `Err` to abort the operation.
    fn on_pre_delete(&self) -> DjangoResult<()> {
        Ok(())
    }

    /// Called after a successful delete operation.
    fn on_post_delete(&self) {
        // no-op
    }
}

// ── Model CRUD free functions ──────────────────────────────────────────

/// Saves a model instance to the database.
///
/// If the primary key is set (non-None), performs an UPDATE of all fields.
/// If the primary key is None, performs an INSERT and sets the PK from the
/// returned value.
///
/// # Errors
///
/// Returns an error if the database operation fails.
pub async fn save_model<M: Model>(model: &mut M, db: &dyn DbExecutor) -> DjangoResult<()> {
    let compiler = SqlCompiler::new(db.backend_type());

    if model.pk().is_some() {
        // UPDATE: set all non-pk fields WHERE pk = value
        let pk_value = model.pk().unwrap().clone();
        let pk_name = M::pk_field_name();
        let fields: Vec<(&'static str, Value)> = model.non_pk_field_values();

        if fields.is_empty() {
            return Ok(());
        }

        let where_clause = WhereNode::Condition {
            column: pk_name.to_string(),
            lookup: Lookup::Exact(pk_value),
        };
        let (sql, params) = compiler.compile_update(M::table_name(), &fields, &where_clause);
        db.execute_sql(&sql, &params).await?;
    } else {
        // INSERT: insert non-pk fields, retrieve the auto-generated PK
        let fields: Vec<(&'static str, Value)> = model.non_pk_field_values();
        let (sql, params) = compiler.compile_insert(M::table_name(), &fields);
        let pk = db.insert_returning_id(&sql, &params).await?;
        model.set_pk(pk);
    }

    Ok(())
}

/// Saves a model with lifecycle hooks.
///
/// Calls `on_pre_save` before and `on_post_save` after the operation.
pub async fn save_model_with_hooks<M: ModelLifecycleHooks>(
    model: &mut M,
    db: &dyn DbExecutor,
) -> DjangoResult<()> {
    model.on_pre_save()?;
    save_model(model, db).await?;
    model.on_post_save();
    Ok(())
}

/// Creates a new model instance in the database via INSERT.
///
/// Always performs an INSERT regardless of whether the PK is set.
/// Sets the PK from the returned value.
///
/// # Errors
///
/// Returns an error if the INSERT fails.
pub async fn create_model<M: Model>(model: &mut M, db: &dyn DbExecutor) -> DjangoResult<()> {
    let compiler = SqlCompiler::new(db.backend_type());
    let fields: Vec<(&'static str, Value)> = model.non_pk_field_values();
    let (sql, params) = compiler.compile_insert(M::table_name(), &fields);
    let pk = db.insert_returning_id(&sql, &params).await?;
    model.set_pk(pk);
    Ok(())
}

/// Creates a model instance with lifecycle hooks.
pub async fn create_model_with_hooks<M: ModelLifecycleHooks>(
    model: &mut M,
    db: &dyn DbExecutor,
) -> DjangoResult<()> {
    model.on_pre_save()?;
    create_model(model, db).await?;
    model.on_post_save();
    Ok(())
}

/// Deletes a model instance from the database.
///
/// Issues a `DELETE WHERE pk = $1` statement. The model's PK must be set.
///
/// # Errors
///
/// Returns an error if the PK is not set or the DELETE fails.
pub async fn delete_model<M: Model>(model: &M, db: &dyn DbExecutor) -> DjangoResult<u64> {
    let pk_value = model.pk().ok_or_else(|| {
        DjangoError::DatabaseError("Cannot delete a model without a primary key".to_string())
    })?;
    let compiler = SqlCompiler::new(db.backend_type());
    let where_clause = WhereNode::Condition {
        column: M::pk_field_name().to_string(),
        lookup: Lookup::Exact(pk_value.clone()),
    };
    let (sql, params) = compiler.compile_delete(M::table_name(), &where_clause);
    db.execute_sql(&sql, &params).await
}

/// Deletes a model instance with lifecycle hooks.
pub async fn delete_model_with_hooks<M: ModelLifecycleHooks>(
    model: &M,
    db: &dyn DbExecutor,
) -> DjangoResult<u64> {
    model.on_pre_delete()?;
    let count = delete_model(model, db).await?;
    model.on_post_delete();
    Ok(count)
}

/// Refreshes a model instance from the database.
///
/// Performs a `SELECT * WHERE pk = $1` and updates the model with the
/// latest values from the database.
///
/// # Errors
///
/// Returns an error if the PK is not set or the record does not exist.
pub async fn refresh_model<M: Model>(model: &mut M, db: &dyn DbExecutor) -> DjangoResult<()> {
    let pk_value = model.pk().ok_or_else(|| {
        DjangoError::DatabaseError("Cannot refresh a model without a primary key".to_string())
    })?;
    let compiler = SqlCompiler::new(db.backend_type());

    let mut query = crate::query::compiler::Query::new(M::table_name());
    query.where_clause = Some(WhereNode::Condition {
        column: M::pk_field_name().to_string(),
        lookup: Lookup::Exact(pk_value.clone()),
    });
    query.limit = Some(1);

    let (sql, params) = compiler.compile_select(&query);
    let row = db.query_one(&sql, &params).await?;
    *model = M::from_row(&row)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test that DbExecutor is object-safe
    fn _assert_object_safe(_: &dyn DbExecutor) {}

    // Test that ModelLifecycleHooks can be used
    #[test]
    fn test_lifecycle_hooks_default() {
        use crate::fields::{FieldDef, FieldType};
        use crate::model::ModelMeta;

        struct Dummy {
            id: i64,
        }

        impl Model for Dummy {
            fn meta() -> &'static ModelMeta {
                use std::sync::LazyLock;
                static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
                    app_label: "test",
                    model_name: "dummy",
                    db_table: "test_dummy".to_string(),
                    verbose_name: "dummy".to_string(),
                    verbose_name_plural: "dummies".to_string(),
                    ordering: vec![],
                    unique_together: vec![],
                    indexes: vec![],
                    abstract_model: false,
                    fields: vec![FieldDef::new("id", FieldType::BigAutoField).primary_key()],
                    constraints: vec![],
                    inheritance_type: crate::query::compiler::InheritanceType::None,
                });
                &META
            }
            fn table_name() -> &'static str {
                "test_dummy"
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
                vec![("id", Value::Int(self.id))]
            }
            fn from_row(row: &Row) -> Result<Self, DjangoError> {
                Ok(Dummy { id: row.get("id")? })
            }
        }

        impl ModelLifecycleHooks for Dummy {}

        let d = Dummy { id: 1 };
        assert!(d.on_pre_save().is_ok());
        d.on_post_save(); // should not panic
        assert!(d.on_pre_delete().is_ok());
        d.on_post_delete(); // should not panic
    }
}
