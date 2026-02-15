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
    CompoundQuery, CompoundType, DatabaseBackendType, InheritanceType, OrderBy,
    PrefetchRelatedField, Query, SelectColumn, SelectRelatedField, SqlCompiler, WhereNode,
};
use super::expressions::Expression;
use super::lookups::Q;
use crate::executor::DbExecutor;
use crate::model::Model;
use crate::value::Value;
use django_rs_core::{DjangoError, DjangoResult};
use std::collections::HashMap;
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

    /// Forces this queryset to use a specific database connection.
    ///
    /// This is the equivalent of Django's `QuerySet.using(db)`. The `db`
    /// parameter is an alias that corresponds to a key in the `DATABASES`
    /// configuration.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let qs = Article::objects().all().using("replica");
    /// ```
    #[must_use]
    pub fn using(mut self, db: impl Into<String>) -> Self {
        self.using = Some(db.into());
        self
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
    ///
    /// When select_related fields are configured with `select_related_with()`,
    /// the SQL compiler generates LEFT OUTER JOINs to eagerly load related
    /// objects in a single query. This is equivalent to Django's
    /// `QuerySet.select_related()`.
    ///
    /// This simpler version stores field names as hints. For full functionality
    /// with actual JOIN generation, use `select_related_with()`.
    #[must_use]
    pub fn select_related(self, fields: Vec<&str>) -> Self {
        // For backward compatibility, store field names as hints in group_by.
        // Real JOIN generation uses select_related_with().
        let mut qs = self;
        for field in fields {
            qs.query.group_by.push(format!("__select_related__{field}"));
        }
        qs
    }

    /// Adds `select_related` fields with full relation metadata for JOIN generation.
    ///
    /// Each entry provides the field name, related table, FK column, related PK column,
    /// and a table alias for the JOIN. The SQL compiler generates LEFT OUTER JOINs
    /// and the result set includes columns from the joined tables.
    #[must_use]
    pub fn select_related_with(mut self, fields: Vec<SelectRelatedField>) -> Self {
        self.query.select_related.extend(fields);
        self
    }

    /// Adds `prefetch_related` fields.
    ///
    /// This simpler version stores field names as hints. For full functionality
    /// with actual batch queries, use `prefetch_related_with()`.
    #[must_use]
    pub fn prefetch_related(self, fields: Vec<&str>) -> Self {
        let mut qs = self;
        for field in fields {
            qs.query
                .group_by
                .push(format!("__prefetch_related__{field}"));
        }
        qs
    }

    /// Adds `prefetch_related` fields with full relation metadata.
    ///
    /// After the main query executes, additional batch queries are issued
    /// for each prefetch field to load related objects. The results are
    /// returned alongside the main query results via `execute_with_prefetch()`.
    #[must_use]
    pub fn prefetch_related_with(mut self, fields: Vec<PrefetchRelatedField>) -> Self {
        self.query.prefetch_related.extend(fields);
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
    ///
    /// By default, UNION deduplicates rows. Pass `all=true` to use UNION ALL
    /// which preserves duplicates and is faster.
    #[must_use]
    pub fn union(mut self, other: QuerySet<M>) -> Self {
        self.query.compound_queries.push(CompoundQuery {
            compound_type: CompoundType::Union,
            other: Box::new(other.query),
        });
        self
    }

    /// Combines two querysets with UNION ALL (preserves duplicates).
    #[must_use]
    pub fn union_all(mut self, other: QuerySet<M>) -> Self {
        self.query.compound_queries.push(CompoundQuery {
            compound_type: CompoundType::UnionAll,
            other: Box::new(other.query),
        });
        self
    }

    /// Combines two querysets with INTERSECT.
    ///
    /// Returns only rows that appear in both querysets.
    #[must_use]
    pub fn intersection(mut self, other: QuerySet<M>) -> Self {
        self.query.compound_queries.push(CompoundQuery {
            compound_type: CompoundType::Intersect,
            other: Box::new(other.query),
        });
        self
    }

    /// Combines two querysets with EXCEPT (MINUS on some backends).
    ///
    /// Returns rows from this queryset that do not appear in the other.
    #[must_use]
    pub fn difference(mut self, other: QuerySet<M>) -> Self {
        self.query.compound_queries.push(CompoundQuery {
            compound_type: CompoundType::Except,
            other: Box::new(other.query),
        });
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

    /// Executes the main query and then runs prefetch_related batch queries.
    ///
    /// Returns a tuple of `(models, prefetch_cache)` where `prefetch_cache` is a
    /// `HashMap<String, Vec<Row>>` mapping each prefetch field name to the rows
    /// returned by its batch query.
    ///
    /// This is the async execution counterpart of `prefetch_related_with()`.
    pub async fn execute_with_prefetch(
        &self,
        db: &dyn DbExecutor,
    ) -> DjangoResult<(Vec<M>, HashMap<String, Vec<super::compiler::Row>>)> {
        if self.is_none {
            return Ok((Vec::new(), HashMap::new()));
        }

        // Execute the main query
        let (sql, params) = self.to_sql(db.backend_type());
        let rows = db.query(&sql, &params).await?;
        let models: Vec<M> = rows.iter().map(M::from_row).collect::<Result<Vec<_>, _>>()?;

        // Collect PK values from results for the prefetch IN clause
        let pk_values: Vec<Value> = models
            .iter()
            .filter_map(|m| m.pk().cloned())
            .collect();

        // Run prefetch queries
        let compiler = SqlCompiler::new(db.backend_type());
        let prefetch_queries =
            compiler.compile_prefetch_queries(&self.query.prefetch_related, &pk_values);

        let mut prefetch_cache = HashMap::new();
        for (field_name, pf_sql, pf_params) in prefetch_queries {
            let pf_rows = db.query(&pf_sql, &pf_params).await?;
            prefetch_cache.insert(field_name, pf_rows);
        }

        Ok((models, prefetch_cache))
    }

    /// Sets the inheritance type on the underlying query.
    ///
    /// This configures how the SQL compiler generates queries for models
    /// with multi-table or proxy inheritance.
    #[must_use]
    pub fn set_inheritance(mut self, inheritance: InheritanceType) -> Self {
        self.query.inheritance = inheritance;
        self
    }
}

/// Result of a prefetch_related query, containing the main query results
/// and a cache of related objects keyed by field name.
#[derive(Debug)]
pub struct PrefetchResult<M: Model> {
    /// The main query result models.
    pub models: Vec<M>,
    /// Cached prefetch query results, keyed by field name.
    pub prefetch_cache: HashMap<String, Vec<super::compiler::Row>>,
}

impl<M: Model> PrefetchResult<M> {
    /// Returns a reference to the prefetched rows for the given field.
    pub fn get_prefetched(&self, field_name: &str) -> Option<&Vec<super::compiler::Row>> {
        self.prefetch_cache.get(field_name)
    }

    /// Returns the number of main result models.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Returns true if there are no main result models.
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
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
                constraints: vec![],
                inheritance_type: crate::query::compiler::InheritanceType::None,
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

    // ── UNION / INTERSECT / EXCEPT queryset tests ────────────────────

    #[test]
    fn test_queryset_union() {
        let mgr = Manager::<User>::new();
        let qs1 = mgr.filter(Q::filter("age", Lookup::Lt(Value::from(25))));
        let qs2 = mgr.filter(Q::filter("age", Lookup::Gt(Value::from(60))));
        let combined = qs1.union(qs2);
        let (sql, params) = combined.to_sql(pg());
        assert!(sql.contains("UNION"));
        assert!(!sql.contains("UNION ALL"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_queryset_union_all() {
        let mgr = Manager::<User>::new();
        let qs1 = mgr.all();
        let qs2 = mgr.all();
        let combined = qs1.union_all(qs2);
        let (sql, _) = combined.to_sql(pg());
        assert!(sql.contains("UNION ALL"));
    }

    #[test]
    fn test_queryset_intersection() {
        let mgr = Manager::<User>::new();
        let qs1 = mgr.filter(Q::filter("age", Lookup::Gte(Value::from(18))));
        let qs2 = mgr.filter(Q::filter("age", Lookup::Lte(Value::from(65))));
        let combined = qs1.intersection(qs2);
        let (sql, params) = combined.to_sql(pg());
        assert!(sql.contains("INTERSECT"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_queryset_difference() {
        let mgr = Manager::<User>::new();
        let qs1 = mgr.all();
        let qs2 = mgr.filter(Q::filter("age", Lookup::Lt(Value::from(18))));
        let combined = qs1.difference(qs2);
        let (sql, params) = combined.to_sql(pg());
        assert!(sql.contains("EXCEPT"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_queryset_union_with_order_by() {
        let mgr = Manager::<User>::new();
        let qs1 = mgr.filter(Q::filter("age", Lookup::Lt(Value::from(25))));
        let qs2 = mgr.filter(Q::filter("age", Lookup::Gt(Value::from(60))));
        let combined = qs1.union(qs2).order_by(vec![OrderBy::asc("name")]);
        let (sql, _) = combined.to_sql(pg());
        let union_pos = sql.find("UNION").unwrap();
        let order_pos = sql.find("ORDER BY").unwrap();
        assert!(order_pos > union_pos);
    }

    #[test]
    fn test_queryset_union_with_limit() {
        let mgr = Manager::<User>::new();
        let qs1 = mgr.all();
        let qs2 = mgr.all();
        let combined = qs1.union(qs2).limit(10);
        let (sql, _) = combined.to_sql(pg());
        assert!(sql.contains("UNION"));
        assert!(sql.contains("LIMIT 10"));
    }

    #[test]
    fn test_queryset_chained_unions() {
        let mgr = Manager::<User>::new();
        let qs1 = mgr.filter(Q::filter("age", Lookup::Lt(Value::from(20))));
        let qs2 = mgr.filter(Q::filter("age", Lookup::Gt(Value::from(60))));
        let qs3 = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Admin"))));
        let combined = qs1.union(qs2).union(qs3);
        let (sql, params) = combined.to_sql(pg());
        assert_eq!(sql.matches("UNION").count(), 2);
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_queryset_union_sqlite() {
        let mgr = Manager::<User>::new();
        let qs1 = mgr.filter(Q::filter("age", Lookup::Lt(Value::from(25))));
        let qs2 = mgr.filter(Q::filter("age", Lookup::Gt(Value::from(60))));
        let combined = qs1.union(qs2);
        let (sql, _) = combined.to_sql(sqlite());
        assert!(sql.contains("UNION"));
        assert!(!sql.contains('$'));
    }

    // ── select_related queryset tests ────────────────────────────────

    #[test]
    fn test_queryset_select_related_with() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().select_related_with(vec![
            crate::query::compiler::SelectRelatedField {
                field_name: "profile".to_string(),
                related_table: "auth_profile".to_string(),
                fk_column: "profile_id".to_string(),
                related_column: "id".to_string(),
                alias: "profile".to_string(),
            },
        ]);
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("LEFT JOIN \"auth_profile\" AS \"profile\""));
        assert!(sql.contains("\"auth_user\".\"profile_id\" = \"profile\".\"id\""));
    }

    #[test]
    fn test_queryset_select_related_with_filter() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .filter(Q::filter("age", Lookup::Gte(Value::from(18))))
            .select_related_with(vec![crate::query::compiler::SelectRelatedField {
                field_name: "department".to_string(),
                related_table: "org_department".to_string(),
                fk_column: "department_id".to_string(),
                related_column: "id".to_string(),
                alias: "dept".to_string(),
            }]);
        let (sql, params) = qs.to_sql(pg());
        assert!(sql.contains("LEFT JOIN \"org_department\" AS \"dept\""));
        assert!(sql.contains("WHERE"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_queryset_select_related_hint_backward_compat() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().select_related(vec!["author", "category"]);
        // The hint-style select_related stores in group_by but they're filtered out
        let (sql, _) = qs.to_sql(pg());
        assert!(!sql.contains("GROUP BY"));
        assert!(!sql.contains("__select_related__"));
    }

    // ── prefetch_related queryset tests ──────────────────────────────

    #[test]
    fn test_queryset_prefetch_related_with() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().prefetch_related_with(vec![
            crate::query::compiler::PrefetchRelatedField {
                field_name: "orders".to_string(),
                related_table: "shop_order".to_string(),
                source_column: "id".to_string(),
                related_column: "user_id".to_string(),
            },
        ]);
        // Main query should be normal (no JOIN)
        let (sql, _) = qs.to_sql(pg());
        assert!(!sql.contains("JOIN"));
        assert!(sql.contains("FROM \"auth_user\""));
        // But the prefetch data is stored in the query
        assert_eq!(qs.query().prefetch_related.len(), 1);
    }

    #[test]
    fn test_queryset_prefetch_related_hint_backward_compat() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().prefetch_related(vec!["comments"]);
        let (sql, _) = qs.to_sql(pg());
        assert!(!sql.contains("GROUP BY"));
        assert!(!sql.contains("__prefetch_related__"));
    }

    // ── Model inheritance queryset tests ─────────────────────────────

    #[test]
    fn test_queryset_set_inheritance_proxy() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().set_inheritance(
            crate::query::compiler::InheritanceType::Proxy {
                parent_table: "base_user".to_string(),
            },
        );
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("FROM \"base_user\""));
        assert!(!sql.contains("FROM \"auth_user\""));
    }

    #[test]
    fn test_queryset_set_inheritance_multi_table() {
        let mgr = Manager::<User>::new();
        let qs = mgr.all().set_inheritance(
            crate::query::compiler::InheritanceType::MultiTable {
                parent_table: "base_person".to_string(),
                parent_link_column: "person_ptr_id".to_string(),
                parent_pk_column: "id".to_string(),
            },
        );
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("FROM \"auth_user\""));
        assert!(sql.contains("INNER JOIN \"base_person\""));
        assert!(sql.contains(
            "\"auth_user\".\"person_ptr_id\" = \"base_person\".\"id\""
        ));
    }

    #[test]
    fn test_queryset_multi_table_with_filter() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
            .set_inheritance(crate::query::compiler::InheritanceType::MultiTable {
                parent_table: "base_person".to_string(),
                parent_link_column: "person_ptr_id".to_string(),
                parent_pk_column: "id".to_string(),
            });
        let (sql, params) = qs.to_sql(pg());
        assert!(sql.contains("INNER JOIN \"base_person\""));
        assert!(sql.contains("WHERE \"name\" = $1"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_queryset_proxy_with_select_related() {
        let mgr = Manager::<User>::new();
        let qs = mgr
            .all()
            .set_inheritance(crate::query::compiler::InheritanceType::Proxy {
                parent_table: "base_user".to_string(),
            })
            .select_related_with(vec![crate::query::compiler::SelectRelatedField {
                field_name: "group".to_string(),
                related_table: "auth_group".to_string(),
                fk_column: "group_id".to_string(),
                related_column: "id".to_string(),
                alias: "grp".to_string(),
            }]);
        let (sql, _) = qs.to_sql(pg());
        assert!(sql.contains("FROM \"base_user\""));
        assert!(sql.contains("LEFT JOIN \"auth_group\" AS \"grp\""));
        assert!(sql.contains("\"base_user\".\"group_id\" = \"grp\".\"id\""));
    }

    // ── PrefetchResult tests ─────────────────────────────────────────

    #[test]
    fn test_prefetch_result_accessors() {
        use super::super::compiler::Row;
        let result = super::PrefetchResult::<User> {
            models: vec![
                User { id: 1, name: "Alice".to_string(), age: 30 },
                User { id: 2, name: "Bob".to_string(), age: 25 },
            ],
            prefetch_cache: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "orders".to_string(),
                    vec![Row::new(
                        vec!["id".to_string(), "user_id".to_string()],
                        vec![Value::Int(1), Value::Int(1)],
                    )],
                );
                m
            },
        };
        assert_eq!(result.len(), 2);
        assert!(!result.is_empty());
        assert!(result.get_prefetched("orders").is_some());
        assert_eq!(result.get_prefetched("orders").unwrap().len(), 1);
        assert!(result.get_prefetched("nonexistent").is_none());
    }

    #[test]
    fn test_prefetch_result_empty() {
        let result = super::PrefetchResult::<User> {
            models: vec![],
            prefetch_cache: std::collections::HashMap::new(),
        };
        assert_eq!(result.len(), 0);
        assert!(result.is_empty());
    }
}
