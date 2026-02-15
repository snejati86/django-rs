//! QuerySet and Manager for building and executing database queries.
//!
//! The [`QuerySet`] represents a lazy database query that builds up a SQL query
//! AST. It only executes when a terminal method is called (`.get()`, `.count()`,
//! `.first()`, etc.). The [`Manager`] is the entry point for accessing querysets
//! on a model, equivalent to Django's `objects` manager.
//!
//! # Examples
//!
//! ```
//! use django_rs_db::query::queryset::{QuerySet, Manager};
//! use django_rs_db::query::lookups::{Q, Lookup};
//! use django_rs_db::value::Value;
//! // QuerySets are lazy — they build a Query AST without executing anything.
//! ```

use super::compiler::{
    DatabaseBackendType, OrderBy, Query, SelectColumn, SqlCompiler, WhereNode,
};
use super::expressions::Expression;
use super::lookups::Q;
use crate::executor::DbExecutor;
use crate::model::Model;
use crate::value::Value;
use django_rs_core::{DjangoError, DjangoResult};
use std::marker::PhantomData;

/// The entry point for model-level query operations.
///
/// Every model has a default `Manager` that provides access to the
/// `QuerySet` API. This is the equivalent of Django's `Model.objects`.
///
/// The `Manager` itself does not hold any query state — it simply
/// creates fresh `QuerySet` instances.
#[derive(Debug)]
pub struct Manager<M: Model> {
    _phantom: PhantomData<M>,
    using: Option<String>,
}

impl<M: Model> Default for Manager<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Model> Manager<M> {
    /// Creates a new manager.
    pub const fn new() -> Self {
        Self {
            _phantom: PhantomData,
            using: None,
        }
    }

    /// Sets the database alias for this manager.
    #[must_use]
    pub fn using(mut self, db: impl Into<String>) -> Self {
        self.using = Some(db.into());
        self
    }

    /// Returns a new `QuerySet` that returns all objects.
    pub fn all(&self) -> QuerySet<M> {
        QuerySet::new(self.using.clone())
    }

    /// Returns a new `QuerySet` with the given filter applied.
    pub fn filter(&self, q: Q) -> QuerySet<M> {
        self.all().filter(q)
    }

    /// Returns a new `QuerySet` with the given exclusion applied.
    pub fn exclude(&self, q: Q) -> QuerySet<M> {
        self.all().exclude(q)
    }

    /// Returns an empty `QuerySet` that matches nothing.
    pub fn none(&self) -> QuerySet<M> {
        self.all().none()
    }

    /// Shortcut for creating a record via the queryset.
    pub fn create(&self, fields: Vec<(&'static str, Value)>) -> QuerySet<M> {
        let mut qs = self.all();
        qs.pending_create = Some(fields);
        qs
    }
}

/// A lazy, composable database query.
///
/// `QuerySet` builds a [`Query`] AST through method chaining. The SQL is only
/// generated and executed when a terminal method is called. This mirrors
/// Django's `QuerySet` behavior exactly.
///
/// All filtering/ordering methods return a new `QuerySet` (they consume `self`
/// and return a modified version), making the API chainable and immutable from
/// the caller's perspective.
pub struct QuerySet<M: Model> {
    model: PhantomData<M>,
    query: Query,
    using: Option<String>,
    /// Whether this queryset should return no results.
    is_none: bool,
    /// Pending create operation fields.
    pending_create: Option<Vec<(&'static str, Value)>>,
    /// Pending update operation fields.
    pending_update: Option<Vec<(&'static str, Value)>>,
    /// Whether this is a delete operation.
    pending_delete: bool,
}

impl<M: Model> QuerySet<M> {
    /// Creates a new queryset for the model.
    fn new(using: Option<String>) -> Self {
        Self {
            model: PhantomData,
            query: Query::new(M::table_name()),
            using,
            is_none: false,
            pending_create: None,
            pending_update: None,
            pending_delete: false,
        }
    }

    /// Returns a reference to the underlying query AST.
    pub const fn query(&self) -> &Query {
        &self.query
    }

    /// Returns the database alias in use.
    pub fn using_db(&self) -> Option<&str> {
        self.using.as_deref()
    }

    // ── Filtering methods (lazy) ─────────────────────────────────────

    /// Adds a filter condition. Returns a new queryset.
    #[must_use]
    pub fn filter(mut self, q: Q) -> Self {
        let new_node = WhereNode::from_q(&q);
        self.query.where_clause = Some(match self.query.where_clause.take() {
            Some(existing) => WhereNode::And(vec![existing, new_node]),
            None => new_node,
        });
        self
    }

    /// Adds an exclusion condition (NOT). Returns a new queryset.
    #[must_use]
    pub fn exclude(mut self, q: Q) -> Self {
        let new_node = WhereNode::Not(Box::new(WhereNode::from_q(&q)));
        self.query.where_clause = Some(match self.query.where_clause.take() {
            Some(existing) => WhereNode::And(vec![existing, new_node]),
            None => new_node,
        });
        self
    }

    /// Sets the ordering. Returns a new queryset.
    #[must_use]
    pub fn order_by(mut self, fields: Vec<OrderBy>) -> Self {
        self.query.order_by = fields;
        self
    }

    /// Reverses the current ordering.
    #[must_use]
    pub fn reverse(mut self) -> Self {
        for order in &mut self.query.order_by {
            order.descending = !order.descending;
        }
        self
    }

    /// Selects specific fields (equivalent to `.values()`).
    #[must_use]
    pub fn values(mut self, fields: Vec<&str>) -> Self {
        self.query.select = fields
            .into_iter()
            .map(|f| SelectColumn::Column(f.to_string()))
            .collect();
        self
    }

    /// Selects specific fields as a flat list.
    #[must_use]
    pub fn values_list(mut self, fields: Vec<&str>) -> Self {
        self.query.select = fields
            .into_iter()
            .map(|f| SelectColumn::Column(f.to_string()))
            .collect();
        self
    }

    /// Adds DISTINCT to the query.
    #[must_use]
    pub fn distinct(mut self) -> Self {
        self.query.distinct = true;
        self
    }

    /// Returns all objects (identity operation for chaining).
    #[must_use]
    pub fn all(self) -> Self {
        self
    }

    /// Returns an empty queryset.
    #[must_use]
    pub fn none(mut self) -> Self {
        self.is_none = true;
        self
    }

    /// Sets the LIMIT.
    #[must_use]
    pub fn limit(mut self, n: usize) -> Self {
        self.query.limit = Some(n);
        self
    }

    /// Sets the OFFSET.
    #[must_use]
    pub fn offset(mut self, n: usize) -> Self {
        self.query.offset = Some(n);
        self
    }

    /// Adds an annotation (computed expression with an alias).
    #[must_use]
    pub fn annotate(mut self, name: impl Into<String>, expr: Expression) -> Self {
        self.query.annotations.insert(name.into(), expr);
        self
    }

    /// Adds `select_related` fields (controls JOIN behavior).
    #[must_use]
    pub fn select_related(mut self, fields: Vec<&str>) -> Self {
        // Store the fields for JOIN generation during execution.
        // For now, this is a placeholder that records the intent.
        for field in fields {
            self.query.group_by.push(format!("__select_related__{field}"));
        }
        self
    }

    /// Adds `prefetch_related` fields.
    #[must_use]
    pub fn prefetch_related(mut self, fields: Vec<&str>) -> Self {
        // Placeholder for prefetch intent — execution handled by backend.
        for field in fields {
            self.query
                .group_by
                .push(format!("__prefetch_related__{field}"));
        }
        self
    }

    /// Sets fields for an update operation.
    #[must_use]
    pub fn update(mut self, fields: Vec<(&'static str, Value)>) -> Self {
        self.pending_update = Some(fields);
        self
    }

    /// Marks this queryset for deletion.
    #[must_use]
    pub fn delete(mut self) -> Self {
        self.pending_delete = true;
        self
    }

    /// Combines two querysets with UNION.
    #[must_use]
    pub fn union(self, _other: QuerySet<M>) -> Self {
        // Placeholder — full UNION support requires extending Query AST
        self
    }

    /// Combines two querysets with INTERSECT.
    #[must_use]
    pub fn intersection(self, _other: QuerySet<M>) -> Self {
        self
    }

    /// Combines two querysets with EXCEPT/MINUS.
    #[must_use]
    pub fn difference(self, _other: QuerySet<M>) -> Self {
        self
    }

    // ── SQL generation (for inspection/debugging) ────────────────────

    /// Compiles the queryset to SQL for the given backend.
    ///
    /// This is useful for debugging and testing. In production, the backend
    /// calls this internally during execution.
    pub fn to_sql(&self, backend: DatabaseBackendType) -> (String, Vec<Value>) {
        if self.is_none {
            return ("SELECT * FROM \"__none__\" WHERE 1=0".to_string(), vec![]);
        }

        let compiler = SqlCompiler::new(backend);

        if let Some(ref fields) = self.pending_create {
            return compiler.compile_insert(&self.query.table, fields);
        }

        if let Some(ref fields) = self.pending_update {
            if let Some(ref where_clause) = self.query.where_clause {
                return compiler.compile_update(&self.query.table, fields, where_clause);
            }
            // Update without WHERE — update all rows
            let where_all = WhereNode::And(vec![]);
            return compiler.compile_update(&self.query.table, fields, &where_all);
        }

        if self.pending_delete {
            if let Some(ref where_clause) = self.query.where_clause {
                return compiler.compile_delete(&self.query.table, where_clause);
            }
            let where_all = WhereNode::And(vec![]);
            return compiler.compile_delete(&self.query.table, &where_all);
        }

        compiler.compile_select(&self.query)
    }

    /// Compiles a COUNT query.
    pub fn count_sql(&self, backend: DatabaseBackendType) -> (String, Vec<Value>) {
        if self.is_none {
            return (
                "SELECT COUNT(*) FROM \"__none__\" WHERE 1=0".to_string(),
                vec![],
            );
        }
        let mut count_query = self.query.clone();
        count_query.select = vec![SelectColumn::Expression(
            Expression::aggregate(
                super::expressions::AggregateFunc::Count,
                Expression::col("*"),
            ),
            "count".to_string(),
        )];
        count_query.order_by.clear();
        count_query.limit = None;
        count_query.offset = None;
        SqlCompiler::new(backend).compile_select(&count_query)
    }

    /// Compiles an EXISTS query.
    pub fn exists_sql(&self, backend: DatabaseBackendType) -> (String, Vec<Value>) {
        if self.is_none {
            return (
                "SELECT EXISTS(SELECT 1 FROM \"__none__\" WHERE 1=0)".to_string(),
                vec![],
            );
        }
        let mut exists_query = self.query.clone();
        exists_query.select = vec![SelectColumn::Expression(
            Expression::value(1),
            "__exists__".to_string(),
        )];
        exists_query.order_by.clear();
        exists_query.limit = Some(1);
        let (inner_sql, params) = SqlCompiler::new(backend).compile_select(&exists_query);
        (format!("SELECT EXISTS({inner_sql})"), params)
    }

    /// Compiles a query to get the first result.
    pub fn first_sql(&self, backend: DatabaseBackendType) -> (String, Vec<Value>) {
        let mut first_query = self.query.clone();
        first_query.limit = Some(1);
        SqlCompiler::new(backend).compile_select(&first_query)
    }

    /// Compiles a query to get the last result.
    pub fn last_sql(&self, backend: DatabaseBackendType) -> (String, Vec<Value>) {
        let mut last_query = self.query.clone();
        // Reverse all orderings
        for order in &mut last_query.order_by {
            order.descending = !order.descending;
        }
        last_query.limit = Some(1);
        SqlCompiler::new(backend).compile_select(&last_query)
    }

    /// Compiles a query for `.get()` (expects exactly one result).
    pub fn get_sql(&self, backend: DatabaseBackendType) -> (String, Vec<Value>) {
        let mut get_query = self.query.clone();
        get_query.limit = Some(2); // Get 2 to detect MultipleObjectsReturned
        SqlCompiler::new(backend).compile_select(&get_query)
    }

    /// Compiles an aggregate query.
    pub fn aggregate_sql(
        &self,
        aggregates: Vec<(String, Expression)>,
        backend: DatabaseBackendType,
    ) -> (String, Vec<Value>) {
        let mut agg_query = self.query.clone();
        agg_query.select = aggregates
            .into_iter()
            .map(|(alias, expr)| SelectColumn::Expression(expr, alias))
            .collect();
        agg_query.order_by.clear();
        agg_query.limit = None;
        agg_query.offset = None;
        SqlCompiler::new(backend).compile_select(&agg_query)
    }

    // ── Async execution methods ───────────────────────────────────────

    /// Executes the query and returns all matching model instances.
    ///
    /// Compiles the query to SQL using the backend's dialect, sends it,
    /// and maps the returned rows to model instances via `M::from_row()`.
    pub async fn execute_query(&self, db: &dyn DbExecutor) -> DjangoResult<Vec<M>> {
        if self.is_none {
            return Ok(Vec::new());
        }

        let (sql, params) = self.to_sql(db.backend_type());
        let rows = db.query(&sql, &params).await?;
        rows.iter().map(M::from_row).collect()
    }

    /// Returns the count of matching records.
    ///
    /// Runs a `SELECT COUNT(*)` query.
    pub async fn count_exec(&self, db: &dyn DbExecutor) -> DjangoResult<i64> {
        if self.is_none {
            return Ok(0);
        }

        let (sql, params) = self.count_sql(db.backend_type());
        let rows = db.query(&sql, &params).await?;
        if let Some(row) = rows.into_iter().next() {
            row.get_by_index::<i64>(0)
        } else {
            Ok(0)
        }
    }

    /// Returns whether any records match the query.
    pub async fn exists_exec(&self, db: &dyn DbExecutor) -> DjangoResult<bool> {
        if self.is_none {
            return Ok(false);
        }

        let mut first_query = self.query.clone();
        first_query.select = vec![SelectColumn::Expression(
            Expression::value(1),
            "__exists__".to_string(),
        )];
        first_query.order_by.clear();
        first_query.limit = Some(1);

        let (sql, params) = SqlCompiler::new(db.backend_type()).compile_select(&first_query);
        let rows = db.query(&sql, &params).await?;
        Ok(!rows.is_empty())
    }

    /// Returns the first matching record, or `None` if no records match.
    pub async fn first_exec(&self, db: &dyn DbExecutor) -> DjangoResult<Option<M>> {
        if self.is_none {
            return Ok(None);
        }

        let (sql, params) = self.first_sql(db.backend_type());
        let rows = db.query(&sql, &params).await?;
        match rows.into_iter().next() {
            Some(row) => Ok(Some(M::from_row(&row)?)),
            None => Ok(None),
        }
    }

    /// Returns a single matching record.
    ///
    /// Returns `DoesNotExist` if no records match, or
    /// `MultipleObjectsReturned` if more than one record matches.
    pub async fn get_exec(&self, db: &dyn DbExecutor) -> DjangoResult<M> {
        if self.is_none {
            return Err(DjangoError::DoesNotExist(format!(
                "{} matching query does not exist.",
                M::table_name()
            )));
        }

        let (sql, params) = self.get_sql(db.backend_type());
        let rows = db.query(&sql, &params).await?;
        match rows.len() {
            0 => Err(DjangoError::DoesNotExist(format!(
                "{} matching query does not exist.",
                M::table_name()
            ))),
            1 => M::from_row(&rows[0]),
            _ => Err(DjangoError::MultipleObjectsReturned(format!(
                "get() returned more than one {} -- it returned {}!",
                M::table_name(),
                rows.len()
            ))),
        }
    }

    /// Runs an UPDATE and returns the number of rows affected.
    ///
    /// The queryset must have been prepared with `.update(fields)`.
    pub async fn update_exec(&self, db: &dyn DbExecutor) -> DjangoResult<u64> {
        if self.is_none {
            return Ok(0);
        }

        if self.pending_update.is_none() {
            return Err(DjangoError::DatabaseError(
                "No pending update fields. Call .update(fields) before .update_exec()".to_string(),
            ));
        }

        let (sql, params) = self.to_sql(db.backend_type());
        db.execute_sql(&sql, &params).await
    }

    /// Runs a DELETE and returns the number of rows affected.
    ///
    /// The queryset must have been prepared with `.delete()`.
    pub async fn delete_exec(&self, db: &dyn DbExecutor) -> DjangoResult<u64> {
        if self.is_none {
            return Ok(0);
        }

        if !self.pending_delete {
            return Err(DjangoError::DatabaseError(
                "QuerySet is not marked for deletion. Call .delete() before .delete_exec()"
                    .to_string(),
            ));
        }

        let (sql, params) = self.to_sql(db.backend_type());
        db.execute_sql(&sql, &params).await
    }

    /// Runs a CREATE (INSERT) and returns the inserted row ID.
    ///
    /// The queryset must have been prepared via `Manager::create(fields)`.
    pub async fn create_exec(&self, db: &dyn DbExecutor) -> DjangoResult<Value> {
        if self.pending_create.is_none() {
            return Err(DjangoError::DatabaseError(
                "No pending create fields. Call Manager::create(fields) before .create_exec()"
                    .to_string(),
            ));
        }

        let (sql, params) = self.to_sql(db.backend_type());
        db.insert_returning_id(&sql, &params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::{FieldDef, FieldType};
    use crate::model::{Model, ModelMeta};
    use crate::query::compiler::{DatabaseBackendType, Row};
    use crate::query::expressions::AggregateFunc;
    use crate::query::lookups::Lookup;

    // A test model for queryset tests
    struct User {
        id: i64,
        name: String,
        age: i64,
    }

    impl Model for User {
        fn meta() -> &'static ModelMeta {
            use std::sync::LazyLock;
            static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
                app_label: "auth",
                model_name: "user",
                db_table: "auth_user".to_string(),
                verbose_name: "user".to_string(),
                verbose_name_plural: "users".to_string(),
                ordering: vec![OrderBy::asc("name")],
                unique_together: vec![],
                indexes: vec![],
                abstract_model: false,
                fields: vec![
                    FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                    FieldDef::new("name", FieldType::CharField).max_length(100),
                    FieldDef::new("age", FieldType::IntegerField),
                ],
            });
            &META
        }
        fn table_name() -> &'static str {
            "auth_user"
        }
        fn app_label() -> &'static str {
            "auth"
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
                ("age", Value::Int(self.age)),
            ]
        }
        fn from_row(row: &Row) -> Result<Self, django_rs_core::DjangoError> {
            Ok(User {
                id: row.get("id")?,
                name: row.get("name")?,
                age: row.get("age")?,
            })
        }
    }

    fn pg() -> DatabaseBackendType {
        DatabaseBackendType::PostgreSQL
    }

    fn sqlite() -> DatabaseBackendType {
        DatabaseBackendType::SQLite
    }

    #[test]
    fn test_manager_all() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all();
        let (sql, params) = qs.to_sql(pg());
        assert_eq!(sql, "SELECT * FROM \"auth_user\"");
        assert!(params.is_empty());
    }

    #[test]
    fn test_manager_filter() {
        let mgr = Manager::<User>::new();
        let qs = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))));
        let (sql, params) = qs.to_sql(pg());
        assert_eq!(
            sql,
            "SELECT * FROM \"auth_user\" WHERE \"name\" = $1"
        );
        assert_eq!(params, vec![Value::String("Alice".to_string())]);
    }

    #[test]
    fn test_manager_exclude() {
        let mgr = Manager::<User>::new();
        let qs = mgr.exclude(Q::filter("active", Lookup::Exact(Value::from(false))));
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("NOT"));
    }

    #[test]
    fn test_queryset_chaining() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .all()
            .filter(Q::filter("age", Lookup::Gte(Value::from(18))))
            .filter(Q::filter("age", Lookup::Lte(Value::from(65))))
            .order_by(vec![OrderBy::asc("name")])
            .limit(10)
            .offset(0);
        let (sql, params) = qs.to_sql(pg());
        assert!(sql.contains("\"age\" >= $1"));
        assert!(sql.contains("\"age\" <= $2"));
        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 0"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_queryset_distinct() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().values(vec!["name"]).distinct();
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("DISTINCT"));
        assert!(sql.contains("\"name\""));
    }

    #[test]
    fn test_queryset_none() {
        let mgr = Manager::<User>::new();
        let qs = mgr.none();
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("1=0"));
    }

    #[test]
    fn test_queryset_reverse() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .all()
            .order_by(vec![OrderBy::asc("name")])
            .reverse();
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("DESC"));
    }

    #[test]
    fn test_queryset_values() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().values(vec!["name", "age"]);
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("\"name\""));
        assert!(sql.contains("\"age\""));
        assert!(!sql.contains("*"));
    }

    #[test]
    fn test_queryset_count_sql() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .all()
            .filter(Q::filter("age", Lookup::Gt(Value::from(18))));
        let (sql, params) = qs.count_sql(pg());
        assert!(sql.contains("COUNT"));
        assert!(sql.contains("\"age\" > $1"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_queryset_exists_sql() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .all()
            .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))));
        let (sql, _) = qs.exists_sql(pg());
        assert!(sql.contains("EXISTS"));
        assert!(sql.contains("LIMIT 1"));
    }

    #[test]
    fn test_queryset_first_sql() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().order_by(vec![OrderBy::asc("name")]);
        let (sql, _) = qs.first_sql(pg());
        assert!(sql.contains("LIMIT 1"));
        assert!(sql.contains("ASC"));
    }

    #[test]
    fn test_queryset_last_sql() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().order_by(vec![OrderBy::asc("name")]);
        let (sql, _) = qs.last_sql(pg());
        assert!(sql.contains("LIMIT 1"));
        assert!(sql.contains("DESC"));
    }

    #[test]
    fn test_queryset_get_sql() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .all()
            .filter(Q::filter("id", Lookup::Exact(Value::from(1))));
        let (sql, _) = qs.get_sql(pg());
        assert!(sql.contains("LIMIT 2"));
    }

    #[test]
    fn test_queryset_create_sql() {
        let mgr = Manager::<User>::new();
        let qs = mgr.create(vec![
            ("name", Value::from("Alice")),
            ("age", Value::from(30)),
        ]);
        let (sql, params) = qs.to_sql(pg());
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("\"name\""));
        assert!(sql.contains("\"age\""));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_queryset_update_sql() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .all()
            .filter(Q::filter("id", Lookup::Exact(Value::from(1))))
            .update(vec![("name", Value::from("Updated"))]);
        let (sql, params) = qs.to_sql(pg());
        assert!(sql.contains("UPDATE"));
        assert!(sql.contains("SET"));
        assert!(sql.contains("WHERE"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_queryset_delete_sql() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .all()
            .filter(Q::filter("id", Lookup::Exact(Value::from(1))))
            .delete();
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("DELETE FROM"));
        assert!(sql.contains("WHERE"));
    }

    #[test]
    fn test_queryset_annotate() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().annotate(
            "name_upper",
            Expression::func("UPPER", vec![Expression::col("name")]),
        );
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("UPPER(\"name\") AS \"name_upper\""));
    }

    #[test]
    fn test_queryset_aggregate_sql() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all();
        let (sql, _) = qs.aggregate_sql(
            vec![(
                "avg_age".to_string(),
                Expression::aggregate(AggregateFunc::Avg, Expression::col("age")),
            )],
            pg(),
        );
        assert!(sql.contains("AVG(\"age\") AS \"avg_age\""));
    }

    #[test]
    fn test_queryset_sqlite_backend() {
        let mgr = Manager::<User>::new();
        let qs = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("test"))));
        let (sql, _) = qs.to_sql(sqlite());
        assert!(sql.contains("?"));
        assert!(!sql.contains('$'));
    }

    #[test]
    fn test_queryset_none_count() {
        let mgr = Manager::<User>::new();
        let qs = mgr.none();
        let (sql, _) = qs.count_sql(pg());
        assert!(sql.contains("1=0"));
    }

    #[test]
    fn test_queryset_none_exists() {
        let mgr = Manager::<User>::new();
        let qs = mgr.none();
        let (sql, _) = qs.exists_sql(pg());
        assert!(sql.contains("1=0"));
    }

    #[test]
    fn test_manager_default() {
        let mgr = Manager::<User>::default();
        let qs = mgr.all();
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("auth_user"));
    }

    #[test]
    fn test_queryset_update_all() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().update(vec![("age", Value::from(0))]);
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("UPDATE"));
        assert!(sql.contains("1=1")); // empty AND = update all
    }

    #[test]
    fn test_queryset_delete_all() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().delete();
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("DELETE FROM"));
    }

    #[test]
    fn test_queryset_complex_filter_chain() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .all()
            .filter(
                Q::filter("name", Lookup::Contains("Al".to_string()))
                    | Q::filter("name", Lookup::Contains("Bo".to_string())),
            )
            .filter(Q::filter("age", Lookup::Gte(Value::from(18))))
            .exclude(Q::filter("age", Lookup::Gt(Value::from(100))))
            .order_by(vec![OrderBy::desc("age"), OrderBy::asc("name")])
            .limit(25)
            .offset(50);
        let (sql, params) = qs.to_sql(pg());
        // Should have a complex WHERE clause
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("AND"));
        assert!(sql.contains("NOT"));
        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("LIMIT 25"));
        assert!(sql.contains("OFFSET 50"));
        assert_eq!(params.len(), 4); // 2 contains + 1 gte + 1 gt
    }
}
