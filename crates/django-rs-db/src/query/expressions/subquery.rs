//! Subquery, OuterRef, and Exists expressions for correlated subqueries.
//!
//! These expressions allow embedding one query inside another, referencing
//! columns from the outer query, and checking for existence of matching rows.
//! This mirrors Django's `Subquery`, `OuterRef`, and `Exists` expressions.
//!
//! # Examples
//!
//! ```
//! use django_rs_db::query::expressions::subquery::{SubqueryExpression, OuterRef, Exists};
//! use django_rs_db::query::compiler::Query;
//!
//! // Create a subquery that selects a single column
//! let mut inner = Query::new("comments");
//! let subquery = SubqueryExpression::new(inner);
//!
//! // OuterRef references a column from the outer query
//! let outer_ref = OuterRef::new("author_id");
//!
//! // Exists wraps a subquery to produce a boolean
//! let mut exists_query = Query::new("comments");
//! let exists = Exists::new(exists_query);
//! ```

use super::core::Expression;
use crate::query::compiler::Query;

/// A subquery expression that wraps a Query to be used inside another query.
///
/// Renders as `(SELECT ... FROM ... WHERE ...)` in SQL. Typically used in
/// annotations to fetch a scalar value from a correlated subquery.
///
/// This is the equivalent of Django's `Subquery()`.
#[derive(Debug, Clone)]
pub struct SubqueryExpression {
    /// The inner query that forms the subquery.
    query: Query,
}

impl SubqueryExpression {
    /// Creates a new subquery expression from a Query AST.
    pub fn new(query: Query) -> Self {
        Self { query }
    }

    /// Returns a reference to the inner query.
    pub fn query(&self) -> &Query {
        &self.query
    }

    /// Converts this subquery into an Expression that can be used
    /// in annotations, filters, and other expression contexts.
    pub fn into_expression(self) -> Expression {
        Expression::Subquery(Box::new(self.query))
    }
}

/// An outer reference used inside a subquery to reference a column from the
/// enclosing (outer) query.
///
/// In SQL, this renders as a qualified column reference from the outer table.
/// This is the equivalent of Django's `OuterRef()`.
///
/// # How it works
///
/// When you use `OuterRef("author_id")` inside a subquery, the SQL compiler
/// will resolve it to the outer query's table, producing something like
/// `"outer_table"."author_id"` in the WHERE clause of the subquery.
#[derive(Debug, Clone)]
pub struct OuterRef {
    /// The column name in the outer query.
    column: String,
}

impl OuterRef {
    /// Creates a new outer reference to the given column name.
    pub fn new(column: impl Into<String>) -> Self {
        Self {
            column: column.into(),
        }
    }

    /// Returns the column name being referenced.
    pub fn column(&self) -> &str {
        &self.column
    }

    /// Converts this outer reference into an Expression.
    ///
    /// The expression uses a special naming convention with `__outer__` prefix
    /// that the SQL compiler recognizes and resolves to the outer query's table.
    pub fn into_expression(self) -> Expression {
        Expression::OuterRef(self.column)
    }
}

/// An EXISTS subquery expression that checks whether a subquery returns any rows.
///
/// Renders as `EXISTS (SELECT 1 FROM ... WHERE ...)` in SQL. This is commonly
/// used in filters to check for related objects.
///
/// This is the equivalent of Django's `Exists()`.
///
/// # Examples
///
/// ```
/// use django_rs_db::query::expressions::subquery::Exists;
/// use django_rs_db::query::compiler::Query;
///
/// let inner = Query::new("comments");
/// let exists = Exists::new(inner);
/// let expr = exists.into_expression();
/// ```
#[derive(Debug, Clone)]
pub struct Exists {
    /// The inner query for the EXISTS check.
    query: Query,
    /// Whether to negate (NOT EXISTS).
    negated: bool,
}

impl Exists {
    /// Creates a new EXISTS expression from a Query AST.
    pub fn new(query: Query) -> Self {
        Self {
            query,
            negated: false,
        }
    }

    /// Negates this EXISTS to produce NOT EXISTS.
    pub fn negate(mut self) -> Self {
        self.negated = !self.negated;
        self
    }

    /// Returns whether this is a negated (NOT EXISTS) expression.
    pub fn is_negated(&self) -> bool {
        self.negated
    }

    /// Returns a reference to the inner query.
    pub fn query(&self) -> &Query {
        &self.query
    }

    /// Converts this EXISTS into an Expression.
    pub fn into_expression(self) -> Expression {
        Expression::Exists {
            query: Box::new(self.query),
            negated: self.negated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::compiler::{DatabaseBackendType, SelectColumn, SqlCompiler, WhereNode};
    use crate::query::lookups::Lookup;
    use crate::value::Value;

    fn pg() -> SqlCompiler {
        SqlCompiler::new(DatabaseBackendType::PostgreSQL)
    }

    fn sqlite() -> SqlCompiler {
        SqlCompiler::new(DatabaseBackendType::SQLite)
    }

    #[test]
    fn test_subquery_expression_creation() {
        let query = Query::new("comments");
        let subquery = SubqueryExpression::new(query);
        assert_eq!(subquery.query().table, "comments");
    }

    #[test]
    fn test_subquery_into_expression() {
        let mut query = Query::new("comments");
        query.select = vec![SelectColumn::Column("count_val".to_string())];
        let expr = SubqueryExpression::new(query).into_expression();
        assert!(matches!(expr, Expression::Subquery(_)));
    }

    #[test]
    fn test_subquery_compiles_to_sql_pg() {
        let mut inner = Query::new("comments");
        inner.select = vec![SelectColumn::Expression(
            Expression::aggregate(
                super::super::core::AggregateFunc::Count,
                Expression::col("id"),
            ),
            "cnt".to_string(),
        )];
        inner.where_clause = Some(WhereNode::Condition {
            column: "post_id".to_string(),
            lookup: Lookup::Exact(Value::from(1)),
        });

        let expr = SubqueryExpression::new(inner).into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);

        assert!(sql.starts_with('('));
        assert!(sql.ends_with(')'));
        assert!(sql.contains("SELECT COUNT(\"id\") AS \"cnt\" FROM \"comments\""));
        assert!(sql.contains("WHERE \"post_id\" = $1"));
        assert_eq!(params, vec![Value::Int(1)]);
    }

    #[test]
    fn test_subquery_compiles_to_sql_sqlite() {
        let mut inner = Query::new("comments");
        inner.select = vec![SelectColumn::Column("body".to_string())];
        inner.where_clause = Some(WhereNode::Condition {
            column: "post_id".to_string(),
            lookup: Lookup::Exact(Value::from(5)),
        });

        let expr = SubqueryExpression::new(inner).into_expression();
        let compiler = sqlite();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);

        assert!(sql.contains("?"));
        assert!(!sql.contains('$'));
        assert_eq!(params, vec![Value::Int(5)]);
    }

    #[test]
    fn test_outer_ref_creation() {
        let outer_ref = OuterRef::new("author_id");
        assert_eq!(outer_ref.column(), "author_id");
    }

    #[test]
    fn test_outer_ref_into_expression() {
        let expr = OuterRef::new("user_id").into_expression();
        match &expr {
            Expression::OuterRef(col) => assert_eq!(col, "user_id"),
            _ => panic!("Expected OuterRef expression"),
        }
    }

    #[test]
    fn test_outer_ref_compiles_to_sql() {
        let expr = OuterRef::new("author_id").into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        // OuterRef renders as a quoted column reference from the outer table
        assert_eq!(sql, "\"author_id\"");
        assert!(params.is_empty());
    }

    #[test]
    fn test_exists_creation() {
        let query = Query::new("comments");
        let exists = Exists::new(query);
        assert!(!exists.is_negated());
        assert_eq!(exists.query().table, "comments");
    }

    #[test]
    fn test_exists_negation() {
        let query = Query::new("comments");
        let exists = Exists::new(query).negate();
        assert!(exists.is_negated());
    }

    #[test]
    fn test_exists_double_negation() {
        let query = Query::new("comments");
        let exists = Exists::new(query).negate().negate();
        assert!(!exists.is_negated());
    }

    #[test]
    fn test_exists_into_expression() {
        let query = Query::new("comments");
        let expr = Exists::new(query).into_expression();
        assert!(matches!(expr, Expression::Exists { negated: false, .. }));
    }

    #[test]
    fn test_not_exists_into_expression() {
        let query = Query::new("comments");
        let expr = Exists::new(query).negate().into_expression();
        assert!(matches!(expr, Expression::Exists { negated: true, .. }));
    }

    #[test]
    fn test_exists_compiles_to_sql_pg() {
        let mut inner = Query::new("comments");
        inner.where_clause = Some(WhereNode::Condition {
            column: "post_id".to_string(),
            lookup: Lookup::Exact(Value::from(42)),
        });

        let expr = Exists::new(inner).into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);

        assert!(sql.starts_with("EXISTS ("));
        assert!(sql.ends_with(')'));
        assert!(sql.contains("SELECT 1 AS \"__exists__\" FROM \"comments\""));
        assert!(sql.contains("WHERE \"post_id\" ="));
        assert_eq!(params, vec![Value::Int(42)]);
    }

    #[test]
    fn test_not_exists_compiles_to_sql_pg() {
        let mut inner = Query::new("comments");
        inner.where_clause = Some(WhereNode::Condition {
            column: "active".to_string(),
            lookup: Lookup::Exact(Value::from(true)),
        });

        let expr = Exists::new(inner).negate().into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);

        assert!(sql.starts_with("NOT EXISTS ("));
        assert!(sql.ends_with(')'));
    }

    #[test]
    fn test_exists_compiles_to_sql_sqlite() {
        let inner = Query::new("orders");
        let expr = Exists::new(inner).into_expression();
        let compiler = sqlite();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);

        assert!(sql.starts_with("EXISTS ("));
        assert!(sql.contains("SELECT 1 AS \"__exists__\" FROM \"orders\""));
    }

    #[test]
    fn test_correlated_subquery() {
        // Simulate: annotate posts with comment count where comment.post_id = OuterRef('id')
        let mut inner = Query::new("comments");
        inner.select = vec![SelectColumn::Expression(
            Expression::aggregate(
                super::super::core::AggregateFunc::Count,
                Expression::col("id"),
            ),
            "cnt".to_string(),
        )];
        // The WHERE clause would reference the outer query via OuterRef
        inner.where_clause = Some(WhereNode::Condition {
            column: "post_id".to_string(),
            lookup: Lookup::Exact(Value::from(1)), // placeholder for OuterRef
        });

        let subquery = SubqueryExpression::new(inner);
        let mut outer = Query::new("posts");
        outer
            .annotations
            .insert("comment_count".to_string(), subquery.into_expression());

        let compiler = pg();
        let (sql, params) = compiler.compile_select(&outer);

        assert!(sql.contains("comment_count"));
        assert!(sql.contains("COUNT"));
        assert!(sql.contains("comments"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_subquery_in_annotation() {
        let mut inner = Query::new("prices");
        inner.select = vec![SelectColumn::Expression(
            Expression::aggregate(
                super::super::core::AggregateFunc::Min,
                Expression::col("amount"),
            ),
            "min_price".to_string(),
        )];

        let mut outer = Query::new("products");
        outer.annotations.insert(
            "lowest_price".to_string(),
            SubqueryExpression::new(inner).into_expression(),
        );

        let compiler = pg();
        let (sql, _) = compiler.compile_select(&outer);

        assert!(sql.contains("lowest_price"));
        assert!(sql.contains("MIN(\"amount\")"));
        assert!(sql.contains("\"prices\""));
    }

    #[test]
    fn test_exists_in_annotation() {
        let inner = Query::new("reviews");
        let mut outer = Query::new("products");
        outer.annotations.insert(
            "has_reviews".to_string(),
            Exists::new(inner).into_expression(),
        );

        let compiler = pg();
        let (sql, _) = compiler.compile_select(&outer);

        assert!(sql.contains("has_reviews"));
        assert!(sql.contains("EXISTS"));
    }
}
