//! Window expressions and window functions.
//!
//! SQL window functions compute values across a set of rows related to the current
//! row. This module provides the [`WindowExpression`] type for building window
//! function calls with PARTITION BY, ORDER BY, and frame specifications.
//!
//! This mirrors Django's `Window`, `RowNumber`, `Rank`, etc.
//!
//! # Examples
//!
//! ```
//! use django_rs_db::query::expressions::window::*;
//! use django_rs_db::query::expressions::core::Expression;
//!
//! // ROW_NUMBER() OVER (PARTITION BY department ORDER BY salary DESC)
//! let window = WindowExpression::new(WindowFunction::RowNumber)
//!     .partition_by(vec!["department".to_string()])
//!     .order_by(vec![("salary".to_string(), true)]);
//! ```

use super::core::Expression;

/// A window function type.
///
/// These are the standard SQL window functions supported across major databases.
#[derive(Debug, Clone)]
pub enum WindowFunction {
    /// ROW_NUMBER() - assigns a unique sequential integer to each row.
    RowNumber,
    /// RANK() - assigns a rank with gaps for ties.
    Rank,
    /// DENSE_RANK() - assigns a rank without gaps for ties.
    DenseRank,
    /// LAG(expr, offset, default) - accesses a previous row's value.
    Lag {
        /// The expression to evaluate.
        expression: Box<Expression>,
        /// Number of rows back (default 1).
        offset: Option<i64>,
        /// Default value if the offset goes beyond the partition.
        default: Option<Box<Expression>>,
    },
    /// LEAD(expr, offset, default) - accesses a following row's value.
    Lead {
        /// The expression to evaluate.
        expression: Box<Expression>,
        /// Number of rows forward (default 1).
        offset: Option<i64>,
        /// Default value if the offset goes beyond the partition.
        default: Option<Box<Expression>>,
    },
    /// FIRST_VALUE(expr) - returns the first value in the window frame.
    FirstValue(Box<Expression>),
    /// LAST_VALUE(expr) - returns the last value in the window frame.
    LastValue(Box<Expression>),
    /// NTH_VALUE(expr, n) - returns the nth value in the window frame.
    NthValue(Box<Expression>, i64),
    /// NTILE(n) - distributes rows into n roughly equal groups.
    Ntile(i64),
    /// CUME_DIST() - cumulative distribution.
    CumeDist,
    /// PERCENT_RANK() - relative rank of the current row.
    PercentRank,
    /// A generic aggregate used as a window function (e.g., SUM, AVG, COUNT).
    Aggregate(Box<Expression>),
}

impl WindowFunction {
    /// Returns the SQL function name for this window function.
    pub fn sql_name(&self) -> &str {
        match self {
            Self::RowNumber => "ROW_NUMBER",
            Self::Rank => "RANK",
            Self::DenseRank => "DENSE_RANK",
            Self::Lag { .. } => "LAG",
            Self::Lead { .. } => "LEAD",
            Self::FirstValue(_) => "FIRST_VALUE",
            Self::LastValue(_) => "LAST_VALUE",
            Self::NthValue(_, _) => "NTH_VALUE",
            Self::Ntile(_) => "NTILE",
            Self::CumeDist => "CUME_DIST",
            Self::PercentRank => "PERCENT_RANK",
            Self::Aggregate(_) => "__aggregate__",
        }
    }
}

/// The type of window frame (ROWS vs RANGE).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowFrameType {
    /// ROWS BETWEEN - frame defined by physical row offsets.
    Rows,
    /// RANGE BETWEEN - frame defined by logical value ranges.
    Range,
    /// GROUPS BETWEEN - frame defined by peer groups.
    Groups,
}

impl WindowFrameType {
    /// Returns the SQL keyword.
    pub fn sql_keyword(&self) -> &str {
        match self {
            Self::Rows => "ROWS",
            Self::Range => "RANGE",
            Self::Groups => "GROUPS",
        }
    }
}

/// A window frame boundary specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowFrameBound {
    /// UNBOUNDED PRECEDING.
    UnboundedPreceding,
    /// N PRECEDING.
    Preceding(i64),
    /// CURRENT ROW.
    CurrentRow,
    /// N FOLLOWING.
    Following(i64),
    /// UNBOUNDED FOLLOWING.
    UnboundedFollowing,
}

impl WindowFrameBound {
    /// Returns the SQL representation of this bound.
    pub fn to_sql(&self) -> String {
        match self {
            Self::UnboundedPreceding => "UNBOUNDED PRECEDING".to_string(),
            Self::Preceding(n) => format!("{n} PRECEDING"),
            Self::CurrentRow => "CURRENT ROW".to_string(),
            Self::Following(n) => format!("{n} FOLLOWING"),
            Self::UnboundedFollowing => "UNBOUNDED FOLLOWING".to_string(),
        }
    }
}

/// A window frame specification: `ROWS/RANGE BETWEEN start AND end`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowFrame {
    /// The frame type (ROWS, RANGE, or GROUPS).
    pub frame_type: WindowFrameType,
    /// The start bound.
    pub start: WindowFrameBound,
    /// The end bound.
    pub end: WindowFrameBound,
}

impl WindowFrame {
    /// Creates a new window frame.
    pub fn new(
        frame_type: WindowFrameType,
        start: WindowFrameBound,
        end: WindowFrameBound,
    ) -> Self {
        Self {
            frame_type,
            start,
            end,
        }
    }

    /// Creates a ROWS frame from start to end.
    pub fn rows(start: WindowFrameBound, end: WindowFrameBound) -> Self {
        Self::new(WindowFrameType::Rows, start, end)
    }

    /// Creates a RANGE frame from start to end.
    pub fn range(start: WindowFrameBound, end: WindowFrameBound) -> Self {
        Self::new(WindowFrameType::Range, start, end)
    }

    /// Returns the SQL for this frame specification.
    pub fn to_sql(&self) -> String {
        format!(
            "{} BETWEEN {} AND {}",
            self.frame_type.sql_keyword(),
            self.start.to_sql(),
            self.end.to_sql()
        )
    }
}

/// A complete window expression: function call + OVER clause.
///
/// This combines a window function with optional PARTITION BY, ORDER BY,
/// and frame specifications.
#[derive(Debug, Clone)]
pub struct WindowExpression {
    /// The window function to apply.
    pub function: WindowFunction,
    /// Columns to partition by.
    pub partition_by: Vec<String>,
    /// Columns to order by within each partition. Tuple of (column, descending).
    pub order_by: Vec<(String, bool)>,
    /// Optional window frame specification.
    pub frame: Option<WindowFrame>,
}

impl WindowExpression {
    /// Creates a new window expression with the given function.
    pub fn new(function: WindowFunction) -> Self {
        Self {
            function,
            partition_by: Vec::new(),
            order_by: Vec::new(),
            frame: None,
        }
    }

    /// Sets the PARTITION BY columns.
    pub fn partition_by(mut self, columns: Vec<String>) -> Self {
        self.partition_by = columns;
        self
    }

    /// Sets the ORDER BY columns. Each tuple is (column_name, descending).
    pub fn order_by(mut self, columns: Vec<(String, bool)>) -> Self {
        self.order_by = columns;
        self
    }

    /// Sets the window frame.
    pub fn frame(mut self, frame: WindowFrame) -> Self {
        self.frame = Some(frame);
        self
    }

    /// Converts this window expression into an Expression.
    pub fn into_expression(self) -> Expression {
        Expression::Window(Box::new(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::compiler::{DatabaseBackendType, Query, SqlCompiler};
    use crate::query::expressions::core::AggregateFunc;

    fn pg() -> SqlCompiler {
        SqlCompiler::new(DatabaseBackendType::PostgreSQL)
    }

    fn sqlite() -> SqlCompiler {
        SqlCompiler::new(DatabaseBackendType::SQLite)
    }

    #[test]
    fn test_window_function_sql_names() {
        assert_eq!(WindowFunction::RowNumber.sql_name(), "ROW_NUMBER");
        assert_eq!(WindowFunction::Rank.sql_name(), "RANK");
        assert_eq!(WindowFunction::DenseRank.sql_name(), "DENSE_RANK");
        assert_eq!(WindowFunction::CumeDist.sql_name(), "CUME_DIST");
        assert_eq!(WindowFunction::PercentRank.sql_name(), "PERCENT_RANK");
        assert_eq!(WindowFunction::Ntile(4).sql_name(), "NTILE");
    }

    #[test]
    fn test_window_frame_bound_sql() {
        assert_eq!(
            WindowFrameBound::UnboundedPreceding.to_sql(),
            "UNBOUNDED PRECEDING"
        );
        assert_eq!(WindowFrameBound::Preceding(3).to_sql(), "3 PRECEDING");
        assert_eq!(WindowFrameBound::CurrentRow.to_sql(), "CURRENT ROW");
        assert_eq!(WindowFrameBound::Following(2).to_sql(), "2 FOLLOWING");
        assert_eq!(
            WindowFrameBound::UnboundedFollowing.to_sql(),
            "UNBOUNDED FOLLOWING"
        );
    }

    #[test]
    fn test_window_frame_rows_sql() {
        let frame = WindowFrame::rows(
            WindowFrameBound::UnboundedPreceding,
            WindowFrameBound::CurrentRow,
        );
        assert_eq!(
            frame.to_sql(),
            "ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW"
        );
    }

    #[test]
    fn test_window_frame_range_sql() {
        let frame = WindowFrame::range(
            WindowFrameBound::CurrentRow,
            WindowFrameBound::UnboundedFollowing,
        );
        assert_eq!(
            frame.to_sql(),
            "RANGE BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING"
        );
    }

    #[test]
    fn test_window_frame_n_preceding_following() {
        let frame = WindowFrame::rows(
            WindowFrameBound::Preceding(5),
            WindowFrameBound::Following(5),
        );
        assert_eq!(frame.to_sql(), "ROWS BETWEEN 5 PRECEDING AND 5 FOLLOWING");
    }

    #[test]
    fn test_window_frame_groups() {
        let frame = WindowFrame::new(
            WindowFrameType::Groups,
            WindowFrameBound::UnboundedPreceding,
            WindowFrameBound::CurrentRow,
        );
        assert_eq!(
            frame.to_sql(),
            "GROUPS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW"
        );
    }

    #[test]
    fn test_row_number_simple() {
        let window = WindowExpression::new(WindowFunction::RowNumber);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "ROW_NUMBER() OVER ()");
    }

    #[test]
    fn test_row_number_with_partition_and_order() {
        let window = WindowExpression::new(WindowFunction::RowNumber)
            .partition_by(vec!["department".to_string()])
            .order_by(vec![("salary".to_string(), true)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(
            sql,
            "ROW_NUMBER() OVER (PARTITION BY \"department\" ORDER BY \"salary\" DESC)"
        );
    }

    #[test]
    fn test_rank_with_order() {
        let window =
            WindowExpression::new(WindowFunction::Rank).order_by(vec![("score".to_string(), true)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "RANK() OVER (ORDER BY \"score\" DESC)");
    }

    #[test]
    fn test_dense_rank() {
        let window = WindowExpression::new(WindowFunction::DenseRank)
            .order_by(vec![("grade".to_string(), false)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "DENSE_RANK() OVER (ORDER BY \"grade\" ASC)");
    }

    #[test]
    fn test_lag_simple() {
        let window = WindowExpression::new(WindowFunction::Lag {
            expression: Box::new(Expression::col("price")),
            offset: None,
            default: None,
        })
        .order_by(vec![("date".to_string(), false)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "LAG(\"price\") OVER (ORDER BY \"date\" ASC)");
    }

    #[test]
    fn test_lag_with_offset_and_default() {
        let window = WindowExpression::new(WindowFunction::Lag {
            expression: Box::new(Expression::col("price")),
            offset: Some(2),
            default: Some(Box::new(Expression::value(0))),
        })
        .order_by(vec![("date".to_string(), false)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "LAG(\"price\", 2, $1) OVER (ORDER BY \"date\" ASC)");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_lead_simple() {
        let window = WindowExpression::new(WindowFunction::Lead {
            expression: Box::new(Expression::col("value")),
            offset: None,
            default: None,
        })
        .order_by(vec![("ts".to_string(), false)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "LEAD(\"value\") OVER (ORDER BY \"ts\" ASC)");
    }

    #[test]
    fn test_lead_with_offset() {
        let window = WindowExpression::new(WindowFunction::Lead {
            expression: Box::new(Expression::col("value")),
            offset: Some(3),
            default: None,
        })
        .order_by(vec![("ts".to_string(), false)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "LEAD(\"value\", 3) OVER (ORDER BY \"ts\" ASC)");
    }

    #[test]
    fn test_first_value() {
        let window = WindowExpression::new(WindowFunction::FirstValue(Box::new(Expression::col(
            "name",
        ))))
        .partition_by(vec!["dept".to_string()])
        .order_by(vec![("hire_date".to_string(), false)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(
            sql,
            "FIRST_VALUE(\"name\") OVER (PARTITION BY \"dept\" ORDER BY \"hire_date\" ASC)"
        );
    }

    #[test]
    fn test_last_value() {
        let window = WindowExpression::new(WindowFunction::LastValue(Box::new(Expression::col(
            "price",
        ))))
        .order_by(vec![("date".to_string(), false)])
        .frame(WindowFrame::rows(
            WindowFrameBound::UnboundedPreceding,
            WindowFrameBound::UnboundedFollowing,
        ));
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(
            sql,
            "LAST_VALUE(\"price\") OVER (ORDER BY \"date\" ASC ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING)"
        );
    }

    #[test]
    fn test_nth_value() {
        let window = WindowExpression::new(WindowFunction::NthValue(
            Box::new(Expression::col("score")),
            3,
        ))
        .order_by(vec![("rank".to_string(), false)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "NTH_VALUE(\"score\", 3) OVER (ORDER BY \"rank\" ASC)");
    }

    #[test]
    fn test_ntile() {
        let window = WindowExpression::new(WindowFunction::Ntile(4))
            .order_by(vec![("score".to_string(), true)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "NTILE(4) OVER (ORDER BY \"score\" DESC)");
    }

    #[test]
    fn test_cume_dist() {
        let window = WindowExpression::new(WindowFunction::CumeDist)
            .order_by(vec![("value".to_string(), false)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "CUME_DIST() OVER (ORDER BY \"value\" ASC)");
    }

    #[test]
    fn test_percent_rank() {
        let window = WindowExpression::new(WindowFunction::PercentRank)
            .order_by(vec![("grade".to_string(), true)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "PERCENT_RANK() OVER (ORDER BY \"grade\" DESC)");
    }

    #[test]
    fn test_aggregate_as_window_function() {
        let sum_expr = Expression::aggregate(AggregateFunc::Sum, Expression::col("amount"));
        let window = WindowExpression::new(WindowFunction::Aggregate(Box::new(sum_expr)))
            .partition_by(vec!["customer_id".to_string()])
            .order_by(vec![("order_date".to_string(), false)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(
            sql,
            "SUM(\"amount\") OVER (PARTITION BY \"customer_id\" ORDER BY \"order_date\" ASC)"
        );
    }

    #[test]
    fn test_window_with_frame() {
        let window = WindowExpression::new(WindowFunction::Aggregate(Box::new(
            Expression::aggregate(AggregateFunc::Avg, Expression::col("price")),
        )))
        .order_by(vec![("date".to_string(), false)])
        .frame(WindowFrame::rows(
            WindowFrameBound::Preceding(7),
            WindowFrameBound::CurrentRow,
        ));
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(
            sql,
            "AVG(\"price\") OVER (ORDER BY \"date\" ASC ROWS BETWEEN 7 PRECEDING AND CURRENT ROW)"
        );
    }

    #[test]
    fn test_window_multiple_partition_by() {
        let window = WindowExpression::new(WindowFunction::RowNumber)
            .partition_by(vec!["dept".to_string(), "team".to_string()])
            .order_by(vec![("hire_date".to_string(), false)]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(
            sql,
            "ROW_NUMBER() OVER (PARTITION BY \"dept\", \"team\" ORDER BY \"hire_date\" ASC)"
        );
    }

    #[test]
    fn test_window_multiple_order_by() {
        let window = WindowExpression::new(WindowFunction::Rank).order_by(vec![
            ("score".to_string(), true),
            ("name".to_string(), false),
        ]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "RANK() OVER (ORDER BY \"score\" DESC, \"name\" ASC)");
    }

    #[test]
    fn test_window_in_annotation() {
        let window = WindowExpression::new(WindowFunction::RowNumber)
            .order_by(vec![("id".to_string(), false)]);

        let mut query = Query::new("products");
        query
            .annotations
            .insert("row_num".to_string(), window.into_expression());

        let compiler = pg();
        let (sql, _) = compiler.compile_select(&query);
        assert!(sql.contains("ROW_NUMBER() OVER (ORDER BY \"id\" ASC) AS \"row_num\""));
    }

    #[test]
    fn test_window_partition_only() {
        let window = WindowExpression::new(WindowFunction::Aggregate(Box::new(
            Expression::aggregate(AggregateFunc::Count, Expression::col("id")),
        )))
        .partition_by(vec!["category".to_string()]);
        let expr = window.into_expression();
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "COUNT(\"id\") OVER (PARTITION BY \"category\")");
    }

    #[test]
    fn test_window_sqlite_backend() {
        let window = WindowExpression::new(WindowFunction::RowNumber)
            .partition_by(vec!["dept".to_string()])
            .order_by(vec![("salary".to_string(), true)]);
        let expr = window.into_expression();
        let compiler = sqlite();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        // SQLite also supports window functions (since 3.25.0)
        assert_eq!(
            sql,
            "ROW_NUMBER() OVER (PARTITION BY \"dept\" ORDER BY \"salary\" DESC)"
        );
    }
}
