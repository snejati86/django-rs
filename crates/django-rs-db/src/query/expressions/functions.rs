//! Database functions for comparison, text, math, date/time, and type conversion.
//!
//! This module provides builder functions that create [`Expression`] values for
//! the most commonly used SQL functions, mirroring Django's `django.db.models.functions`.
//!
//! Functions are organized into categories:
//! - **Comparison**: Coalesce, Greatest, Least, NullIf
//! - **Text**: Concat, Left, Right, Length, Lower, Upper, Trim, Replace, Reverse, Substr, etc.
//! - **Math**: Abs, Ceil, Floor, Round, Sqrt, Power, Mod, Log, Ln, Exp, trig functions, etc.
//! - **Date/Time**: Now, Extract, Trunc, TruncDate, TruncTime
//! - **Type Conversion**: Cast, Collate
//!
//! All functions return [`Expression`] values that can be used in annotations, filters,
//! and ordering.
//!
//! # Examples
//!
//! ```
//! use django_rs_db::query::expressions::functions::*;
//! use django_rs_db::query::expressions::core::Expression;
//!
//! // COALESCE(nickname, name, 'Anonymous')
//! let expr = coalesce(vec![
//!     Expression::col("nickname"),
//!     Expression::col("name"),
//!     Expression::value("Anonymous"),
//! ]);
//!
//! // UPPER(name)
//! let upper = upper(Expression::col("name"));
//!
//! // ROUND(price, 2)
//! let rounded = round(Expression::col("price"), Some(2));
//! ```

use super::core::Expression;

// ═══════════════════════════════════════════════════════════════════════════
// Comparison Functions
// ═══════════════════════════════════════════════════════════════════════════

/// COALESCE(expr1, expr2, ...) - returns the first non-NULL argument.
pub fn coalesce(args: Vec<Expression>) -> Expression {
    Expression::Func {
        name: "COALESCE".to_string(),
        args,
    }
}

/// GREATEST(expr1, expr2, ...) - returns the largest argument.
pub fn greatest(args: Vec<Expression>) -> Expression {
    Expression::Func {
        name: "GREATEST".to_string(),
        args,
    }
}

/// LEAST(expr1, expr2, ...) - returns the smallest argument.
pub fn least(args: Vec<Expression>) -> Expression {
    Expression::Func {
        name: "LEAST".to_string(),
        args,
    }
}

/// NULLIF(expr1, expr2) - returns NULL if expr1 equals expr2, otherwise expr1.
pub fn nullif(expr1: Expression, expr2: Expression) -> Expression {
    Expression::Func {
        name: "NULLIF".to_string(),
        args: vec![expr1, expr2],
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Text Functions
// ═══════════════════════════════════════════════════════════════════════════

/// CONCAT(expr1, expr2, ...) - concatenates strings.
pub fn concat(args: Vec<Expression>) -> Expression {
    Expression::Func {
        name: "CONCAT".to_string(),
        args,
    }
}

/// CONCAT(expr1, expr2) - concatenates exactly two expressions (Django's ConcatPair).
pub fn concat_pair(left: Expression, right: Expression) -> Expression {
    Expression::Func {
        name: "CONCAT".to_string(),
        args: vec![left, right],
    }
}

/// LEFT(str, n) - returns the leftmost n characters.
pub fn left(expr: Expression, n: Expression) -> Expression {
    Expression::Func {
        name: "LEFT".to_string(),
        args: vec![expr, n],
    }
}

/// RIGHT(str, n) - returns the rightmost n characters.
pub fn right(expr: Expression, n: Expression) -> Expression {
    Expression::Func {
        name: "RIGHT".to_string(),
        args: vec![expr, n],
    }
}

/// LENGTH(str) / CHAR_LENGTH(str) - returns the length of a string.
pub fn length(expr: Expression) -> Expression {
    Expression::Func {
        name: "LENGTH".to_string(),
        args: vec![expr],
    }
}

/// LOWER(str) - converts to lowercase.
pub fn lower(expr: Expression) -> Expression {
    Expression::Func {
        name: "LOWER".to_string(),
        args: vec![expr],
    }
}

/// UPPER(str) - converts to uppercase.
pub fn upper(expr: Expression) -> Expression {
    Expression::Func {
        name: "UPPER".to_string(),
        args: vec![expr],
    }
}

/// TRIM(str) - removes leading and trailing whitespace.
pub fn trim(expr: Expression) -> Expression {
    Expression::Func {
        name: "TRIM".to_string(),
        args: vec![expr],
    }
}

/// LTRIM(str) - removes leading whitespace.
pub fn ltrim(expr: Expression) -> Expression {
    Expression::Func {
        name: "LTRIM".to_string(),
        args: vec![expr],
    }
}

/// RTRIM(str) - removes trailing whitespace.
pub fn rtrim(expr: Expression) -> Expression {
    Expression::Func {
        name: "RTRIM".to_string(),
        args: vec![expr],
    }
}

/// REPLACE(str, from, to) - replaces all occurrences of `from` with `to`.
pub fn replace(expr: Expression, from: Expression, to: Expression) -> Expression {
    Expression::Func {
        name: "REPLACE".to_string(),
        args: vec![expr, from, to],
    }
}

/// REVERSE(str) - reverses a string.
pub fn reverse(expr: Expression) -> Expression {
    Expression::Func {
        name: "REVERSE".to_string(),
        args: vec![expr],
    }
}

/// SUBSTR(str, pos) or SUBSTR(str, pos, len) - extracts a substring.
pub fn substr(expr: Expression, pos: Expression, len: Option<Expression>) -> Expression {
    let mut args = vec![expr, pos];
    if let Some(length) = len {
        args.push(length);
    }
    Expression::Func {
        name: "SUBSTR".to_string(),
        args,
    }
}

/// STRPOS(str, substr) / POSITION(substr IN str) / INSTR(str, substr)
/// - returns position of substring.
pub fn str_index(expr: Expression, search: Expression) -> Expression {
    Expression::Func {
        name: "STRPOS".to_string(),
        args: vec![expr, search],
    }
}

/// REPEAT(str, n) - repeats a string n times.
pub fn repeat(expr: Expression, n: Expression) -> Expression {
    Expression::Func {
        name: "REPEAT".to_string(),
        args: vec![expr, n],
    }
}

/// LPAD(str, len, fill) - left-pads a string.
pub fn lpad(expr: Expression, len: Expression, fill: Expression) -> Expression {
    Expression::Func {
        name: "LPAD".to_string(),
        args: vec![expr, len, fill],
    }
}

/// RPAD(str, len, fill) - right-pads a string.
pub fn rpad(expr: Expression, len: Expression, fill: Expression) -> Expression {
    Expression::Func {
        name: "RPAD".to_string(),
        args: vec![expr, len, fill],
    }
}

/// CHR(n) - returns the character for the given Unicode code point.
pub fn chr(expr: Expression) -> Expression {
    Expression::Func {
        name: "CHR".to_string(),
        args: vec![expr],
    }
}

/// ORD(str) / ASCII(str) - returns the Unicode code point of the first character.
pub fn ord(expr: Expression) -> Expression {
    Expression::Func {
        name: "ASCII".to_string(),
        args: vec![expr],
    }
}

/// MD5(str) - returns the MD5 hash as a hex string.
pub fn md5(expr: Expression) -> Expression {
    Expression::Func {
        name: "MD5".to_string(),
        args: vec![expr],
    }
}

/// SHA1(str) - alias SHA1 hash function.
pub fn sha1(expr: Expression) -> Expression {
    Expression::Func {
        name: "SHA1".to_string(),
        args: vec![expr],
    }
}

/// SHA224(str) - SHA-224 hash.
pub fn sha224(expr: Expression) -> Expression {
    Expression::Func {
        name: "SHA224".to_string(),
        args: vec![expr],
    }
}

/// SHA256(str) - SHA-256 hash.
pub fn sha256(expr: Expression) -> Expression {
    Expression::Func {
        name: "SHA256".to_string(),
        args: vec![expr],
    }
}

/// SHA384(str) - SHA-384 hash.
pub fn sha384(expr: Expression) -> Expression {
    Expression::Func {
        name: "SHA384".to_string(),
        args: vec![expr],
    }
}

/// SHA512(str) - SHA-512 hash.
pub fn sha512(expr: Expression) -> Expression {
    Expression::Func {
        name: "SHA512".to_string(),
        args: vec![expr],
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Math Functions
// ═══════════════════════════════════════════════════════════════════════════

/// ABS(expr) - absolute value.
pub fn abs(expr: Expression) -> Expression {
    Expression::Func {
        name: "ABS".to_string(),
        args: vec![expr],
    }
}

/// CEIL(expr) / CEILING(expr) - rounds up to the nearest integer.
pub fn ceil(expr: Expression) -> Expression {
    Expression::Func {
        name: "CEIL".to_string(),
        args: vec![expr],
    }
}

/// FLOOR(expr) - rounds down to the nearest integer.
pub fn floor(expr: Expression) -> Expression {
    Expression::Func {
        name: "FLOOR".to_string(),
        args: vec![expr],
    }
}

/// ROUND(expr) or ROUND(expr, digits) - rounds to the nearest value.
pub fn round(expr: Expression, digits: Option<i32>) -> Expression {
    let mut args = vec![expr];
    if let Some(d) = digits {
        args.push(Expression::value(d));
    }
    Expression::Func {
        name: "ROUND".to_string(),
        args,
    }
}

/// SQRT(expr) - square root.
pub fn sqrt(expr: Expression) -> Expression {
    Expression::Func {
        name: "SQRT".to_string(),
        args: vec![expr],
    }
}

/// POWER(base, exp) - raises base to the power of exp.
pub fn power(base: Expression, exp: Expression) -> Expression {
    Expression::Func {
        name: "POWER".to_string(),
        args: vec![base, exp],
    }
}

/// MOD(dividend, divisor) - modulo operation.
pub fn modulo(dividend: Expression, divisor: Expression) -> Expression {
    Expression::Func {
        name: "MOD".to_string(),
        args: vec![dividend, divisor],
    }
}

/// SIGN(expr) - returns -1, 0, or 1 depending on the sign.
pub fn sign(expr: Expression) -> Expression {
    Expression::Func {
        name: "SIGN".to_string(),
        args: vec![expr],
    }
}

/// LOG(base, value) - logarithm with specified base.
pub fn log(base: Expression, value: Expression) -> Expression {
    Expression::Func {
        name: "LOG".to_string(),
        args: vec![base, value],
    }
}

/// LN(expr) - natural logarithm.
pub fn ln(expr: Expression) -> Expression {
    Expression::Func {
        name: "LN".to_string(),
        args: vec![expr],
    }
}

/// EXP(expr) - e raised to the power of expr.
pub fn exp(expr: Expression) -> Expression {
    Expression::Func {
        name: "EXP".to_string(),
        args: vec![expr],
    }
}

/// DEGREES(expr) - converts radians to degrees.
pub fn degrees(expr: Expression) -> Expression {
    Expression::Func {
        name: "DEGREES".to_string(),
        args: vec![expr],
    }
}

/// RADIANS(expr) - converts degrees to radians.
pub fn radians(expr: Expression) -> Expression {
    Expression::Func {
        name: "RADIANS".to_string(),
        args: vec![expr],
    }
}

/// PI() - returns the value of Pi.
pub fn pi() -> Expression {
    Expression::Func {
        name: "PI".to_string(),
        args: vec![],
    }
}

/// SIN(expr) - sine of angle in radians.
pub fn sin(expr: Expression) -> Expression {
    Expression::Func {
        name: "SIN".to_string(),
        args: vec![expr],
    }
}

/// COS(expr) - cosine of angle in radians.
pub fn cos(expr: Expression) -> Expression {
    Expression::Func {
        name: "COS".to_string(),
        args: vec![expr],
    }
}

/// TAN(expr) - tangent of angle in radians.
pub fn tan(expr: Expression) -> Expression {
    Expression::Func {
        name: "TAN".to_string(),
        args: vec![expr],
    }
}

/// ASIN(expr) - arc sine.
pub fn asin(expr: Expression) -> Expression {
    Expression::Func {
        name: "ASIN".to_string(),
        args: vec![expr],
    }
}

/// ACOS(expr) - arc cosine.
pub fn acos(expr: Expression) -> Expression {
    Expression::Func {
        name: "ACOS".to_string(),
        args: vec![expr],
    }
}

/// ATAN(expr) - arc tangent.
pub fn atan(expr: Expression) -> Expression {
    Expression::Func {
        name: "ATAN".to_string(),
        args: vec![expr],
    }
}

/// ATAN2(y, x) - two-argument arc tangent.
pub fn atan2(y: Expression, x: Expression) -> Expression {
    Expression::Func {
        name: "ATAN2".to_string(),
        args: vec![y, x],
    }
}

/// COT(expr) - cotangent (1/TAN(expr)).
pub fn cot(expr: Expression) -> Expression {
    Expression::Func {
        name: "COT".to_string(),
        args: vec![expr],
    }
}

/// RANDOM() - returns a random value (backend-specific).
pub fn random() -> Expression {
    Expression::Func {
        name: "RANDOM".to_string(),
        args: vec![],
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Date/Time Functions
// ═══════════════════════════════════════════════════════════════════════════

/// NOW() / CURRENT_TIMESTAMP - returns the current date and time.
pub fn now() -> Expression {
    Expression::Func {
        name: "NOW".to_string(),
        args: vec![],
    }
}

/// The part of a date/time to extract or truncate to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateTimePart {
    /// Year.
    Year,
    /// Month.
    Month,
    /// Day.
    Day,
    /// Hour.
    Hour,
    /// Minute.
    Minute,
    /// Second.
    Second,
    /// Quarter.
    Quarter,
    /// Week.
    Week,
    /// Day of week.
    DayOfWeek,
    /// Day of year.
    DayOfYear,
    /// ISO year.
    IsoYear,
}

impl DateTimePart {
    /// Returns the SQL keyword for this part.
    pub fn sql_keyword(self) -> &'static str {
        match self {
            Self::Year => "YEAR",
            Self::Month => "MONTH",
            Self::Day => "DAY",
            Self::Hour => "HOUR",
            Self::Minute => "MINUTE",
            Self::Second => "SECOND",
            Self::Quarter => "QUARTER",
            Self::Week => "WEEK",
            Self::DayOfWeek => "DOW",
            Self::DayOfYear => "DOY",
            Self::IsoYear => "ISOYEAR",
        }
    }
}

/// EXTRACT(part FROM expr) - extracts a component of a date/time value.
pub fn extract(part: DateTimePart, expr: Expression) -> Expression {
    Expression::Extract {
        part: part.sql_keyword().to_string(),
        expr: Box::new(expr),
    }
}

/// DATE_TRUNC(precision, expr) - truncates a timestamp to the specified precision.
pub fn trunc(part: DateTimePart, expr: Expression) -> Expression {
    Expression::DateTrunc {
        precision: part.sql_keyword().to_string(),
        expr: Box::new(expr),
    }
}

/// Truncates a datetime to just the date part.
/// Equivalent to `DATE_TRUNC('day', expr)` or `CAST(expr AS DATE)`.
pub fn trunc_date(expr: Expression) -> Expression {
    Expression::DateTrunc {
        precision: "DAY".to_string(),
        expr: Box::new(expr),
    }
}

/// Truncates a datetime to just the time part.
/// Renders as `CAST(expr AS TIME)`.
pub fn trunc_time(expr: Expression) -> Expression {
    Expression::Cast {
        expr: Box::new(expr),
        data_type: "TIME".to_string(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Type Conversion Functions
// ═══════════════════════════════════════════════════════════════════════════

/// CAST(expr AS type) - converts an expression to a different data type.
pub fn cast(expr: Expression, data_type: impl Into<String>) -> Expression {
    Expression::Cast {
        expr: Box::new(expr),
        data_type: data_type.into(),
    }
}

/// expr COLLATE collation - applies a collation to an expression.
pub fn collate(expr: Expression, collation: impl Into<String>) -> Expression {
    Expression::Collate {
        expr: Box::new(expr),
        collation: collation.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::compiler::{DatabaseBackendType, Query, SqlCompiler};
    use crate::value::Value;

    fn pg() -> SqlCompiler {
        SqlCompiler::new(DatabaseBackendType::PostgreSQL)
    }

    fn sqlite() -> SqlCompiler {
        SqlCompiler::new(DatabaseBackendType::SQLite)
    }

    fn compile(expr: &Expression) -> (String, Vec<Value>) {
        let compiler = pg();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(expr, &mut params);
        (sql, params)
    }

    // ── Comparison functions ────────────────────────────────────────────

    #[test]
    fn test_coalesce() {
        let expr = coalesce(vec![
            Expression::col("nickname"),
            Expression::col("name"),
            Expression::value("Anonymous"),
        ]);
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "COALESCE(\"nickname\", \"name\", $1)");
        assert_eq!(params, vec![Value::from("Anonymous")]);
    }

    #[test]
    fn test_greatest() {
        let expr = greatest(vec![
            Expression::col("a"),
            Expression::col("b"),
            Expression::col("c"),
        ]);
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "GREATEST(\"a\", \"b\", \"c\")");
        assert!(params.is_empty());
    }

    #[test]
    fn test_least() {
        let expr = least(vec![Expression::col("x"), Expression::col("y")]);
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "LEAST(\"x\", \"y\")");
        assert!(params.is_empty());
    }

    #[test]
    fn test_nullif() {
        let expr = nullif(Expression::col("value"), Expression::value(0));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "NULLIF(\"value\", $1)");
        assert_eq!(params, vec![Value::Int(0)]);
    }

    // ── Text functions ──────────────────────────────────────────────────

    #[test]
    fn test_concat() {
        let expr = concat(vec![
            Expression::col("first_name"),
            Expression::value(" "),
            Expression::col("last_name"),
        ]);
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "CONCAT(\"first_name\", $1, \"last_name\")");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_concat_pair() {
        let expr = concat_pair(Expression::col("first"), Expression::col("last"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "CONCAT(\"first\", \"last\")");
    }

    #[test]
    fn test_left() {
        let expr = left(Expression::col("name"), Expression::value(3));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "LEFT(\"name\", $1)");
        assert_eq!(params, vec![Value::Int(3)]);
    }

    #[test]
    fn test_right() {
        let expr = right(Expression::col("name"), Expression::value(3));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "RIGHT(\"name\", $1)");
        assert_eq!(params, vec![Value::Int(3)]);
    }

    #[test]
    fn test_length() {
        let expr = length(Expression::col("name"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "LENGTH(\"name\")");
    }

    #[test]
    fn test_lower() {
        let expr = lower(Expression::col("email"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "LOWER(\"email\")");
    }

    #[test]
    fn test_upper() {
        let expr = upper(Expression::col("name"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "UPPER(\"name\")");
    }

    #[test]
    fn test_trim() {
        let expr = trim(Expression::col("data"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "TRIM(\"data\")");
    }

    #[test]
    fn test_ltrim() {
        let expr = ltrim(Expression::col("data"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "LTRIM(\"data\")");
    }

    #[test]
    fn test_rtrim() {
        let expr = rtrim(Expression::col("data"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "RTRIM(\"data\")");
    }

    #[test]
    fn test_replace() {
        let expr = replace(
            Expression::col("text"),
            Expression::value("old"),
            Expression::value("new"),
        );
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "REPLACE(\"text\", $1, $2)");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_reverse() {
        let expr = reverse(Expression::col("name"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "REVERSE(\"name\")");
    }

    #[test]
    fn test_substr_without_length() {
        let expr = substr(Expression::col("name"), Expression::value(2), None);
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "SUBSTR(\"name\", $1)");
        assert_eq!(params, vec![Value::Int(2)]);
    }

    #[test]
    fn test_substr_with_length() {
        let expr = substr(
            Expression::col("name"),
            Expression::value(1),
            Some(Expression::value(5)),
        );
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "SUBSTR(\"name\", $1, $2)");
        assert_eq!(params, vec![Value::Int(1), Value::Int(5)]);
    }

    #[test]
    fn test_str_index() {
        let expr = str_index(Expression::col("text"), Expression::value("needle"));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "STRPOS(\"text\", $1)");
        assert_eq!(params, vec![Value::from("needle")]);
    }

    #[test]
    fn test_repeat() {
        let expr = repeat(Expression::value("-"), Expression::value(10));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "REPEAT($1, $2)");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_lpad() {
        let expr = lpad(
            Expression::col("code"),
            Expression::value(10),
            Expression::value("0"),
        );
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "LPAD(\"code\", $1, $2)");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_rpad() {
        let expr = rpad(
            Expression::col("name"),
            Expression::value(20),
            Expression::value(" "),
        );
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "RPAD(\"name\", $1, $2)");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_chr() {
        let expr = chr(Expression::value(65));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "CHR($1)");
        assert_eq!(params, vec![Value::Int(65)]);
    }

    #[test]
    fn test_ord() {
        let expr = ord(Expression::value("A"));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "ASCII($1)");
        assert_eq!(params, vec![Value::from("A")]);
    }

    #[test]
    fn test_md5() {
        let expr = md5(Expression::col("password"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "MD5(\"password\")");
    }

    #[test]
    fn test_sha1() {
        let expr = sha1(Expression::col("data"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "SHA1(\"data\")");
    }

    #[test]
    fn test_sha256() {
        let expr = sha256(Expression::col("data"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "SHA256(\"data\")");
    }

    #[test]
    fn test_sha224() {
        let expr = sha224(Expression::col("data"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "SHA224(\"data\")");
    }

    #[test]
    fn test_sha384() {
        let expr = sha384(Expression::col("data"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "SHA384(\"data\")");
    }

    #[test]
    fn test_sha512() {
        let expr = sha512(Expression::col("data"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "SHA512(\"data\")");
    }

    // ── Math functions ──────────────────────────────────────────────────

    #[test]
    fn test_abs() {
        let expr = abs(Expression::col("balance"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "ABS(\"balance\")");
    }

    #[test]
    fn test_ceil() {
        let expr = ceil(Expression::col("price"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "CEIL(\"price\")");
    }

    #[test]
    fn test_floor() {
        let expr = floor(Expression::col("price"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "FLOOR(\"price\")");
    }

    #[test]
    fn test_round_without_digits() {
        let expr = round(Expression::col("price"), None);
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "ROUND(\"price\")");
    }

    #[test]
    fn test_round_with_digits() {
        let expr = round(Expression::col("price"), Some(2));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "ROUND(\"price\", $1)");
        assert_eq!(params, vec![Value::Int(2)]);
    }

    #[test]
    fn test_sqrt() {
        let expr = sqrt(Expression::col("variance"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "SQRT(\"variance\")");
    }

    #[test]
    fn test_power() {
        let expr = power(Expression::col("base"), Expression::value(3));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "POWER(\"base\", $1)");
        assert_eq!(params, vec![Value::Int(3)]);
    }

    #[test]
    fn test_modulo() {
        let expr = modulo(Expression::col("num"), Expression::value(7));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "MOD(\"num\", $1)");
        assert_eq!(params, vec![Value::Int(7)]);
    }

    #[test]
    fn test_sign() {
        let expr = sign(Expression::col("delta"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "SIGN(\"delta\")");
    }

    #[test]
    fn test_log() {
        let expr = log(Expression::value(10), Expression::col("value"));
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "LOG($1, \"value\")");
        assert_eq!(params, vec![Value::Int(10)]);
    }

    #[test]
    fn test_ln() {
        let expr = ln(Expression::col("value"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "LN(\"value\")");
    }

    #[test]
    fn test_exp() {
        let expr = exp(Expression::col("rate"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXP(\"rate\")");
    }

    #[test]
    fn test_degrees() {
        let expr = degrees(Expression::col("radians_col"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "DEGREES(\"radians_col\")");
    }

    #[test]
    fn test_radians() {
        let expr = radians(Expression::col("degrees_col"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "RADIANS(\"degrees_col\")");
    }

    #[test]
    fn test_pi() {
        let expr = pi();
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "PI()");
        assert!(params.is_empty());
    }

    #[test]
    fn test_sin() {
        let expr = sin(Expression::col("angle"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "SIN(\"angle\")");
    }

    #[test]
    fn test_cos() {
        let expr = cos(Expression::col("angle"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "COS(\"angle\")");
    }

    #[test]
    fn test_tan() {
        let expr = tan(Expression::col("angle"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "TAN(\"angle\")");
    }

    #[test]
    fn test_asin() {
        let expr = asin(Expression::col("value"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "ASIN(\"value\")");
    }

    #[test]
    fn test_acos() {
        let expr = acos(Expression::col("value"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "ACOS(\"value\")");
    }

    #[test]
    fn test_atan() {
        let expr = atan(Expression::col("value"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "ATAN(\"value\")");
    }

    #[test]
    fn test_atan2() {
        let expr = atan2(Expression::col("y"), Expression::col("x"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "ATAN2(\"y\", \"x\")");
    }

    #[test]
    fn test_cot() {
        let expr = cot(Expression::col("angle"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "COT(\"angle\")");
    }

    #[test]
    fn test_random() {
        let expr = random();
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "RANDOM()");
        assert!(params.is_empty());
    }

    // ── Date/Time functions ─────────────────────────────────────────────

    #[test]
    fn test_now() {
        let expr = now();
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "NOW()");
    }

    #[test]
    fn test_extract_year() {
        let expr = extract(DateTimePart::Year, Expression::col("created_at"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(YEAR FROM \"created_at\")");
    }

    #[test]
    fn test_extract_month() {
        let expr = extract(DateTimePart::Month, Expression::col("created_at"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(MONTH FROM \"created_at\")");
    }

    #[test]
    fn test_extract_day() {
        let expr = extract(DateTimePart::Day, Expression::col("created_at"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(DAY FROM \"created_at\")");
    }

    #[test]
    fn test_extract_hour() {
        let expr = extract(DateTimePart::Hour, Expression::col("event_time"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(HOUR FROM \"event_time\")");
    }

    #[test]
    fn test_extract_minute() {
        let expr = extract(DateTimePart::Minute, Expression::col("event_time"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(MINUTE FROM \"event_time\")");
    }

    #[test]
    fn test_extract_second() {
        let expr = extract(DateTimePart::Second, Expression::col("event_time"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(SECOND FROM \"event_time\")");
    }

    #[test]
    fn test_extract_quarter() {
        let expr = extract(DateTimePart::Quarter, Expression::col("sale_date"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(QUARTER FROM \"sale_date\")");
    }

    #[test]
    fn test_extract_week() {
        let expr = extract(DateTimePart::Week, Expression::col("date"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(WEEK FROM \"date\")");
    }

    #[test]
    fn test_extract_day_of_week() {
        let expr = extract(DateTimePart::DayOfWeek, Expression::col("date"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(DOW FROM \"date\")");
    }

    #[test]
    fn test_extract_day_of_year() {
        let expr = extract(DateTimePart::DayOfYear, Expression::col("date"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(DOY FROM \"date\")");
    }

    #[test]
    fn test_extract_iso_year() {
        let expr = extract(DateTimePart::IsoYear, Expression::col("date"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "EXTRACT(ISOYEAR FROM \"date\")");
    }

    #[test]
    fn test_trunc_year() {
        let expr = trunc(DateTimePart::Year, Expression::col("created_at"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "DATE_TRUNC('YEAR', \"created_at\")");
    }

    #[test]
    fn test_trunc_month() {
        let expr = trunc(DateTimePart::Month, Expression::col("created_at"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "DATE_TRUNC('MONTH', \"created_at\")");
    }

    #[test]
    fn test_trunc_day() {
        let expr = trunc(DateTimePart::Day, Expression::col("created_at"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "DATE_TRUNC('DAY', \"created_at\")");
    }

    #[test]
    fn test_trunc_hour() {
        let expr = trunc(DateTimePart::Hour, Expression::col("event_time"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "DATE_TRUNC('HOUR', \"event_time\")");
    }

    #[test]
    fn test_trunc_date() {
        let expr = trunc_date(Expression::col("timestamp_col"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "DATE_TRUNC('DAY', \"timestamp_col\")");
    }

    #[test]
    fn test_trunc_time() {
        let expr = trunc_time(Expression::col("timestamp_col"));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "CAST(\"timestamp_col\" AS TIME)");
    }

    // ── Type conversion functions ───────────────────────────────────────

    #[test]
    fn test_cast_integer() {
        let expr = cast(Expression::col("price"), "INTEGER");
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "CAST(\"price\" AS INTEGER)");
    }

    #[test]
    fn test_cast_varchar() {
        let expr = cast(Expression::col("id"), "VARCHAR(255)");
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "CAST(\"id\" AS VARCHAR(255))");
    }

    #[test]
    fn test_cast_date() {
        let expr = cast(Expression::col("created_at"), "DATE");
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "CAST(\"created_at\" AS DATE)");
    }

    #[test]
    fn test_cast_boolean() {
        let expr = cast(Expression::value(1), "BOOLEAN");
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "CAST($1 AS BOOLEAN)");
        assert_eq!(params, vec![Value::Int(1)]);
    }

    #[test]
    fn test_collate() {
        let expr = collate(Expression::col("name"), "utf8_general_ci");
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "\"name\" COLLATE \"utf8_general_ci\"");
    }

    #[test]
    fn test_collate_in_order() {
        let expr = collate(Expression::col("title"), "C");
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "\"title\" COLLATE \"C\"");
    }

    // ── Combined function usage ─────────────────────────────────────────

    #[test]
    fn test_coalesce_with_arithmetic() {
        let expr = coalesce(vec![Expression::col("discount"), Expression::value(0)])
            + Expression::col("tax");
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "(COALESCE(\"discount\", $1) + \"tax\")");
        assert_eq!(params, vec![Value::Int(0)]);
    }

    #[test]
    fn test_nested_functions() {
        // UPPER(TRIM(name))
        let expr = upper(trim(Expression::col("name")));
        let (sql, _) = compile(&expr);
        assert_eq!(sql, "UPPER(TRIM(\"name\"))");
    }

    #[test]
    fn test_function_in_annotation() {
        let mut query = Query::new("users");
        query
            .annotations
            .insert("name_upper".to_string(), upper(Expression::col("name")));
        let compiler = pg();
        let (sql, _) = compiler.compile_select(&query);
        assert!(sql.contains("UPPER(\"name\") AS \"name_upper\""));
    }

    #[test]
    fn test_extract_in_annotation() {
        let mut query = Query::new("events");
        query.annotations.insert(
            "event_year".to_string(),
            extract(DateTimePart::Year, Expression::col("created_at")),
        );
        let compiler = pg();
        let (sql, _) = compiler.compile_select(&query);
        assert!(sql.contains("EXTRACT(YEAR FROM \"created_at\") AS \"event_year\""));
    }

    #[test]
    fn test_datetime_part_sql_keywords() {
        assert_eq!(DateTimePart::Year.sql_keyword(), "YEAR");
        assert_eq!(DateTimePart::Month.sql_keyword(), "MONTH");
        assert_eq!(DateTimePart::Day.sql_keyword(), "DAY");
        assert_eq!(DateTimePart::Hour.sql_keyword(), "HOUR");
        assert_eq!(DateTimePart::Minute.sql_keyword(), "MINUTE");
        assert_eq!(DateTimePart::Second.sql_keyword(), "SECOND");
        assert_eq!(DateTimePart::Quarter.sql_keyword(), "QUARTER");
        assert_eq!(DateTimePart::Week.sql_keyword(), "WEEK");
        assert_eq!(DateTimePart::DayOfWeek.sql_keyword(), "DOW");
        assert_eq!(DateTimePart::DayOfYear.sql_keyword(), "DOY");
        assert_eq!(DateTimePart::IsoYear.sql_keyword(), "ISOYEAR");
    }

    #[test]
    fn test_round_with_coalesce() {
        // ROUND(COALESCE(price, 0), 2)
        let expr = round(
            coalesce(vec![Expression::col("price"), Expression::value(0)]),
            Some(2),
        );
        let (sql, params) = compile(&expr);
        assert_eq!(sql, "ROUND(COALESCE(\"price\", $1), $2)");
        assert_eq!(params, vec![Value::Int(0), Value::Int(2)]);
    }

    #[test]
    fn test_sqlite_backend_functions() {
        let expr = upper(Expression::col("name"));
        let compiler = sqlite();
        let mut params = Vec::new();
        let sql = compiler.compile_expression(&expr, &mut params);
        assert_eq!(sql, "UPPER(\"name\")");
    }
}
