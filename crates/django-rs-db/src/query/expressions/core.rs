//! Query expressions, aggregates, and F-objects.
//!
//! This module provides the [`Expression`] enum for building computed values,
//! annotations, and aggregates in queries. It mirrors Django's
//! `django.db.models.expressions` module.
//!
//! # Examples
//!
//! ```
//! use django_rs_db::query::expressions::{Expression, AggregateFunc};
//! use django_rs_db::value::Value;
//!
//! // F("price") * 1.1
//! let expr = Expression::f("price") * Expression::value(1.1);
//!
//! // Count("id")
//! let count = Expression::aggregate(AggregateFunc::Count, Expression::col("id"));
//! ```

use crate::query::compiler::Query;
use crate::query::lookups::Q;
use crate::value::Value;
use std::ops;

use super::window::WindowExpression;

/// A query expression that produces a value in the context of a SQL query.
///
/// Expressions can reference columns, literal values, functions, aggregates,
/// subqueries, and arithmetic combinations. They are used in `annotate()`,
/// `aggregate()`, `filter()`, and `order_by()` clauses.
#[derive(Debug, Clone)]
pub enum Expression {
    /// A column reference (fully qualified or plain).
    Col(String),
    /// A literal value.
    Value(Value),
    /// An F-expression referencing another field.
    F(String),
    /// A database function call.
    Func {
        /// Function name (e.g., "COALESCE", "UPPER").
        name: String,
        /// Function arguments.
        args: Vec<Expression>,
    },
    /// An aggregate function.
    Aggregate {
        /// The aggregate operation.
        func: AggregateFunc,
        /// The expression being aggregated.
        field: Box<Expression>,
        /// Whether to apply DISTINCT.
        distinct: bool,
        /// Optional FILTER clause.
        filter: Option<Box<Q>>,
    },
    /// A CASE ... WHEN ... THEN ... ELSE ... END expression.
    Case {
        /// The WHEN/THEN branches.
        whens: Vec<When>,
        /// The ELSE value.
        default: Option<Box<Expression>>,
    },
    /// A subquery expression.
    Subquery(Box<Query>),
    /// An outer reference used inside a subquery to reference the enclosing query's column.
    OuterRef(String),
    /// An EXISTS (or NOT EXISTS) subquery expression.
    Exists {
        /// The inner query for the EXISTS check.
        query: Box<Query>,
        /// Whether this is NOT EXISTS.
        negated: bool,
    },
    /// A window expression: function OVER (PARTITION BY ... ORDER BY ... frame).
    Window(Box<WindowExpression>),
    /// EXTRACT(part FROM expr) - extracts a component from a date/time value.
    Extract {
        /// The part to extract (e.g., "YEAR", "MONTH", "DAY").
        part: String,
        /// The expression to extract from.
        expr: Box<Expression>,
    },
    /// DATE_TRUNC(precision, expr) - truncates a timestamp.
    DateTrunc {
        /// The precision to truncate to (e.g., "YEAR", "MONTH").
        precision: String,
        /// The expression to truncate.
        expr: Box<Expression>,
    },
    /// CAST(expr AS type) - type conversion.
    Cast {
        /// The expression to cast.
        expr: Box<Expression>,
        /// The target data type.
        data_type: String,
    },
    /// expr COLLATE collation - applies a collation.
    Collate {
        /// The expression.
        expr: Box<Expression>,
        /// The collation name.
        collation: String,
    },
    /// Raw SQL with parameters.
    RawSQL(String, Vec<Value>),
    /// Addition.
    Add(Box<Expression>, Box<Expression>),
    /// Subtraction.
    Sub(Box<Expression>, Box<Expression>),
    /// Multiplication.
    Mul(Box<Expression>, Box<Expression>),
    /// Division.
    Div(Box<Expression>, Box<Expression>),
}

/// Aggregate function types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateFunc {
    /// COUNT.
    Count,
    /// SUM.
    Sum,
    /// AVG.
    Avg,
    /// MIN.
    Min,
    /// MAX.
    Max,
    /// Standard deviation.
    StdDev,
    /// Variance.
    Variance,
}

impl AggregateFunc {
    /// Returns the SQL function name for this aggregate.
    pub const fn sql_name(&self) -> &'static str {
        match self {
            Self::Count => "COUNT",
            Self::Sum => "SUM",
            Self::Avg => "AVG",
            Self::Min => "MIN",
            Self::Max => "MAX",
            Self::StdDev => "STDDEV",
            Self::Variance => "VARIANCE",
        }
    }
}

/// A single WHEN/THEN branch in a CASE expression.
#[derive(Debug, Clone)]
pub struct When {
    /// The condition for this branch.
    pub condition: Q,
    /// The value to return when the condition is met.
    pub then: Expression,
}

impl Expression {
    /// Creates a column reference expression.
    pub fn col(name: impl Into<String>) -> Self {
        Self::Col(name.into())
    }

    /// Creates an F-expression referencing a field.
    pub fn f(name: impl Into<String>) -> Self {
        Self::F(name.into())
    }

    /// Creates a literal value expression.
    pub fn value(v: impl Into<Value>) -> Self {
        Self::Value(v.into())
    }

    /// Creates a function call expression.
    pub fn func(name: impl Into<String>, args: Vec<Expression>) -> Self {
        Self::Func {
            name: name.into(),
            args,
        }
    }

    /// Creates an aggregate expression.
    pub fn aggregate(func: AggregateFunc, field: Expression) -> Self {
        Self::Aggregate {
            func,
            field: Box::new(field),
            distinct: false,
            filter: None,
        }
    }

    /// Creates an aggregate with DISTINCT.
    pub fn aggregate_distinct(func: AggregateFunc, field: Expression) -> Self {
        Self::Aggregate {
            func,
            field: Box::new(field),
            distinct: true,
            filter: None,
        }
    }

    /// Creates a CASE expression.
    pub fn case(whens: Vec<When>, default: Option<Expression>) -> Self {
        Self::Case {
            whens,
            default: default.map(Box::new),
        }
    }

    /// Creates a raw SQL expression with parameters.
    pub fn raw(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self::RawSQL(sql.into(), params)
    }
}

impl ops::Add for Expression {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self::Add(Box::new(self), Box::new(rhs))
    }
}

impl ops::Sub for Expression {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::Sub(Box::new(self), Box::new(rhs))
    }
}

impl ops::Mul for Expression {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        Self::Mul(Box::new(self), Box::new(rhs))
    }
}

impl ops::Div for Expression {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        Self::Div(Box::new(self), Box::new(rhs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::lookups::Lookup;

    #[test]
    fn test_col_expression() {
        let expr = Expression::col("name");
        assert!(matches!(expr, Expression::Col(ref s) if s == "name"));
    }

    #[test]
    fn test_f_expression() {
        let expr = Expression::f("price");
        assert!(matches!(expr, Expression::F(ref s) if s == "price"));
    }

    #[test]
    fn test_value_expression() {
        let expr = Expression::value(42);
        assert!(matches!(expr, Expression::Value(Value::Int(42))));
    }

    #[test]
    fn test_func_expression() {
        let expr = Expression::func("UPPER", vec![Expression::col("name")]);
        if let Expression::Func { name, args } = &expr {
            assert_eq!(name, "UPPER");
            assert_eq!(args.len(), 1);
        } else {
            panic!("Expected Func");
        }
    }

    #[test]
    fn test_aggregate_expression() {
        let expr = Expression::aggregate(AggregateFunc::Count, Expression::col("id"));
        if let Expression::Aggregate {
            func,
            distinct,
            filter,
            ..
        } = &expr
        {
            assert_eq!(*func, AggregateFunc::Count);
            assert!(!distinct);
            assert!(filter.is_none());
        } else {
            panic!("Expected Aggregate");
        }
    }

    #[test]
    fn test_aggregate_distinct() {
        let expr = Expression::aggregate_distinct(AggregateFunc::Count, Expression::col("category"));
        if let Expression::Aggregate { distinct, .. } = &expr {
            assert!(distinct);
        } else {
            panic!("Expected Aggregate");
        }
    }

    #[test]
    fn test_case_expression() {
        let when = When {
            condition: Q::filter("status", Lookup::Exact(Value::from("active"))),
            then: Expression::value(1),
        };
        let expr = Expression::case(vec![when], Some(Expression::value(0)));
        if let Expression::Case { whens, default } = &expr {
            assert_eq!(whens.len(), 1);
            assert!(default.is_some());
        } else {
            panic!("Expected Case");
        }
    }

    #[test]
    fn test_raw_sql_expression() {
        let expr = Expression::raw("EXTRACT(year FROM ?)", vec![Value::from("2024-01-01")]);
        if let Expression::RawSQL(sql, params) = &expr {
            assert_eq!(sql, "EXTRACT(year FROM ?)");
            assert_eq!(params.len(), 1);
        } else {
            panic!("Expected RawSQL");
        }
    }

    #[test]
    fn test_add_operator() {
        let expr = Expression::f("price") + Expression::value(10);
        assert!(matches!(expr, Expression::Add(_, _)));
    }

    #[test]
    fn test_sub_operator() {
        let expr = Expression::f("price") - Expression::value(5);
        assert!(matches!(expr, Expression::Sub(_, _)));
    }

    #[test]
    fn test_mul_operator() {
        let expr = Expression::f("quantity") * Expression::f("price");
        assert!(matches!(expr, Expression::Mul(_, _)));
    }

    #[test]
    fn test_div_operator() {
        let expr = Expression::f("total") / Expression::value(2);
        assert!(matches!(expr, Expression::Div(_, _)));
    }

    #[test]
    fn test_aggregate_func_sql_names() {
        assert_eq!(AggregateFunc::Count.sql_name(), "COUNT");
        assert_eq!(AggregateFunc::Sum.sql_name(), "SUM");
        assert_eq!(AggregateFunc::Avg.sql_name(), "AVG");
        assert_eq!(AggregateFunc::Min.sql_name(), "MIN");
        assert_eq!(AggregateFunc::Max.sql_name(), "MAX");
        assert_eq!(AggregateFunc::StdDev.sql_name(), "STDDEV");
        assert_eq!(AggregateFunc::Variance.sql_name(), "VARIANCE");
    }

    #[test]
    fn test_chained_arithmetic() {
        // (price * quantity) - discount
        let expr =
            (Expression::f("price") * Expression::f("quantity")) - Expression::f("discount");
        assert!(matches!(expr, Expression::Sub(_, _)));
    }
}
