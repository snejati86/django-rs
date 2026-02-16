//! Database constraints for model-level integrity rules.
//!
//! This module provides [`CheckConstraint`], [`UniqueConstraint`], and [`ExclusionConstraint`] types that
//! correspond to Django's `CheckConstraint`, `UniqueConstraint`, and `ExclusionConstraint` from
//! `django.db.models.constraints`. They are declared in a model's `Meta.constraints`
//! list and generate SQL during migration/schema operations.
//!
//! # Examples
//!
//! ```
//! use django_rs_db::constraints::{CheckConstraint, UniqueConstraint, Constraint};
//! use django_rs_db::query::lookups::{Q, Lookup};
//! use django_rs_db::value::Value;
//!
//! // CHECK (age >= 0)
//! let check = CheckConstraint::new(
//!     "age_non_negative",
//!     Q::filter("age", Lookup::Gte(Value::from(0))),
//! );
//!
//! // UNIQUE (email) -- simple unique constraint
//! let unique = UniqueConstraint::new("unique_email", vec!["email".to_string()]);
//!
//! // Generate the SQL
//! assert!(check.to_sql("users").contains("CHECK"));
//! assert!(unique.to_sql("users").contains("UNIQUE"));
//! ```

use crate::query::compiler::{DatabaseBackendType, SqlCompiler, WhereNode};
use crate::query::lookups::Q;
use crate::value::Value;

/// Trait for all database constraint types.
///
/// Constraints generate SQL that can be used in CREATE TABLE statements
/// or ALTER TABLE ADD CONSTRAINT commands.
pub trait Constraint: std::fmt::Debug + Send + Sync {
    /// Returns the constraint name.
    fn name(&self) -> &str;

    /// Generates the SQL DDL for this constraint on the given table.
    fn to_sql(&self, table: &str) -> String;

    /// Generates the SQL DDL for adding this constraint to an existing table.
    fn create_sql(&self, table: &str) -> String {
        format!(
            "ALTER TABLE \"{}\" ADD CONSTRAINT {}",
            table,
            self.to_sql(table)
        )
    }

    /// Generates the SQL DDL for removing this constraint from a table.
    fn drop_sql(&self, table: &str) -> String {
        format!(
            "ALTER TABLE \"{}\" DROP CONSTRAINT \"{}\"",
            table,
            self.name()
        )
    }
}

/// A CHECK constraint that enforces an arbitrary condition on rows.
///
/// This is the equivalent of Django's `CheckConstraint`. It wraps a [`Q`] object
/// that defines the condition. The condition is compiled to SQL using the
/// standard SQL compiler.
///
/// # Examples
///
/// ```
/// use django_rs_db::constraints::{CheckConstraint, Constraint};
/// use django_rs_db::query::lookups::{Q, Lookup};
/// use django_rs_db::value::Value;
///
/// // Ensure price is always positive
/// let constraint = CheckConstraint::new(
///     "price_positive",
///     Q::filter("price", Lookup::Gt(Value::from(0))),
/// );
/// let sql = constraint.to_sql("products");
/// assert!(sql.contains("CHECK"));
/// assert!(sql.contains("price_positive"));
/// ```
#[derive(Debug, Clone)]
pub struct CheckConstraint {
    /// The constraint name.
    name: String,
    /// The condition that must be satisfied.
    condition: Q,
}

impl CheckConstraint {
    /// Creates a new check constraint.
    pub fn new(name: impl Into<String>, condition: Q) -> Self {
        Self {
            name: name.into(),
            condition,
        }
    }

    /// Returns the condition Q object.
    pub fn condition(&self) -> &Q {
        &self.condition
    }

    /// Compiles the condition to SQL for the given backend.
    pub fn condition_sql(&self, backend: DatabaseBackendType) -> (String, Vec<Value>) {
        let compiler = SqlCompiler::new(backend);
        let node = WhereNode::from_q(&self.condition);
        let mut sql = String::new();
        let mut params = Vec::new();
        compiler.compile_where_node_pub(&node, &mut sql, &mut params);
        (sql, params)
    }
}

impl Constraint for CheckConstraint {
    fn name(&self) -> &str {
        &self.name
    }

    fn to_sql(&self, _table: &str) -> String {
        let compiler = SqlCompiler::new(DatabaseBackendType::PostgreSQL);
        let node = WhereNode::from_q(&self.condition);
        let mut cond_sql = String::new();
        let mut params = Vec::new();
        compiler.compile_where_node_pub(&node, &mut cond_sql, &mut params);

        // For DDL, inline parameter values (constraints don't use placeholders).
        let final_cond = inline_params(&cond_sql, &params);

        format!("\"{}\" CHECK ({final_cond})", self.name)
    }
}

/// A UNIQUE constraint that enforces uniqueness across one or more columns.
///
/// This is the equivalent of Django's `UniqueConstraint`. It supports optional
/// conditions for partial unique indexes (PostgreSQL `WHERE` clause on index).
///
/// # Examples
///
/// ```
/// use django_rs_db::constraints::{UniqueConstraint, Constraint};
///
/// // Simple unique constraint
/// let constraint = UniqueConstraint::new(
///     "unique_email",
///     vec!["email".to_string()],
/// );
/// let sql = constraint.to_sql("users");
/// assert!(sql.contains("UNIQUE"));
///
/// // Multi-column unique constraint
/// let constraint = UniqueConstraint::new(
///     "unique_user_project",
///     vec!["user_id".to_string(), "project_id".to_string()],
/// );
/// ```
#[derive(Debug, Clone)]
pub struct UniqueConstraint {
    /// The constraint name.
    name: String,
    /// The columns that must be unique together.
    fields: Vec<String>,
    /// Optional condition for a partial unique index.
    condition: Option<Q>,
    /// Whether nulls should be considered distinct (default true in SQL standard).
    nulls_distinct: Option<bool>,
}

impl UniqueConstraint {
    /// Creates a new unique constraint on the given fields.
    pub fn new(name: impl Into<String>, fields: Vec<String>) -> Self {
        Self {
            name: name.into(),
            fields,
            condition: None,
            nulls_distinct: None,
        }
    }

    /// Adds a condition for a partial unique index.
    ///
    /// This is only supported on PostgreSQL and SQLite (via partial indexes).
    /// The condition restricts which rows are considered for the uniqueness check.
    pub fn condition(mut self, q: Q) -> Self {
        self.condition = Some(q);
        self
    }

    /// Sets whether NULL values are considered distinct.
    ///
    /// When `true` (default SQL behavior), multiple NULL values are allowed.
    /// When `false`, at most one NULL is permitted. PostgreSQL 15+ supports
    /// `NULLS NOT DISTINCT`.
    pub fn nulls_distinct(mut self, distinct: bool) -> Self {
        self.nulls_distinct = Some(distinct);
        self
    }

    /// Returns the fields in this constraint.
    pub fn fields(&self) -> &[String] {
        &self.fields
    }

    /// Returns the optional condition.
    pub fn get_condition(&self) -> Option<&Q> {
        self.condition.as_ref()
    }

    /// Compiles the condition (if any) to SQL for the given backend.
    pub fn condition_sql(&self, backend: DatabaseBackendType) -> Option<(String, Vec<Value>)> {
        self.condition.as_ref().map(|q| {
            let compiler = SqlCompiler::new(backend);
            let node = WhereNode::from_q(q);
            let mut sql = String::new();
            let mut params = Vec::new();
            compiler.compile_where_node_pub(&node, &mut sql, &mut params);
            (sql, params)
        })
    }
}

impl Constraint for UniqueConstraint {
    fn name(&self) -> &str {
        &self.name
    }

    fn to_sql(&self, _table: &str) -> String {
        let cols: Vec<String> = self.fields.iter().map(|f| format!("\"{f}\"")).collect();
        let mut sql = format!("\"{}\" UNIQUE ({})", self.name, cols.join(", "));

        if self.nulls_distinct == Some(false) {
            sql.push_str(" NULLS NOT DISTINCT");
        }

        if let Some(ref q) = self.condition {
            let compiler = SqlCompiler::new(DatabaseBackendType::PostgreSQL);
            let node = WhereNode::from_q(q);
            let mut cond_sql = String::new();
            let mut params = Vec::new();
            compiler.compile_where_node_pub(&node, &mut cond_sql, &mut params);
            let final_cond = inline_params(&cond_sql, &params);
            sql.push_str(&format!(" WHERE {final_cond}"));
        }

        sql
    }
}

/// A PostgreSQL EXCLUDE constraint that prevents overlapping values.
///
/// Exclusion constraints are a generalization of unique constraints that allow
/// specifying an operator for each column pair. They are commonly used with
/// range types to prevent overlapping time ranges.
///
/// This is the equivalent of Django's `ExclusionConstraint` (PostgreSQL only).
///
/// # Examples
///
/// ```
/// use django_rs_db::constraints::{ExclusionConstraint, Constraint};
///
/// // Prevent overlapping reservations for the same room
/// let constraint = ExclusionConstraint::new(
///     "no_overlapping_reservations",
///     vec![
///         ("room_id".to_string(), "=".to_string()),
///         ("during".to_string(), "&&".to_string()),
///     ],
/// );
/// let sql = constraint.to_sql("reservations");
/// assert!(sql.contains("EXCLUDE"));
/// ```
#[derive(Debug, Clone)]
pub struct ExclusionConstraint {
    /// The constraint name.
    name: String,
    /// Pairs of (column, operator) that define the exclusion condition.
    expressions: Vec<(String, String)>,
    /// The index type to use (default: GiST).
    index_type: String,
    /// Optional WHERE condition for a partial exclusion constraint.
    condition: Option<Q>,
}

impl ExclusionConstraint {
    /// Creates a new exclusion constraint.
    ///
    /// Each expression is a `(column_name, operator)` pair. Common operators:
    /// - `"="` for equality
    /// - `"&&"` for range overlap
    /// - `"<>"` for inequality
    pub fn new(name: impl Into<String>, expressions: Vec<(String, String)>) -> Self {
        Self {
            name: name.into(),
            expressions,
            index_type: "gist".to_string(),
            condition: None,
        }
    }

    /// Sets the index type for the exclusion constraint (default: "gist").
    pub fn using(mut self, index_type: impl Into<String>) -> Self {
        self.index_type = index_type.into();
        self
    }

    /// Adds a condition for a partial exclusion constraint.
    pub fn condition(mut self, q: Q) -> Self {
        self.condition = Some(q);
        self
    }

    /// Returns the expressions.
    pub fn expressions(&self) -> &[(String, String)] {
        &self.expressions
    }

    /// Returns the index type.
    pub fn get_index_type(&self) -> &str {
        &self.index_type
    }

    /// Returns the optional condition.
    pub fn get_condition(&self) -> Option<&Q> {
        self.condition.as_ref()
    }
}

impl Constraint for ExclusionConstraint {
    fn name(&self) -> &str {
        &self.name
    }

    fn to_sql(&self, _table: &str) -> String {
        let expr_parts: Vec<String> = self
            .expressions
            .iter()
            .map(|(col, op)| format!("\"{}\" WITH {}", col, op))
            .collect();
        let mut sql = format!(
            "\"{}\" EXCLUDE USING {} ({})",
            self.name,
            self.index_type,
            expr_parts.join(", ")
        );

        if let Some(ref q) = self.condition {
            let compiler = SqlCompiler::new(DatabaseBackendType::PostgreSQL);
            let node = WhereNode::from_q(q);
            let mut cond_sql = String::new();
            let mut params = Vec::new();
            compiler.compile_where_node_pub(&node, &mut cond_sql, &mut params);
            let final_cond = inline_params(&cond_sql, &params);
            sql.push_str(&format!(" WHERE {final_cond}"));
        }

        sql
    }
}

/// Replaces parameter placeholders ($1, $2, ...) with inline values for DDL.
///
/// Constraints are defined in DDL (CREATE TABLE / ALTER TABLE) where
/// parameterized queries are not used. This function substitutes the
/// placeholders with properly quoted literal values.
fn inline_params(sql: &str, params: &[Value]) -> String {
    let mut result = sql.to_string();
    // Replace in reverse order so $10 doesn't match $1 first
    for (i, val) in params.iter().enumerate().rev() {
        let placeholder = format!("${}", i + 1);
        let literal = value_to_sql_literal(val);
        result = result.replace(&placeholder, &literal);
    }
    result
}

/// Converts a Value to an inline SQL literal for use in DDL.
fn value_to_sql_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Date(d) => format!("'{d}'"),
        Value::DateTime(dt) => format!("'{dt}'"),
        Value::DateTimeTz(dt) => format!("'{dt}'"),
        Value::Time(t) => format!("'{t}'"),
        _ => format!("'{value}'"),
    }
}

/// A boxed constraint for use in collections.
pub type BoxedConstraint = Box<dyn Constraint>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::lookups::Lookup;

    // ── CheckConstraint tests ───────────────────────────────────────────

    #[test]
    fn test_check_constraint_creation() {
        let constraint = CheckConstraint::new(
            "age_positive",
            Q::filter("age", Lookup::Gte(Value::from(0))),
        );
        assert_eq!(constraint.name(), "age_positive");
    }

    #[test]
    fn test_check_constraint_to_sql_gte() {
        let constraint = CheckConstraint::new(
            "age_non_negative",
            Q::filter("age", Lookup::Gte(Value::from(0))),
        );
        let sql = constraint.to_sql("users");
        assert_eq!(sql, "\"age_non_negative\" CHECK (\"age\" >= 0)");
    }

    #[test]
    fn test_check_constraint_to_sql_gt() {
        let constraint = CheckConstraint::new(
            "price_positive",
            Q::filter("price", Lookup::Gt(Value::from(0))),
        );
        let sql = constraint.to_sql("products");
        assert!(sql.contains("CHECK"));
        assert!(sql.contains("\"price\" > 0"));
    }

    #[test]
    fn test_check_constraint_to_sql_range() {
        let constraint = CheckConstraint::new(
            "valid_rating",
            Q::filter("rating", Lookup::Range(Value::from(1), Value::from(5))),
        );
        let sql = constraint.to_sql("reviews");
        assert!(sql.contains("CHECK"));
        assert!(sql.contains("BETWEEN 1 AND 5"));
    }

    #[test]
    fn test_check_constraint_to_sql_and() {
        let constraint = CheckConstraint::new(
            "valid_dates",
            Q::filter("start_date", Lookup::IsNull(false))
                & Q::filter("end_date", Lookup::IsNull(false)),
        );
        let sql = constraint.to_sql("events");
        assert!(sql.contains("CHECK"));
        assert!(sql.contains("IS NOT NULL"));
        assert!(sql.contains("AND"));
    }

    #[test]
    fn test_check_constraint_to_sql_or() {
        let constraint = CheckConstraint::new(
            "has_contact",
            Q::filter("email", Lookup::IsNull(false)) | Q::filter("phone", Lookup::IsNull(false)),
        );
        let sql = constraint.to_sql("contacts");
        assert!(sql.contains("CHECK"));
        assert!(sql.contains("OR"));
    }

    #[test]
    fn test_check_constraint_to_sql_string_value() {
        let constraint = CheckConstraint::new(
            "status_valid",
            Q::filter(
                "status",
                Lookup::In(vec![
                    Value::from("active"),
                    Value::from("inactive"),
                    Value::from("pending"),
                ]),
            ),
        );
        let sql = constraint.to_sql("users");
        assert!(sql.contains("CHECK"));
        assert!(sql.contains("IN"));
        assert!(sql.contains("'active'"));
        assert!(sql.contains("'inactive'"));
        assert!(sql.contains("'pending'"));
    }

    #[test]
    fn test_check_constraint_create_sql() {
        let constraint = CheckConstraint::new(
            "age_positive",
            Q::filter("age", Lookup::Gte(Value::from(0))),
        );
        let sql = constraint.create_sql("users");
        assert!(sql.starts_with("ALTER TABLE \"users\" ADD CONSTRAINT"));
        assert!(sql.contains("CHECK"));
    }

    #[test]
    fn test_check_constraint_drop_sql() {
        let constraint = CheckConstraint::new(
            "age_positive",
            Q::filter("age", Lookup::Gte(Value::from(0))),
        );
        let sql = constraint.drop_sql("users");
        assert_eq!(
            sql,
            "ALTER TABLE \"users\" DROP CONSTRAINT \"age_positive\""
        );
    }

    #[test]
    fn test_check_constraint_condition_sql() {
        let constraint = CheckConstraint::new("test", Q::filter("x", Lookup::Gt(Value::from(10))));
        let (sql, params) = constraint.condition_sql(DatabaseBackendType::PostgreSQL);
        assert_eq!(sql, "\"x\" > $1");
        assert_eq!(params, vec![Value::Int(10)]);
    }

    #[test]
    fn test_check_constraint_condition_sql_sqlite() {
        let constraint = CheckConstraint::new("test", Q::filter("x", Lookup::Gt(Value::from(10))));
        let (sql, params) = constraint.condition_sql(DatabaseBackendType::SQLite);
        assert_eq!(sql, "\"x\" > ?");
        assert_eq!(params, vec![Value::Int(10)]);
    }

    #[test]
    fn test_check_constraint_not() {
        let constraint = CheckConstraint::new(
            "not_zero",
            !Q::filter("amount", Lookup::Exact(Value::from(0))),
        );
        let sql = constraint.to_sql("transactions");
        assert!(sql.contains("CHECK"));
        assert!(sql.contains("NOT"));
    }

    #[test]
    fn test_check_constraint_bool_value() {
        let constraint = CheckConstraint::new(
            "must_be_active",
            Q::filter("is_active", Lookup::Exact(Value::from(true))),
        );
        let sql = constraint.to_sql("users");
        assert!(sql.contains("CHECK"));
        assert!(sql.contains("TRUE"));
    }

    // ── UniqueConstraint tests ──────────────────────────────────────────

    #[test]
    fn test_unique_constraint_creation() {
        let constraint = UniqueConstraint::new("unique_email", vec!["email".to_string()]);
        assert_eq!(constraint.name(), "unique_email");
        assert_eq!(constraint.fields(), &["email"]);
    }

    #[test]
    fn test_unique_constraint_to_sql_single_column() {
        let constraint = UniqueConstraint::new("unique_email", vec!["email".to_string()]);
        let sql = constraint.to_sql("users");
        assert_eq!(sql, "\"unique_email\" UNIQUE (\"email\")");
    }

    #[test]
    fn test_unique_constraint_to_sql_multi_column() {
        let constraint = UniqueConstraint::new(
            "unique_user_project",
            vec!["user_id".to_string(), "project_id".to_string()],
        );
        let sql = constraint.to_sql("memberships");
        assert_eq!(
            sql,
            "\"unique_user_project\" UNIQUE (\"user_id\", \"project_id\")"
        );
    }

    #[test]
    fn test_unique_constraint_with_condition() {
        let constraint = UniqueConstraint::new("unique_active_email", vec!["email".to_string()])
            .condition(Q::filter("is_active", Lookup::Exact(Value::from(true))));
        let sql = constraint.to_sql("users");
        assert!(sql.contains("UNIQUE (\"email\")"));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("\"is_active\" = TRUE"));
    }

    #[test]
    fn test_unique_constraint_nulls_not_distinct() {
        let constraint =
            UniqueConstraint::new("unique_code", vec!["code".to_string()]).nulls_distinct(false);
        let sql = constraint.to_sql("items");
        assert!(sql.contains("NULLS NOT DISTINCT"));
    }

    #[test]
    fn test_unique_constraint_nulls_distinct_default() {
        let constraint = UniqueConstraint::new("unique_code", vec!["code".to_string()]);
        let sql = constraint.to_sql("items");
        // Default behavior: no NULLS clause
        assert!(!sql.contains("NULLS"));
    }

    #[test]
    fn test_unique_constraint_nulls_distinct_true() {
        let constraint =
            UniqueConstraint::new("unique_code", vec!["code".to_string()]).nulls_distinct(true);
        let sql = constraint.to_sql("items");
        // When explicitly true, it's the SQL default -- no extra clause needed
        assert!(!sql.contains("NULLS NOT DISTINCT"));
    }

    #[test]
    fn test_unique_constraint_create_sql() {
        let constraint = UniqueConstraint::new("unique_email", vec!["email".to_string()]);
        let sql = constraint.create_sql("users");
        assert!(sql.starts_with("ALTER TABLE \"users\" ADD CONSTRAINT"));
        assert!(sql.contains("UNIQUE"));
    }

    #[test]
    fn test_unique_constraint_drop_sql() {
        let constraint = UniqueConstraint::new("unique_email", vec!["email".to_string()]);
        let sql = constraint.drop_sql("users");
        assert_eq!(
            sql,
            "ALTER TABLE \"users\" DROP CONSTRAINT \"unique_email\""
        );
    }

    #[test]
    fn test_unique_constraint_condition_sql() {
        let constraint = UniqueConstraint::new("test", vec!["col".to_string()])
            .condition(Q::filter("active", Lookup::Exact(Value::from(true))));
        let result = constraint.condition_sql(DatabaseBackendType::PostgreSQL);
        assert!(result.is_some());
        let (sql, params) = result.unwrap();
        assert_eq!(sql, "\"active\" = $1");
        assert_eq!(params, vec![Value::Bool(true)]);
    }

    #[test]
    fn test_unique_constraint_no_condition_sql() {
        let constraint = UniqueConstraint::new("test", vec!["col".to_string()]);
        assert!(constraint
            .condition_sql(DatabaseBackendType::PostgreSQL)
            .is_none());
    }

    #[test]
    fn test_unique_constraint_with_complex_condition() {
        let constraint =
            UniqueConstraint::new("unique_active_verified", vec!["username".to_string()])
                .condition(
                    Q::filter("is_active", Lookup::Exact(Value::from(true)))
                        & Q::filter("is_verified", Lookup::Exact(Value::from(true))),
                );
        let sql = constraint.to_sql("users");
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("AND"));
    }

    // ── Inline parameter tests ──────────────────────────────────────────

    #[test]
    fn test_inline_params_int() {
        let sql = inline_params("\"x\" > $1", &[Value::Int(10)]);
        assert_eq!(sql, "\"x\" > 10");
    }

    #[test]
    fn test_inline_params_string() {
        let sql = inline_params("\"name\" = $1", &[Value::from("Alice")]);
        assert_eq!(sql, "\"name\" = 'Alice'");
    }

    #[test]
    fn test_inline_params_string_with_quote() {
        let sql = inline_params("\"name\" = $1", &[Value::from("O'Brien")]);
        assert_eq!(sql, "\"name\" = 'O''Brien'");
    }

    #[test]
    fn test_inline_params_multiple() {
        let sql = inline_params("\"x\" BETWEEN $1 AND $2", &[Value::Int(1), Value::Int(10)]);
        assert_eq!(sql, "\"x\" BETWEEN 1 AND 10");
    }

    #[test]
    fn test_inline_params_null() {
        let sql = inline_params("\"x\" = $1", &[Value::Null]);
        assert_eq!(sql, "\"x\" = NULL");
    }

    #[test]
    fn test_inline_params_bool() {
        let sql = inline_params("\"x\" = $1", &[Value::Bool(true)]);
        assert_eq!(sql, "\"x\" = TRUE");
    }

    #[test]
    fn test_inline_params_float() {
        let sql = inline_params("\"x\" > $1", &[Value::Float(3.14)]);
        assert_eq!(sql, "\"x\" > 3.14");
    }

    #[test]
    fn test_value_to_sql_literal_variants() {
        assert_eq!(value_to_sql_literal(&Value::Null), "NULL");
        assert_eq!(value_to_sql_literal(&Value::Bool(true)), "TRUE");
        assert_eq!(value_to_sql_literal(&Value::Bool(false)), "FALSE");
        assert_eq!(value_to_sql_literal(&Value::Int(42)), "42");
        assert_eq!(value_to_sql_literal(&Value::Float(1.5)), "1.5");
        assert_eq!(value_to_sql_literal(&Value::from("hello")), "'hello'");
    }

    // ── Constraint trait object tests ───────────────────────────────────

    #[test]
    fn test_constraint_as_trait_object() {
        let constraints: Vec<BoxedConstraint> = vec![
            Box::new(CheckConstraint::new(
                "price_positive",
                Q::filter("price", Lookup::Gt(Value::from(0))),
            )),
            Box::new(UniqueConstraint::new(
                "unique_email",
                vec!["email".to_string()],
            )),
        ];

        assert_eq!(constraints.len(), 2);
        assert_eq!(constraints[0].name(), "price_positive");
        assert_eq!(constraints[1].name(), "unique_email");

        let check_sql = constraints[0].to_sql("products");
        assert!(check_sql.contains("CHECK"));

        let unique_sql = constraints[1].to_sql("users");
        assert!(unique_sql.contains("UNIQUE"));
    }

    #[test]
    fn test_three_column_unique() {
        let constraint = UniqueConstraint::new(
            "unique_combo",
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        );
        let sql = constraint.to_sql("t");
        assert_eq!(sql, "\"unique_combo\" UNIQUE (\"a\", \"b\", \"c\")");
    }

    #[test]
    fn test_check_constraint_condition_accessor() {
        let q = Q::filter("age", Lookup::Gte(Value::from(0)));
        let constraint = CheckConstraint::new("test", q.clone());
        assert_eq!(*constraint.condition(), q);
    }

    #[test]
    fn test_unique_constraint_get_condition_none() {
        let constraint = UniqueConstraint::new("test", vec!["col".to_string()]);
        assert!(constraint.get_condition().is_none());
    }

    #[test]
    fn test_unique_constraint_get_condition_some() {
        let q = Q::filter("active", Lookup::Exact(Value::from(true)));
        let constraint = UniqueConstraint::new("test", vec!["col".to_string()]).condition(q);
        assert!(constraint.get_condition().is_some());
    }

    // ── ExclusionConstraint tests ──────────────────────────────────────

    #[test]
    fn test_exclusion_constraint_creation() {
        let constraint = ExclusionConstraint::new(
            "no_overlap",
            vec![
                ("room_id".to_string(), "=".to_string()),
                ("during".to_string(), "&&".to_string()),
            ],
        );
        assert_eq!(constraint.name(), "no_overlap");
        assert_eq!(constraint.expressions().len(), 2);
    }

    #[test]
    fn test_exclusion_constraint_to_sql() {
        let constraint = ExclusionConstraint::new(
            "no_overlap",
            vec![
                ("room_id".to_string(), "=".to_string()),
                ("during".to_string(), "&&".to_string()),
            ],
        );
        let sql = constraint.to_sql("reservations");
        assert_eq!(
            sql,
            "\"no_overlap\" EXCLUDE USING gist (\"room_id\" WITH =, \"during\" WITH &&)"
        );
    }

    #[test]
    fn test_exclusion_constraint_custom_index_type() {
        let constraint = ExclusionConstraint::new(
            "no_overlap",
            vec![("range_col".to_string(), "&&".to_string())],
        )
        .using("spgist");
        let sql = constraint.to_sql("t");
        assert!(sql.contains("USING spgist"));
    }

    #[test]
    fn test_exclusion_constraint_with_condition() {
        let constraint = ExclusionConstraint::new(
            "no_overlap_active",
            vec![
                ("room_id".to_string(), "=".to_string()),
                ("during".to_string(), "&&".to_string()),
            ],
        )
        .condition(Q::filter("is_active", Lookup::Exact(Value::from(true))));
        let sql = constraint.to_sql("reservations");
        assert!(sql.contains("EXCLUDE"));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("\"is_active\" = TRUE"));
    }

    #[test]
    fn test_exclusion_constraint_create_sql() {
        let constraint =
            ExclusionConstraint::new("no_overlap", vec![("during".to_string(), "&&".to_string())]);
        let sql = constraint.create_sql("reservations");
        assert!(sql.starts_with("ALTER TABLE \"reservations\" ADD CONSTRAINT"));
        assert!(sql.contains("EXCLUDE"));
    }

    #[test]
    fn test_exclusion_constraint_drop_sql() {
        let constraint =
            ExclusionConstraint::new("no_overlap", vec![("during".to_string(), "&&".to_string())]);
        let sql = constraint.drop_sql("reservations");
        assert_eq!(
            sql,
            "ALTER TABLE \"reservations\" DROP CONSTRAINT \"no_overlap\""
        );
    }

    #[test]
    fn test_exclusion_constraint_default_index_type() {
        let constraint =
            ExclusionConstraint::new("test", vec![("col".to_string(), "=".to_string())]);
        assert_eq!(constraint.get_index_type(), "gist");
    }

    #[test]
    fn test_exclusion_constraint_get_condition() {
        let constraint =
            ExclusionConstraint::new("test", vec![("col".to_string(), "=".to_string())]);
        assert!(constraint.get_condition().is_none());

        let constraint_with_cond =
            ExclusionConstraint::new("test", vec![("col".to_string(), "=".to_string())])
                .condition(Q::filter("x", Lookup::Gt(Value::from(0))));
        assert!(constraint_with_cond.get_condition().is_some());
    }

    #[test]
    fn test_exclusion_constraint_as_trait_object() {
        let constraints: Vec<BoxedConstraint> = vec![Box::new(ExclusionConstraint::new(
            "no_overlap",
            vec![("during".to_string(), "&&".to_string())],
        ))];
        assert_eq!(constraints[0].name(), "no_overlap");
        let sql = constraints[0].to_sql("t");
        assert!(sql.contains("EXCLUDE"));
    }
}
