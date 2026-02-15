//! SQL query AST and compiler.
//!
//! This module defines the [`Query`] AST that represents a database query, and
//! the [`SqlCompiler`] that translates it into parameterized SQL strings. The
//! compiler supports PostgreSQL (`$1, $2, ...`) and SQLite/MySQL (`?`) parameter
//! placeholder styles.
//!
//! This is the equivalent of Django's `django.db.models.sql.compiler`.

use super::expressions::Expression;
use super::lookups::{Lookup, Q};
use crate::value::Value;
use django_rs_core::DjangoError;
use std::collections::HashMap;

/// The type of database backend, used by the compiler to generate
/// backend-specific SQL syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackendType {
    /// PostgreSQL (uses `$1, $2, ...` placeholders).
    PostgreSQL,
    /// SQLite (uses `?` placeholders).
    SQLite,
    /// MySQL (uses `?` placeholders).
    MySQL,
}

/// A column ordering direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBy {
    /// The column or expression to order by.
    pub column: String,
    /// Whether to sort in descending order.
    pub descending: bool,
    /// Whether to put nulls first or last.
    pub nulls_first: Option<bool>,
}

impl OrderBy {
    /// Creates an ascending order.
    pub fn asc(column: impl Into<String>) -> Self {
        Self {
            column: column.into(),
            descending: false,
            nulls_first: None,
        }
    }

    /// Creates a descending order.
    pub fn desc(column: impl Into<String>) -> Self {
        Self {
            column: column.into(),
            descending: true,
            nulls_first: None,
        }
    }
}

/// A column to select in a query.
#[derive(Debug, Clone)]
pub enum SelectColumn {
    /// A simple column name.
    Column(String),
    /// A column with a table prefix.
    TableColumn(String, String),
    /// An expression with an alias.
    Expression(Expression, String),
    /// All columns (`*`).
    Star,
}

/// A WHERE clause node in the query AST.
#[derive(Debug, Clone)]
pub enum WhereNode {
    /// A single condition.
    Condition {
        /// The column name.
        column: String,
        /// The lookup type.
        lookup: Lookup,
    },
    /// Logical AND of conditions.
    And(Vec<WhereNode>),
    /// Logical OR of conditions.
    Or(Vec<WhereNode>),
    /// Logical NOT of a condition.
    Not(Box<WhereNode>),
}

impl WhereNode {
    /// Converts a `Q` object into a `WhereNode`.
    pub fn from_q(q: &Q) -> Self {
        match q {
            Q::Filter { field, lookup } => Self::Condition {
                column: field.clone(),
                lookup: lookup.clone(),
            },
            Q::And(children) => {
                Self::And(children.iter().map(Self::from_q).collect())
            }
            Q::Or(children) => {
                Self::Or(children.iter().map(Self::from_q).collect())
            }
            Q::Not(inner) => Self::Not(Box::new(Self::from_q(inner))),
        }
    }
}

/// A JOIN clause in the query AST.
#[derive(Debug, Clone)]
pub struct Join {
    /// The table to join.
    pub table: String,
    /// Optional alias for the joined table.
    pub alias: Option<String>,
    /// The type of join.
    pub join_type: JoinType,
    /// The ON condition.
    pub on: WhereNode,
}

/// SQL JOIN types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    /// INNER JOIN.
    Inner,
    /// LEFT OUTER JOIN.
    Left,
    /// RIGHT OUTER JOIN (not supported by SQLite).
    Right,
}

impl JoinType {
    /// Returns the SQL keyword for this join type.
    pub const fn sql_keyword(&self) -> &'static str {
        match self {
            Self::Inner => "INNER JOIN",
            Self::Left => "LEFT JOIN",
            Self::Right => "RIGHT JOIN",
        }
    }
}

/// The complete query AST representing a SELECT statement.
#[derive(Debug, Clone)]
pub struct Query {
    /// The main table name.
    pub table: String,
    /// Columns to select.
    pub select: Vec<SelectColumn>,
    /// WHERE clause.
    pub where_clause: Option<WhereNode>,
    /// ORDER BY clauses.
    pub order_by: Vec<OrderBy>,
    /// GROUP BY columns.
    pub group_by: Vec<String>,
    /// HAVING clause.
    pub having: Option<WhereNode>,
    /// JOIN clauses.
    pub joins: Vec<Join>,
    /// LIMIT.
    pub limit: Option<usize>,
    /// OFFSET.
    pub offset: Option<usize>,
    /// DISTINCT flag.
    pub distinct: bool,
    /// Named annotations (computed columns).
    pub annotations: HashMap<String, Expression>,
    /// Named aggregates.
    pub aggregates: HashMap<String, Expression>,
}

impl Query {
    /// Creates a new query for the given table.
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            select: vec![SelectColumn::Star],
            where_clause: None,
            order_by: Vec::new(),
            group_by: Vec::new(),
            having: None,
            joins: Vec::new(),
            limit: None,
            offset: None,
            distinct: false,
            annotations: HashMap::new(),
            aggregates: HashMap::new(),
        }
    }
}

/// A generic database row for passing data between backends and the ORM.
///
/// `Row` holds a list of column names and their corresponding values. It
/// provides typed access via the [`get`](Row::get) method.
#[derive(Debug, Clone)]
pub struct Row {
    columns: Vec<String>,
    values: Vec<Value>,
}

impl Row {
    /// Creates a new row from column names and values.
    ///
    /// # Panics
    ///
    /// Panics if the number of columns does not match the number of values.
    pub fn new(columns: Vec<String>, values: Vec<Value>) -> Self {
        assert_eq!(
            columns.len(),
            values.len(),
            "Row column count must match value count"
        );
        Self { columns, values }
    }

    /// Returns the column names.
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// Returns the number of columns.
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Returns `true` if the row has no columns.
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Gets a typed value by column name.
    ///
    /// # Errors
    ///
    /// Returns an error if the column does not exist or the value cannot be
    /// converted to the requested type.
    pub fn get<T: FromValue>(&self, column: &str) -> Result<T, DjangoError> {
        let idx = self
            .columns
            .iter()
            .position(|c| c == column)
            .ok_or_else(|| {
                DjangoError::DatabaseError(format!("Column '{column}' not found in row"))
            })?;
        T::from_value(&self.values[idx])
    }

    /// Gets a typed value by column index.
    ///
    /// # Errors
    ///
    /// Returns an error if the index is out of bounds or the value cannot be
    /// converted to the requested type.
    pub fn get_by_index<T: FromValue>(&self, idx: usize) -> Result<T, DjangoError> {
        if idx >= self.values.len() {
            return Err(DjangoError::DatabaseError(format!(
                "Column index {idx} out of bounds (row has {} columns)",
                self.values.len()
            )));
        }
        T::from_value(&self.values[idx])
    }

    /// Returns a reference to the raw Value at the given column name.
    pub fn get_value(&self, column: &str) -> Option<&Value> {
        self.columns
            .iter()
            .position(|c| c == column)
            .map(|idx| &self.values[idx])
    }
}

/// Trait for converting a [`Value`] to a concrete Rust type.
pub trait FromValue: Sized {
    /// Attempts to convert a value reference to this type.
    fn from_value(value: &Value) -> Result<Self, DjangoError>;
}

impl FromValue for i64 {
    fn from_value(value: &Value) -> Result<Self, DjangoError> {
        match value {
            Value::Int(i) => Ok(*i),
            _ => Err(DjangoError::DatabaseError(format!(
                "Expected Int, got {value:?}"
            ))),
        }
    }
}

impl FromValue for i32 {
    fn from_value(value: &Value) -> Result<Self, DjangoError> {
        match value {
            Value::Int(i) => i32::try_from(*i).map_err(|e| {
                DjangoError::DatabaseError(format!("Int value out of i32 range: {e}"))
            }),
            _ => Err(DjangoError::DatabaseError(format!(
                "Expected Int, got {value:?}"
            ))),
        }
    }
}

impl FromValue for f64 {
    fn from_value(value: &Value) -> Result<Self, DjangoError> {
        match value {
            Value::Float(f) => Ok(*f),
            Value::Int(i) => Ok(*i as f64),
            _ => Err(DjangoError::DatabaseError(format!(
                "Expected Float, got {value:?}"
            ))),
        }
    }
}

impl FromValue for bool {
    fn from_value(value: &Value) -> Result<Self, DjangoError> {
        match value {
            Value::Bool(b) => Ok(*b),
            _ => Err(DjangoError::DatabaseError(format!(
                "Expected Bool, got {value:?}"
            ))),
        }
    }
}

impl FromValue for String {
    fn from_value(value: &Value) -> Result<Self, DjangoError> {
        match value {
            Value::String(s) => Ok(s.clone()),
            _ => Err(DjangoError::DatabaseError(format!(
                "Expected String, got {value:?}"
            ))),
        }
    }
}

impl FromValue for uuid::Uuid {
    fn from_value(value: &Value) -> Result<Self, DjangoError> {
        match value {
            Value::Uuid(u) => Ok(*u),
            _ => Err(DjangoError::DatabaseError(format!(
                "Expected Uuid, got {value:?}"
            ))),
        }
    }
}

impl FromValue for Value {
    fn from_value(value: &Value) -> Result<Self, DjangoError> {
        Ok(value.clone())
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: &Value) -> Result<Self, DjangoError> {
        match value {
            Value::Null => Ok(None),
            _ => T::from_value(value).map(Some),
        }
    }
}

/// The SQL compiler translates a [`Query`] AST into parameterized SQL.
///
/// Different backends use different placeholder styles:
/// - PostgreSQL: `$1, $2, $3, ...`
/// - SQLite / MySQL: `?, ?, ?, ...`
pub struct SqlCompiler {
    backend: DatabaseBackendType,
}

impl SqlCompiler {
    /// Creates a new compiler for the given backend type.
    pub const fn new(backend: DatabaseBackendType) -> Self {
        Self { backend }
    }

    /// Returns a parameter placeholder for the given 1-based index.
    fn placeholder(&self, index: usize) -> String {
        match self.backend {
            DatabaseBackendType::PostgreSQL => format!("${index}"),
            DatabaseBackendType::SQLite | DatabaseBackendType::MySQL => "?".to_string(),
        }
    }

    /// Compiles a SELECT query into SQL and parameters.
    pub fn compile_select(&self, query: &Query) -> (String, Vec<Value>) {
        let mut params: Vec<Value> = Vec::new();
        let mut sql = String::from("SELECT ");

        if query.distinct {
            sql.push_str("DISTINCT ");
        }

        // SELECT columns
        let select_parts: Vec<String> = if query.select.is_empty() {
            vec!["*".to_string()]
        } else {
            query
                .select
                .iter()
                .map(|col| match col {
                    SelectColumn::Column(name) => format!("\"{name}\""),
                    SelectColumn::TableColumn(table, name) => {
                        format!("\"{table}\".\"{name}\"")
                    }
                    SelectColumn::Expression(expr, alias) => {
                        let expr_sql = self.compile_expression(expr, &mut params);
                        format!("{expr_sql} AS \"{alias}\"")
                    }
                    SelectColumn::Star => "*".to_string(),
                })
                .collect()
        };
        sql.push_str(&select_parts.join(", "));

        // Add annotations as selected columns
        for (alias, expr) in &query.annotations {
            let expr_sql = self.compile_expression(expr, &mut params);
            sql.push_str(&format!(", {expr_sql} AS \"{alias}\""));
        }

        // FROM
        sql.push_str(&format!(" FROM \"{}\"", query.table));

        // JOINs
        for join in &query.joins {
            let alias = join
                .alias
                .as_deref()
                .unwrap_or(&join.table);
            sql.push_str(&format!(
                " {} \"{}\" AS \"{}\" ON ",
                join.join_type.sql_keyword(),
                join.table,
                alias
            ));
            self.compile_where_node(&join.on, &mut sql, &mut params);
        }

        // WHERE
        if let Some(ref where_clause) = query.where_clause {
            sql.push_str(" WHERE ");
            self.compile_where_node(where_clause, &mut sql, &mut params);
        }

        // GROUP BY
        if !query.group_by.is_empty() {
            let cols: Vec<String> = query.group_by.iter().map(|c| format!("\"{c}\"")).collect();
            sql.push_str(&format!(" GROUP BY {}", cols.join(", ")));
        }

        // HAVING
        if let Some(ref having) = query.having {
            sql.push_str(" HAVING ");
            self.compile_where_node(having, &mut sql, &mut params);
        }

        // ORDER BY
        if !query.order_by.is_empty() {
            let orders: Vec<String> = query
                .order_by
                .iter()
                .map(|o| {
                    let dir = if o.descending { " DESC" } else { " ASC" };
                    let nulls = match o.nulls_first {
                        Some(true) => " NULLS FIRST",
                        Some(false) => " NULLS LAST",
                        None => "",
                    };
                    format!("\"{}\"{dir}{nulls}", o.column)
                })
                .collect();
            sql.push_str(&format!(" ORDER BY {}", orders.join(", ")));
        }

        // LIMIT
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }

        // OFFSET
        if let Some(offset) = query.offset {
            sql.push_str(&format!(" OFFSET {offset}"));
        }

        (sql, params)
    }

    /// Compiles an INSERT statement.
    pub fn compile_insert(
        &self,
        table: &str,
        fields: &[(&str, Value)],
    ) -> (String, Vec<Value>) {
        let mut params = Vec::new();
        let columns: Vec<String> = fields.iter().map(|(name, _)| format!("\"{name}\"")).collect();
        let placeholders: Vec<String> = fields
            .iter()
            .enumerate()
            .map(|(i, (_, val))| {
                params.push(val.clone());
                self.placeholder(i + 1)
            })
            .collect();

        let sql = format!(
            "INSERT INTO \"{}\" ({}) VALUES ({})",
            table,
            columns.join(", "),
            placeholders.join(", ")
        );

        (sql, params)
    }

    /// Compiles an UPDATE statement.
    pub fn compile_update(
        &self,
        table: &str,
        fields: &[(&str, Value)],
        where_clause: &WhereNode,
    ) -> (String, Vec<Value>) {
        let mut params = Vec::new();
        let set_parts: Vec<String> = fields
            .iter()
            .enumerate()
            .map(|(i, (name, val))| {
                params.push(val.clone());
                let ph = self.placeholder(i + 1);
                format!("\"{name}\" = {ph}")
            })
            .collect();

        let mut sql = format!(
            "UPDATE \"{}\" SET {} WHERE ",
            table,
            set_parts.join(", ")
        );

        self.compile_where_node(where_clause, &mut sql, &mut params);

        (sql, params)
    }

    /// Compiles a DELETE statement.
    pub fn compile_delete(
        &self,
        table: &str,
        where_clause: &WhereNode,
    ) -> (String, Vec<Value>) {
        let mut params = Vec::new();
        let mut sql = format!("DELETE FROM \"{table}\" WHERE ");
        self.compile_where_node(where_clause, &mut sql, &mut params);
        (sql, params)
    }

    /// Compiles a `WhereNode` into SQL, appending to the provided string.
    fn compile_where_node(
        &self,
        node: &WhereNode,
        sql: &mut String,
        params: &mut Vec<Value>,
    ) {
        match node {
            WhereNode::Condition { column, lookup } => {
                self.compile_lookup(column, lookup, sql, params);
            }
            WhereNode::And(children) => {
                if children.is_empty() {
                    sql.push_str("1=1");
                    return;
                }
                sql.push('(');
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        sql.push_str(" AND ");
                    }
                    self.compile_where_node(child, sql, params);
                }
                sql.push(')');
            }
            WhereNode::Or(children) => {
                if children.is_empty() {
                    sql.push_str("1=0");
                    return;
                }
                sql.push('(');
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        sql.push_str(" OR ");
                    }
                    self.compile_where_node(child, sql, params);
                }
                sql.push(')');
            }
            WhereNode::Not(inner) => {
                sql.push_str("NOT (");
                self.compile_where_node(inner, sql, params);
                sql.push(')');
            }
        }
    }

    /// Compiles a single lookup into SQL.
    fn compile_lookup(
        &self,
        column: &str,
        lookup: &Lookup,
        sql: &mut String,
        params: &mut Vec<Value>,
    ) {
        match lookup {
            Lookup::Exact(val) => {
                if val.is_null() {
                    sql.push_str(&format!("\"{column}\" IS NULL"));
                } else {
                    params.push(val.clone());
                    let ph = self.placeholder(params.len());
                    sql.push_str(&format!("\"{column}\" = {ph}"));
                }
            }
            Lookup::IExact(val) => {
                params.push(val.clone());
                let ph = self.placeholder(params.len());
                sql.push_str(&format!("LOWER(\"{column}\") = LOWER({ph})"));
            }
            Lookup::Contains(val) => {
                params.push(Value::String(format!("%{val}%")));
                let ph = self.placeholder(params.len());
                sql.push_str(&format!("\"{column}\" LIKE {ph}"));
            }
            Lookup::IContains(val) => {
                params.push(Value::String(format!("%{val}%")));
                let ph = self.placeholder(params.len());
                match self.backend {
                    DatabaseBackendType::PostgreSQL => {
                        sql.push_str(&format!("\"{column}\" ILIKE {ph}"));
                    }
                    _ => {
                        sql.push_str(&format!("LOWER(\"{column}\") LIKE LOWER({ph})"));
                    }
                }
            }
            Lookup::In(vals) => {
                let placeholders: Vec<String> = vals
                    .iter()
                    .map(|v| {
                        params.push(v.clone());
                        self.placeholder(params.len())
                    })
                    .collect();
                sql.push_str(&format!(
                    "\"{column}\" IN ({})",
                    placeholders.join(", ")
                ));
            }
            Lookup::Gt(val) => {
                params.push(val.clone());
                let ph = self.placeholder(params.len());
                sql.push_str(&format!("\"{column}\" > {ph}"));
            }
            Lookup::Gte(val) => {
                params.push(val.clone());
                let ph = self.placeholder(params.len());
                sql.push_str(&format!("\"{column}\" >= {ph}"));
            }
            Lookup::Lt(val) => {
                params.push(val.clone());
                let ph = self.placeholder(params.len());
                sql.push_str(&format!("\"{column}\" < {ph}"));
            }
            Lookup::Lte(val) => {
                params.push(val.clone());
                let ph = self.placeholder(params.len());
                sql.push_str(&format!("\"{column}\" <= {ph}"));
            }
            Lookup::StartsWith(val) => {
                params.push(Value::String(format!("{val}%")));
                let ph = self.placeholder(params.len());
                sql.push_str(&format!("\"{column}\" LIKE {ph}"));
            }
            Lookup::IStartsWith(val) => {
                params.push(Value::String(format!("{val}%")));
                let ph = self.placeholder(params.len());
                match self.backend {
                    DatabaseBackendType::PostgreSQL => {
                        sql.push_str(&format!("\"{column}\" ILIKE {ph}"));
                    }
                    _ => {
                        sql.push_str(&format!("LOWER(\"{column}\") LIKE LOWER({ph})"));
                    }
                }
            }
            Lookup::EndsWith(val) => {
                params.push(Value::String(format!("%{val}")));
                let ph = self.placeholder(params.len());
                sql.push_str(&format!("\"{column}\" LIKE {ph}"));
            }
            Lookup::IEndsWith(val) => {
                params.push(Value::String(format!("%{val}")));
                let ph = self.placeholder(params.len());
                match self.backend {
                    DatabaseBackendType::PostgreSQL => {
                        sql.push_str(&format!("\"{column}\" ILIKE {ph}"));
                    }
                    _ => {
                        sql.push_str(&format!("LOWER(\"{column}\") LIKE LOWER({ph})"));
                    }
                }
            }
            Lookup::Range(low, high) => {
                params.push(low.clone());
                let ph_low = self.placeholder(params.len());
                params.push(high.clone());
                let ph_high = self.placeholder(params.len());
                sql.push_str(&format!("\"{column}\" BETWEEN {ph_low} AND {ph_high}"));
            }
            Lookup::IsNull(is_null) => {
                if *is_null {
                    sql.push_str(&format!("\"{column}\" IS NULL"));
                } else {
                    sql.push_str(&format!("\"{column}\" IS NOT NULL"));
                }
            }
            Lookup::Regex(pattern) => {
                params.push(Value::String(pattern.clone()));
                let ph = self.placeholder(params.len());
                match self.backend {
                    DatabaseBackendType::PostgreSQL => {
                        sql.push_str(&format!("\"{column}\" ~ {ph}"));
                    }
                    DatabaseBackendType::MySQL => {
                        sql.push_str(&format!("\"{column}\" REGEXP {ph}"));
                    }
                    DatabaseBackendType::SQLite => {
                        sql.push_str(&format!("\"{column}\" REGEXP {ph}"));
                    }
                }
            }
            Lookup::IRegex(pattern) => {
                params.push(Value::String(pattern.clone()));
                let ph = self.placeholder(params.len());
                match self.backend {
                    DatabaseBackendType::PostgreSQL => {
                        sql.push_str(&format!("\"{column}\" ~* {ph}"));
                    }
                    DatabaseBackendType::MySQL => {
                        sql.push_str(&format!("\"{column}\" REGEXP {ph}"));
                    }
                    DatabaseBackendType::SQLite => {
                        sql.push_str(&format!("\"{column}\" REGEXP {ph}"));
                    }
                }
            }
        }
    }

    /// Compiles an expression into SQL.
    fn compile_expression(
        &self,
        expr: &Expression,
        params: &mut Vec<Value>,
    ) -> String {
        match expr {
            Expression::Col(name) => format!("\"{name}\""),
            Expression::Value(val) => {
                params.push(val.clone());
                self.placeholder(params.len())
            }
            Expression::F(name) => format!("\"{name}\""),
            Expression::Func { name, args } => {
                let arg_parts: Vec<String> = args
                    .iter()
                    .map(|a| self.compile_expression(a, params))
                    .collect();
                format!("{name}({})", arg_parts.join(", "))
            }
            Expression::Aggregate {
                func,
                field,
                distinct,
                ..
            } => {
                let field_sql = self.compile_expression(field, params);
                let distinct_str = if *distinct { "DISTINCT " } else { "" };
                format!("{}({distinct_str}{field_sql})", func.sql_name())
            }
            Expression::Case { whens, default } => {
                let mut sql = "CASE".to_string();
                for when in whens {
                    sql.push_str(" WHEN ");
                    let node = WhereNode::from_q(&when.condition);
                    let mut cond_sql = String::new();
                    self.compile_where_node(&node, &mut cond_sql, params);
                    sql.push_str(&cond_sql);
                    sql.push_str(" THEN ");
                    sql.push_str(&self.compile_expression(&when.then, params));
                }
                if let Some(default) = default {
                    sql.push_str(" ELSE ");
                    sql.push_str(&self.compile_expression(default, params));
                }
                sql.push_str(" END");
                sql
            }
            Expression::Subquery(query) => {
                let (sub_sql, sub_params) = self.compile_select(query);
                params.extend(sub_params);
                format!("({sub_sql})")
            }
            Expression::RawSQL(raw, raw_params) => {
                params.extend(raw_params.clone());
                raw.clone()
            }
            Expression::Add(left, right) => {
                let l = self.compile_expression(left, params);
                let r = self.compile_expression(right, params);
                format!("({l} + {r})")
            }
            Expression::Sub(left, right) => {
                let l = self.compile_expression(left, params);
                let r = self.compile_expression(right, params);
                format!("({l} - {r})")
            }
            Expression::Mul(left, right) => {
                let l = self.compile_expression(left, params);
                let r = self.compile_expression(right, params);
                format!("({l} * {r})")
            }
            Expression::Div(left, right) => {
                let l = self.compile_expression(left, params);
                let r = self.compile_expression(right, params);
                format!("({l} / {r})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::expressions::AggregateFunc;

    fn pg() -> SqlCompiler {
        SqlCompiler::new(DatabaseBackendType::PostgreSQL)
    }

    fn sqlite() -> SqlCompiler {
        SqlCompiler::new(DatabaseBackendType::SQLite)
    }

    fn mysql() -> SqlCompiler {
        SqlCompiler::new(DatabaseBackendType::MySQL)
    }

    // ── Row tests ────────────────────────────────────────────────────

    #[test]
    fn test_row_get_string() {
        let row = Row::new(
            vec!["name".to_string()],
            vec![Value::String("Alice".to_string())],
        );
        assert_eq!(row.get::<String>("name").unwrap(), "Alice");
    }

    #[test]
    fn test_row_get_int() {
        let row = Row::new(vec!["id".to_string()], vec![Value::Int(42)]);
        assert_eq!(row.get::<i64>("id").unwrap(), 42);
    }

    #[test]
    fn test_row_get_i32() {
        let row = Row::new(vec!["count".to_string()], vec![Value::Int(10)]);
        assert_eq!(row.get::<i32>("count").unwrap(), 10);
    }

    #[test]
    fn test_row_get_bool() {
        let row = Row::new(
            vec!["active".to_string()],
            vec![Value::Bool(true)],
        );
        assert!(row.get::<bool>("active").unwrap());
    }

    #[test]
    fn test_row_get_float() {
        let row = Row::new(
            vec!["price".to_string()],
            vec![Value::Float(9.99)],
        );
        let price: f64 = row.get("price").unwrap();
        assert!((price - 9.99).abs() < f64::EPSILON);
    }

    #[test]
    fn test_row_get_optional_some() {
        let row = Row::new(
            vec!["bio".to_string()],
            vec![Value::String("hello".to_string())],
        );
        let bio: Option<String> = row.get("bio").unwrap();
        assert_eq!(bio, Some("hello".to_string()));
    }

    #[test]
    fn test_row_get_optional_none() {
        let row = Row::new(vec!["bio".to_string()], vec![Value::Null]);
        let bio: Option<String> = row.get("bio").unwrap();
        assert_eq!(bio, None);
    }

    #[test]
    fn test_row_get_missing_column() {
        let row = Row::new(vec!["name".to_string()], vec![Value::String("test".into())]);
        assert!(row.get::<String>("missing").is_err());
    }

    #[test]
    fn test_row_get_by_index() {
        let row = Row::new(
            vec!["a".to_string(), "b".to_string()],
            vec![Value::Int(1), Value::Int(2)],
        );
        assert_eq!(row.get_by_index::<i64>(0).unwrap(), 1);
        assert_eq!(row.get_by_index::<i64>(1).unwrap(), 2);
    }

    #[test]
    fn test_row_get_by_index_out_of_bounds() {
        let row = Row::new(vec!["a".to_string()], vec![Value::Int(1)]);
        assert!(row.get_by_index::<i64>(5).is_err());
    }

    #[test]
    fn test_row_columns() {
        let row = Row::new(
            vec!["a".to_string(), "b".to_string()],
            vec![Value::Int(1), Value::Int(2)],
        );
        assert_eq!(row.columns(), &["a".to_string(), "b".to_string()]);
        assert_eq!(row.len(), 2);
        assert!(!row.is_empty());
    }

    #[test]
    fn test_row_empty() {
        let row = Row::new(vec![], vec![]);
        assert!(row.is_empty());
        assert_eq!(row.len(), 0);
    }

    #[test]
    fn test_row_get_value() {
        let row = Row::new(vec!["x".to_string()], vec![Value::Int(42)]);
        assert_eq!(row.get_value("x"), Some(&Value::Int(42)));
        assert_eq!(row.get_value("y"), None);
    }

    // ── SELECT compilation tests ─────────────────────────────────────

    #[test]
    fn test_simple_select_pg() {
        let query = Query::new("users");
        let (sql, params) = pg().compile_select(&query);
        assert_eq!(sql, "SELECT * FROM \"users\"");
        assert!(params.is_empty());
    }

    #[test]
    fn test_select_with_where_pg() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "name".to_string(),
            lookup: Lookup::Exact(Value::from("Alice")),
        });
        let (sql, params) = pg().compile_select(&query);
        assert_eq!(sql, "SELECT * FROM \"users\" WHERE \"name\" = $1");
        assert_eq!(params, vec![Value::String("Alice".to_string())]);
    }

    #[test]
    fn test_select_with_where_sqlite() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "name".to_string(),
            lookup: Lookup::Exact(Value::from("Alice")),
        });
        let (sql, params) = sqlite().compile_select(&query);
        assert_eq!(sql, "SELECT * FROM \"users\" WHERE \"name\" = ?");
        assert_eq!(params, vec![Value::String("Alice".to_string())]);
    }

    #[test]
    fn test_select_with_where_mysql() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "id".to_string(),
            lookup: Lookup::Gt(Value::from(10)),
        });
        let (sql, params) = mysql().compile_select(&query);
        assert_eq!(sql, "SELECT * FROM \"users\" WHERE \"id\" > ?");
        assert_eq!(params, vec![Value::Int(10)]);
    }

    #[test]
    fn test_select_with_and_where() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::And(vec![
            WhereNode::Condition {
                column: "name".to_string(),
                lookup: Lookup::Exact(Value::from("Alice")),
            },
            WhereNode::Condition {
                column: "age".to_string(),
                lookup: Lookup::Gt(Value::from(25)),
            },
        ]));
        let (sql, params) = pg().compile_select(&query);
        assert_eq!(
            sql,
            "SELECT * FROM \"users\" WHERE (\"name\" = $1 AND \"age\" > $2)"
        );
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_select_with_or_where() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Or(vec![
            WhereNode::Condition {
                column: "name".to_string(),
                lookup: Lookup::Exact(Value::from("Alice")),
            },
            WhereNode::Condition {
                column: "name".to_string(),
                lookup: Lookup::Exact(Value::from("Bob")),
            },
        ]));
        let (sql, _params) = pg().compile_select(&query);
        assert_eq!(
            sql,
            "SELECT * FROM \"users\" WHERE (\"name\" = $1 OR \"name\" = $2)"
        );
    }

    #[test]
    fn test_select_with_not_where() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Not(Box::new(WhereNode::Condition {
            column: "active".to_string(),
            lookup: Lookup::Exact(Value::from(false)),
        })));
        let (sql, _) = pg().compile_select(&query);
        assert_eq!(
            sql,
            "SELECT * FROM \"users\" WHERE NOT (\"active\" = $1)"
        );
    }

    #[test]
    fn test_select_with_order_by() {
        let mut query = Query::new("users");
        query.order_by = vec![
            OrderBy::asc("name"),
            OrderBy::desc("created_at"),
        ];
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("ORDER BY \"name\" ASC, \"created_at\" DESC"));
    }

    #[test]
    fn test_select_with_limit_offset() {
        let mut query = Query::new("users");
        query.limit = Some(10);
        query.offset = Some(20);
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 20"));
    }

    #[test]
    fn test_select_distinct() {
        let mut query = Query::new("users");
        query.distinct = true;
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.starts_with("SELECT DISTINCT *"));
    }

    #[test]
    fn test_select_group_by() {
        let mut query = Query::new("orders");
        query.select = vec![SelectColumn::Column("status".to_string())];
        query.group_by = vec!["status".to_string()];
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("GROUP BY \"status\""));
    }

    #[test]
    fn test_select_with_specific_columns() {
        let mut query = Query::new("users");
        query.select = vec![
            SelectColumn::Column("name".to_string()),
            SelectColumn::Column("email".to_string()),
        ];
        let (sql, _) = pg().compile_select(&query);
        assert_eq!(sql, "SELECT \"name\", \"email\" FROM \"users\"");
    }

    // ── Lookup compilation tests ─────────────────────────────────────

    #[test]
    fn test_lookup_is_null() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "bio".to_string(),
            lookup: Lookup::IsNull(true),
        });
        let (sql, params) = pg().compile_select(&query);
        assert_eq!(sql, "SELECT * FROM \"users\" WHERE \"bio\" IS NULL");
        assert!(params.is_empty());
    }

    #[test]
    fn test_lookup_is_not_null() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "bio".to_string(),
            lookup: Lookup::IsNull(false),
        });
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("\"bio\" IS NOT NULL"));
    }

    #[test]
    fn test_lookup_contains() {
        let mut query = Query::new("posts");
        query.where_clause = Some(WhereNode::Condition {
            column: "title".to_string(),
            lookup: Lookup::Contains("rust".to_string()),
        });
        let (sql, params) = pg().compile_select(&query);
        assert_eq!(
            sql,
            "SELECT * FROM \"posts\" WHERE \"title\" LIKE $1"
        );
        assert_eq!(params, vec![Value::String("%rust%".to_string())]);
    }

    #[test]
    fn test_lookup_icontains_pg() {
        let mut query = Query::new("posts");
        query.where_clause = Some(WhereNode::Condition {
            column: "title".to_string(),
            lookup: Lookup::IContains("rust".to_string()),
        });
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("ILIKE"));
    }

    #[test]
    fn test_lookup_icontains_sqlite() {
        let mut query = Query::new("posts");
        query.where_clause = Some(WhereNode::Condition {
            column: "title".to_string(),
            lookup: Lookup::IContains("rust".to_string()),
        });
        let (sql, _) = sqlite().compile_select(&query);
        assert!(sql.contains("LOWER"));
    }

    #[test]
    fn test_lookup_in() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "id".to_string(),
            lookup: Lookup::In(vec![Value::from(1), Value::from(2), Value::from(3)]),
        });
        let (sql, params) = pg().compile_select(&query);
        assert_eq!(
            sql,
            "SELECT * FROM \"users\" WHERE \"id\" IN ($1, $2, $3)"
        );
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_lookup_range() {
        let mut query = Query::new("products");
        query.where_clause = Some(WhereNode::Condition {
            column: "price".to_string(),
            lookup: Lookup::Range(Value::from(10), Value::from(100)),
        });
        let (sql, params) = pg().compile_select(&query);
        assert_eq!(
            sql,
            "SELECT * FROM \"products\" WHERE \"price\" BETWEEN $1 AND $2"
        );
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_lookup_starts_with() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "name".to_string(),
            lookup: Lookup::StartsWith("Al".to_string()),
        });
        let (sql, params) = pg().compile_select(&query);
        assert!(sql.contains("LIKE $1"));
        assert_eq!(params, vec![Value::String("Al%".to_string())]);
    }

    #[test]
    fn test_lookup_ends_with() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "email".to_string(),
            lookup: Lookup::EndsWith(".com".to_string()),
        });
        let (sql, params) = pg().compile_select(&query);
        assert!(sql.contains("LIKE $1"));
        assert_eq!(params, vec![Value::String("%.com".to_string())]);
    }

    #[test]
    fn test_lookup_iexact() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "name".to_string(),
            lookup: Lookup::IExact(Value::from("alice")),
        });
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("LOWER(\"name\") = LOWER($1)"));
    }

    #[test]
    fn test_lookup_gte_lte() {
        let mut query = Query::new("products");
        query.where_clause = Some(WhereNode::And(vec![
            WhereNode::Condition {
                column: "price".to_string(),
                lookup: Lookup::Gte(Value::from(10)),
            },
            WhereNode::Condition {
                column: "price".to_string(),
                lookup: Lookup::Lte(Value::from(100)),
            },
        ]));
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("\"price\" >= $1"));
        assert!(sql.contains("\"price\" <= $2"));
    }

    #[test]
    fn test_lookup_regex_pg() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "name".to_string(),
            lookup: Lookup::Regex("^A.*".to_string()),
        });
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("\"name\" ~ $1"));
    }

    #[test]
    fn test_lookup_iregex_pg() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "name".to_string(),
            lookup: Lookup::IRegex("^a.*".to_string()),
        });
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("\"name\" ~* $1"));
    }

    #[test]
    fn test_lookup_regex_mysql() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "name".to_string(),
            lookup: Lookup::Regex("^A.*".to_string()),
        });
        let (sql, _) = mysql().compile_select(&query);
        assert!(sql.contains("REGEXP"));
    }

    #[test]
    fn test_lookup_exact_null() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "bio".to_string(),
            lookup: Lookup::Exact(Value::Null),
        });
        let (sql, params) = pg().compile_select(&query);
        assert_eq!(sql, "SELECT * FROM \"users\" WHERE \"bio\" IS NULL");
        assert!(params.is_empty());
    }

    // ── INSERT compilation tests ─────────────────────────────────────

    #[test]
    fn test_insert_pg() {
        let fields: Vec<(&str, Value)> = vec![
            ("name", Value::from("Alice")),
            ("age", Value::from(30)),
        ];
        let (sql, params) = pg().compile_insert("users", &fields);
        assert_eq!(
            sql,
            "INSERT INTO \"users\" (\"name\", \"age\") VALUES ($1, $2)"
        );
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_insert_sqlite() {
        let fields: Vec<(&str, Value)> = vec![
            ("name", Value::from("Bob")),
            ("email", Value::from("bob@test.com")),
        ];
        let (sql, params) = sqlite().compile_insert("users", &fields);
        assert_eq!(
            sql,
            "INSERT INTO \"users\" (\"name\", \"email\") VALUES (?, ?)"
        );
        assert_eq!(params.len(), 2);
    }

    // ── UPDATE compilation tests ─────────────────────────────────────

    #[test]
    fn test_update_pg() {
        let fields: Vec<(&str, Value)> = vec![("name", Value::from("Alice Updated"))];
        let where_clause = WhereNode::Condition {
            column: "id".to_string(),
            lookup: Lookup::Exact(Value::from(1)),
        };
        let (sql, params) = pg().compile_update("users", &fields, &where_clause);
        assert_eq!(
            sql,
            "UPDATE \"users\" SET \"name\" = $1 WHERE \"id\" = $2"
        );
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_update_sqlite() {
        let fields: Vec<(&str, Value)> = vec![
            ("name", Value::from("Updated")),
            ("age", Value::from(31)),
        ];
        let where_clause = WhereNode::Condition {
            column: "id".to_string(),
            lookup: Lookup::Exact(Value::from(1)),
        };
        let (sql, params) = sqlite().compile_update("users", &fields, &where_clause);
        assert_eq!(
            sql,
            "UPDATE \"users\" SET \"name\" = ?, \"age\" = ? WHERE \"id\" = ?"
        );
        assert_eq!(params.len(), 3);
    }

    // ── DELETE compilation tests ─────────────────────────────────────

    #[test]
    fn test_delete_pg() {
        let where_clause = WhereNode::Condition {
            column: "id".to_string(),
            lookup: Lookup::Exact(Value::from(1)),
        };
        let (sql, params) = pg().compile_delete("users", &where_clause);
        assert_eq!(sql, "DELETE FROM \"users\" WHERE \"id\" = $1");
        assert_eq!(params, vec![Value::Int(1)]);
    }

    #[test]
    fn test_delete_sqlite() {
        let where_clause = WhereNode::Condition {
            column: "id".to_string(),
            lookup: Lookup::Exact(Value::from(1)),
        };
        let (sql, _) = sqlite().compile_delete("users", &where_clause);
        assert_eq!(sql, "DELETE FROM \"users\" WHERE \"id\" = ?");
    }

    // ── Expression compilation tests ─────────────────────────────────

    #[test]
    fn test_compile_annotation() {
        let mut query = Query::new("products");
        query.annotations.insert(
            "total".to_string(),
            Expression::Mul(
                Box::new(Expression::F("price".to_string())),
                Box::new(Expression::F("quantity".to_string())),
            ),
        );
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("(\"price\" * \"quantity\") AS \"total\""));
    }

    #[test]
    fn test_compile_aggregate_count() {
        let compiler = pg();
        let mut params = Vec::new();
        let expr = Expression::aggregate(AggregateFunc::Count, Expression::col("id"));
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "COUNT(\"id\")");
    }

    #[test]
    fn test_compile_aggregate_count_distinct() {
        let compiler = pg();
        let mut params = Vec::new();
        let expr = Expression::aggregate_distinct(AggregateFunc::Count, Expression::col("category"));
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "COUNT(DISTINCT \"category\")");
    }

    #[test]
    fn test_compile_func() {
        let compiler = pg();
        let mut params = Vec::new();
        let expr = Expression::func("COALESCE", vec![
            Expression::col("name"),
            Expression::value("unknown"),
        ]);
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "COALESCE(\"name\", $1)");
    }

    // ── WhereNode from Q ─────────────────────────────────────────────

    #[test]
    fn test_where_node_from_q_filter() {
        let q = Q::filter("name", Lookup::Exact(Value::from("test")));
        let node = WhereNode::from_q(&q);
        assert!(matches!(node, WhereNode::Condition { .. }));
    }

    #[test]
    fn test_where_node_from_q_and() {
        let q = Q::filter("a", Lookup::Exact(Value::from(1)))
            & Q::filter("b", Lookup::Exact(Value::from(2)));
        let node = WhereNode::from_q(&q);
        assert!(matches!(node, WhereNode::And(_)));
    }

    #[test]
    fn test_where_node_from_q_not() {
        let q = !Q::filter("active", Lookup::Exact(Value::from(true)));
        let node = WhereNode::from_q(&q);
        assert!(matches!(node, WhereNode::Not(_)));
    }

    // ── Empty AND/OR ─────────────────────────────────────────────────

    #[test]
    fn test_empty_and_produces_true() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::And(vec![]));
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("1=1"));
    }

    #[test]
    fn test_empty_or_produces_false() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Or(vec![]));
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("1=0"));
    }

    // ── Order by nulls ──────────────────────────────────────────────

    #[test]
    fn test_order_by_nulls_first() {
        let mut query = Query::new("users");
        query.order_by = vec![OrderBy {
            column: "name".to_string(),
            descending: false,
            nulls_first: Some(true),
        }];
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("NULLS FIRST"));
    }

    // ── JOIN compilation ─────────────────────────────────────────────

    #[test]
    fn test_select_with_join() {
        let mut query = Query::new("posts");
        query.joins.push(Join {
            table: "users".to_string(),
            alias: Some("author".to_string()),
            join_type: JoinType::Inner,
            on: WhereNode::Condition {
                column: "posts\".\"author_id\" = \"author\".\"id".to_string(),
                lookup: Lookup::IsNull(false), // just to have a valid node
            },
        });
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("INNER JOIN"));
    }

    #[test]
    fn test_join_type_sql_keywords() {
        assert_eq!(JoinType::Inner.sql_keyword(), "INNER JOIN");
        assert_eq!(JoinType::Left.sql_keyword(), "LEFT JOIN");
        assert_eq!(JoinType::Right.sql_keyword(), "RIGHT JOIN");
    }

    // ── Multiple params correctness ──────────────────────────────────

    #[test]
    fn test_pg_param_numbering() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::And(vec![
            WhereNode::Condition {
                column: "a".to_string(),
                lookup: Lookup::Exact(Value::from(1)),
            },
            WhereNode::Condition {
                column: "b".to_string(),
                lookup: Lookup::Exact(Value::from(2)),
            },
            WhereNode::Condition {
                column: "c".to_string(),
                lookup: Lookup::Exact(Value::from(3)),
            },
        ]));
        let (sql, params) = pg().compile_select(&query);
        assert!(sql.contains("$1"));
        assert!(sql.contains("$2"));
        assert!(sql.contains("$3"));
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_sqlite_all_question_marks() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::And(vec![
            WhereNode::Condition {
                column: "a".to_string(),
                lookup: Lookup::Exact(Value::from(1)),
            },
            WhereNode::Condition {
                column: "b".to_string(),
                lookup: Lookup::Exact(Value::from(2)),
            },
        ]));
        let (sql, _) = sqlite().compile_select(&query);
        // Should contain ? and not $1, $2
        assert!(!sql.contains('$'));
        assert!(sql.contains('?'));
    }

    // ── SelectColumn types ───────────────────────────────────────────

    #[test]
    fn test_select_table_column() {
        let mut query = Query::new("posts");
        query.select = vec![SelectColumn::TableColumn(
            "posts".to_string(),
            "title".to_string(),
        )];
        let (sql, _) = pg().compile_select(&query);
        assert_eq!(sql, "SELECT \"posts\".\"title\" FROM \"posts\"");
    }

    #[test]
    fn test_select_expression_column() {
        let mut query = Query::new("orders");
        query.select = vec![SelectColumn::Expression(
            Expression::aggregate(AggregateFunc::Count, Expression::col("id")),
            "total".to_string(),
        )];
        let (sql, _) = pg().compile_select(&query);
        assert_eq!(
            sql,
            "SELECT COUNT(\"id\") AS \"total\" FROM \"orders\""
        );
    }

    // ── Case expression compilation ──────────────────────────────────

    #[test]
    fn test_compile_case_expression() {
        use super::super::lookups::Lookup;
        let compiler = pg();
        let mut params = Vec::new();
        let expr = Expression::case(
            vec![super::super::expressions::When {
                condition: Q::filter("status", Lookup::Exact(Value::from("active"))),
                then: Expression::value(1),
            }],
            Some(Expression::value(0)),
        );
        let sql = compiler.compile_expression(&expr, &mut params);
        assert!(sql.starts_with("CASE WHEN"));
        assert!(sql.contains("THEN"));
        assert!(sql.contains("ELSE"));
        assert!(sql.ends_with("END"));
    }

    // ── IStartsWith / IEndsWith backend differences ──────────────────

    #[test]
    fn test_istartswith_pg() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "name".to_string(),
            lookup: Lookup::IStartsWith("al".to_string()),
        });
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("ILIKE"));
    }

    #[test]
    fn test_istartswith_sqlite() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "name".to_string(),
            lookup: Lookup::IStartsWith("al".to_string()),
        });
        let (sql, _) = sqlite().compile_select(&query);
        assert!(sql.contains("LOWER"));
    }

    #[test]
    fn test_iendswith_pg() {
        let mut query = Query::new("users");
        query.where_clause = Some(WhereNode::Condition {
            column: "name".to_string(),
            lookup: Lookup::IEndsWith("son".to_string()),
        });
        let (sql, _) = pg().compile_select(&query);
        assert!(sql.contains("ILIKE"));
    }
}
