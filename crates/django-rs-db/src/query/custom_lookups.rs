//! Custom lookup and transform registry.
//!
//! This module provides the ability to register custom field lookups and
//! transforms that extend the built-in lookup system. This mirrors Django's
//! custom lookup/transform API.
//!
//! # Architecture
//!
//! - **Lookups** produce a boolean SQL expression (e.g., `field @> value`).
//! - **Transforms** modify the field reference before applying a lookup
//!   (e.g., `LOWER(field)`, `EXTRACT(year FROM field)`).
//! - Transforms can be chained: `field__lower__contains` applies `LOWER()`
//!   then the `CONTAINS` lookup.
//!
//! Custom lookups and transforms are registered in a global [`LookupRegistry`]
//! and referenced by name in filter expressions.
//!
//! # Examples
//!
//! ```
//! use django_rs_db::query::custom_lookups::*;
//! use django_rs_db::query::compiler::DatabaseBackendType;
//!
//! // Register a custom lookup
//! let mut registry = LookupRegistry::new();
//! registry.register_lookup("ne", CustomLookup {
//!     name: "ne".to_string(),
//!     sql_template: "{column} != {value}".to_string(),
//! });
//!
//! // Register a transform
//! registry.register_transform("year", Transform {
//!     name: "year".to_string(),
//!     sql_template_pg: "EXTRACT(YEAR FROM {column})".to_string(),
//!     sql_template_sqlite: "strftime('%Y', {column})".to_string(),
//!     sql_template_mysql: "YEAR({column})".to_string(),
//!     output_type: TransformOutput::Integer,
//! });
//! ```

use crate::query::compiler::DatabaseBackendType;
use crate::value::Value;
use std::collections::HashMap;

/// A custom lookup that produces a boolean SQL expression.
///
/// The `sql_template` uses `{column}` and `{value}` as placeholders.
/// For example: `"{column} @> {value}"` or `"LOWER({column}) = LOWER({value})"`.
#[derive(Debug, Clone)]
pub struct CustomLookup {
    /// The name of this lookup (e.g., "ne", "array_contains").
    pub name: String,
    /// The SQL template with `{column}` and `{value}` placeholders.
    /// The `{value}` placeholder will be replaced with a parameter placeholder
    /// appropriate for the backend ($1, ?, etc.).
    pub sql_template: String,
}

impl CustomLookup {
    /// Creates a new custom lookup.
    pub fn new(name: impl Into<String>, sql_template: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sql_template: sql_template.into(),
        }
    }

    /// Compiles this lookup to SQL for the given column and parameter placeholder.
    pub fn compile(&self, column: &str, placeholder: &str) -> String {
        self.sql_template
            .replace("{column}", &format!("\"{column}\""))
            .replace("{value}", placeholder)
    }
}

/// The output type of a transform, used to determine which lookups are
/// valid after the transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformOutput {
    /// The transform produces a string value.
    String,
    /// The transform produces an integer value.
    Integer,
    /// The transform produces a float value.
    Float,
    /// The transform produces a date value.
    Date,
    /// The transform produces a time value.
    Time,
    /// The transform produces a boolean value.
    Boolean,
    /// The transform output type matches the input type.
    SameAsInput,
}

/// A transform that modifies a column reference before a lookup is applied.
///
/// Transforms wrap the column in a SQL function. For example, `LOWER({column})`
/// or `EXTRACT(YEAR FROM {column})`. They can be chained.
///
/// Since different backends may require different SQL syntax for the same
/// transform, each backend has its own template.
#[derive(Debug, Clone)]
pub struct Transform {
    /// The name of this transform (e.g., "lower", "year", "day_of_week").
    pub name: String,
    /// SQL template for PostgreSQL.
    pub sql_template_pg: String,
    /// SQL template for SQLite.
    pub sql_template_sqlite: String,
    /// SQL template for MySQL.
    pub sql_template_mysql: String,
    /// The output type of this transform.
    pub output_type: TransformOutput,
}

impl Transform {
    /// Creates a new transform with the same SQL across all backends.
    pub fn new(
        name: impl Into<String>,
        sql_template: impl Into<String>,
        output_type: TransformOutput,
    ) -> Self {
        let template = sql_template.into();
        Self {
            name: name.into(),
            sql_template_pg: template.clone(),
            sql_template_sqlite: template.clone(),
            sql_template_mysql: template,
            output_type,
        }
    }

    /// Creates a new transform with per-backend SQL templates.
    pub fn with_backends(
        name: impl Into<String>,
        pg: impl Into<String>,
        sqlite: impl Into<String>,
        mysql: impl Into<String>,
        output_type: TransformOutput,
    ) -> Self {
        Self {
            name: name.into(),
            sql_template_pg: pg.into(),
            sql_template_sqlite: sqlite.into(),
            sql_template_mysql: mysql.into(),
            output_type,
        }
    }

    /// Returns the SQL template for the given backend.
    pub fn sql_template(&self, backend: DatabaseBackendType) -> &str {
        match backend {
            DatabaseBackendType::PostgreSQL => &self.sql_template_pg,
            DatabaseBackendType::SQLite => &self.sql_template_sqlite,
            DatabaseBackendType::MySQL => &self.sql_template_mysql,
        }
    }

    /// Applies this transform to a column expression.
    pub fn apply(&self, column_sql: &str, backend: DatabaseBackendType) -> String {
        self.sql_template(backend).replace("{column}", column_sql)
    }
}

/// A registry of custom lookups and transforms.
///
/// The registry is a `HashMap`-based store where lookups and transforms
/// are registered by name. When resolving a filter expression like
/// `field__lower__contains`, the system:
///
/// 1. Splits on `__` to get segments: `["field", "lower", "contains"]`
/// 2. Looks up each segment (after the field name) as a transform or lookup
/// 3. Applies transforms in order, then the final lookup
#[derive(Debug, Clone)]
pub struct LookupRegistry {
    /// Registered custom lookups, keyed by name.
    lookups: HashMap<String, CustomLookup>,
    /// Registered transforms, keyed by name.
    transforms: HashMap<String, Transform>,
}

impl Default for LookupRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LookupRegistry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            lookups: HashMap::new(),
            transforms: HashMap::new(),
        }
    }

    /// Creates a new registry pre-populated with standard transforms.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Lower transform
        registry.register_transform(
            "lower",
            Transform::new("lower", "LOWER({column})", TransformOutput::String),
        );

        // Upper transform
        registry.register_transform(
            "upper",
            Transform::new("upper", "UPPER({column})", TransformOutput::String),
        );

        // Length transform
        registry.register_transform(
            "length",
            Transform::with_backends(
                "length",
                "LENGTH({column})",
                "LENGTH({column})",
                "CHAR_LENGTH({column})",
                TransformOutput::Integer,
            ),
        );

        // Trim transform
        registry.register_transform(
            "trim",
            Transform::new("trim", "TRIM({column})", TransformOutput::String),
        );

        // Year transform
        registry.register_transform(
            "year",
            Transform::with_backends(
                "year",
                "EXTRACT(YEAR FROM {column})",
                "CAST(strftime('%Y', {column}) AS INTEGER)",
                "YEAR({column})",
                TransformOutput::Integer,
            ),
        );

        // Month transform
        registry.register_transform(
            "month",
            Transform::with_backends(
                "month",
                "EXTRACT(MONTH FROM {column})",
                "CAST(strftime('%m', {column}) AS INTEGER)",
                "MONTH({column})",
                TransformOutput::Integer,
            ),
        );

        // Day transform
        registry.register_transform(
            "day",
            Transform::with_backends(
                "day",
                "EXTRACT(DAY FROM {column})",
                "CAST(strftime('%d', {column}) AS INTEGER)",
                "DAY({column})",
                TransformOutput::Integer,
            ),
        );

        // Hour transform
        registry.register_transform(
            "hour",
            Transform::with_backends(
                "hour",
                "EXTRACT(HOUR FROM {column})",
                "CAST(strftime('%H', {column}) AS INTEGER)",
                "HOUR({column})",
                TransformOutput::Integer,
            ),
        );

        // Minute transform
        registry.register_transform(
            "minute",
            Transform::with_backends(
                "minute",
                "EXTRACT(MINUTE FROM {column})",
                "CAST(strftime('%M', {column}) AS INTEGER)",
                "MINUTE({column})",
                TransformOutput::Integer,
            ),
        );

        // Second transform
        registry.register_transform(
            "second",
            Transform::with_backends(
                "second",
                "EXTRACT(SECOND FROM {column})",
                "CAST(strftime('%S', {column}) AS INTEGER)",
                "SECOND({column})",
                TransformOutput::Integer,
            ),
        );

        // Date (extract date from datetime)
        registry.register_transform(
            "date",
            Transform::with_backends(
                "date",
                "{column}::date",
                "DATE({column})",
                "DATE({column})",
                TransformOutput::Date,
            ),
        );

        // Abs transform (absolute value)
        registry.register_transform(
            "abs",
            Transform::new("abs", "ABS({column})", TransformOutput::SameAsInput),
        );

        // Custom lookup: not equal
        registry.register_lookup("ne", CustomLookup::new("ne", "{column} != {value}"));

        registry
    }

    /// Registers a custom lookup.
    pub fn register_lookup(&mut self, name: impl Into<String>, lookup: CustomLookup) {
        self.lookups.insert(name.into(), lookup);
    }

    /// Registers a transform.
    pub fn register_transform(&mut self, name: impl Into<String>, transform: Transform) {
        self.transforms.insert(name.into(), transform);
    }

    /// Unregisters a lookup by name.
    pub fn unregister_lookup(&mut self, name: &str) -> Option<CustomLookup> {
        self.lookups.remove(name)
    }

    /// Unregisters a transform by name.
    pub fn unregister_transform(&mut self, name: &str) -> Option<Transform> {
        self.transforms.remove(name)
    }

    /// Returns a reference to a registered lookup.
    pub fn get_lookup(&self, name: &str) -> Option<&CustomLookup> {
        self.lookups.get(name)
    }

    /// Returns a reference to a registered transform.
    pub fn get_transform(&self, name: &str) -> Option<&Transform> {
        self.transforms.get(name)
    }

    /// Returns true if a lookup with the given name is registered.
    pub fn has_lookup(&self, name: &str) -> bool {
        self.lookups.contains_key(name)
    }

    /// Returns true if a transform with the given name is registered.
    pub fn has_transform(&self, name: &str) -> bool {
        self.transforms.contains_key(name)
    }

    /// Returns the number of registered lookups.
    pub fn lookup_count(&self) -> usize {
        self.lookups.len()
    }

    /// Returns the number of registered transforms.
    pub fn transform_count(&self) -> usize {
        self.transforms.len()
    }

    /// Returns all registered lookup names.
    pub fn lookup_names(&self) -> Vec<&str> {
        self.lookups.keys().map(String::as_str).collect()
    }

    /// Returns all registered transform names.
    pub fn transform_names(&self) -> Vec<&str> {
        self.transforms.keys().map(String::as_str).collect()
    }

    /// Resolves a chain of transforms and a final lookup from a list of
    /// path segments (the parts after the field name split by `__`).
    ///
    /// For example, given segments `["lower", "contains"]`:
    /// - `lower` is resolved as a transform
    /// - `contains` is resolved as the final lookup name
    ///
    /// Returns `(transforms, final_lookup_name)` where `final_lookup_name`
    /// may be a built-in lookup name.
    pub fn resolve_chain<'a>(
        &'a self,
        segments: &[&'a str],
    ) -> (Vec<&'a Transform>, Option<&'a str>) {
        let mut transforms = Vec::new();
        let mut remaining = segments;

        while !remaining.is_empty() {
            let segment = remaining[0];

            if let Some(transform) = self.transforms.get(segment) {
                transforms.push(transform);
                remaining = &remaining[1..];
            } else {
                // This segment is the final lookup (or unrecognized)
                return (transforms, Some(segment));
            }
        }

        // All segments were transforms, no final lookup
        (transforms, None)
    }

    /// Compiles a chain of transforms into SQL.
    ///
    /// Starting from the base column name, applies each transform in order,
    /// producing the final column expression.
    pub fn apply_transforms(
        &self,
        column: &str,
        transforms: &[&Transform],
        backend: DatabaseBackendType,
    ) -> String {
        let mut result = format!("\"{column}\"");
        for transform in transforms {
            result = transform.apply(&result, backend);
        }
        result
    }

    /// Resolves and compiles a full lookup expression from a field path.
    ///
    /// Given a field path like `"name__lower__contains"`, this:
    /// 1. Splits on `__`
    /// 2. The first segment is the field name
    /// 3. Remaining segments are resolved as transforms + lookup
    /// 4. Returns the field name, compiled column SQL, and lookup name
    ///
    /// Returns `(field_name, column_sql, lookup_name)`.
    pub fn resolve_field_path(
        &self,
        path: &str,
        backend: DatabaseBackendType,
    ) -> (String, String, String) {
        let segments: Vec<&str> = path.split("__").collect();

        if segments.len() <= 1 {
            // Just a field name, use "exact" lookup
            let field = segments[0].to_string();
            let col_sql = format!("\"{field}\"");
            return (field, col_sql, "exact".to_string());
        }

        let field_name = segments[0].to_string();
        let rest = &segments[1..];

        let (transforms, lookup_name) = self.resolve_chain(rest);

        let col_sql = if transforms.is_empty() {
            format!("\"{field_name}\"")
        } else {
            self.apply_transforms(&field_name, &transforms, backend)
        };

        let final_lookup = lookup_name.unwrap_or("exact").to_string();
        (field_name, col_sql, final_lookup)
    }
}

/// Compiles a custom lookup to SQL.
///
/// Given a custom lookup, a column SQL expression, a parameter value,
/// and the backend type, produces the SQL fragment and appends the
/// parameter value to the params vec.
pub fn compile_custom_lookup(
    lookup: &CustomLookup,
    column_sql: &str,
    value: &Value,
    params: &mut Vec<Value>,
    backend: DatabaseBackendType,
) -> String {
    params.push(value.clone());
    let placeholder = match backend {
        DatabaseBackendType::PostgreSQL => format!("${}", params.len()),
        DatabaseBackendType::SQLite | DatabaseBackendType::MySQL => "?".to_string(),
    };

    // The column_sql may already have quotes, so use it directly
    lookup
        .sql_template
        .replace("{column}", column_sql)
        .replace("{value}", &placeholder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_lookup_new() {
        let lookup = CustomLookup::new("ne", "{column} != {value}");
        assert_eq!(lookup.name, "ne");
        assert_eq!(lookup.sql_template, "{column} != {value}");
    }

    #[test]
    fn test_custom_lookup_compile() {
        let lookup = CustomLookup::new("ne", "{column} != {value}");
        let sql = lookup.compile("name", "$1");
        assert_eq!(sql, "\"name\" != $1");
    }

    #[test]
    fn test_transform_new() {
        let t = Transform::new("lower", "LOWER({column})", TransformOutput::String);
        assert_eq!(t.name, "lower");
        assert_eq!(t.output_type, TransformOutput::String);
        assert_eq!(t.sql_template_pg, "LOWER({column})");
        assert_eq!(t.sql_template_sqlite, "LOWER({column})");
        assert_eq!(t.sql_template_mysql, "LOWER({column})");
    }

    #[test]
    fn test_transform_with_backends() {
        let t = Transform::with_backends(
            "year",
            "EXTRACT(YEAR FROM {column})",
            "strftime('%Y', {column})",
            "YEAR({column})",
            TransformOutput::Integer,
        );
        assert_eq!(t.sql_template_pg, "EXTRACT(YEAR FROM {column})");
        assert_eq!(t.sql_template_sqlite, "strftime('%Y', {column})");
        assert_eq!(t.sql_template_mysql, "YEAR({column})");
    }

    #[test]
    fn test_transform_apply() {
        let t = Transform::new("lower", "LOWER({column})", TransformOutput::String);
        assert_eq!(
            t.apply("\"name\"", DatabaseBackendType::PostgreSQL),
            "LOWER(\"name\")"
        );
    }

    #[test]
    fn test_transform_apply_backend_specific() {
        let t = Transform::with_backends(
            "year",
            "EXTRACT(YEAR FROM {column})",
            "strftime('%Y', {column})",
            "YEAR({column})",
            TransformOutput::Integer,
        );

        assert_eq!(
            t.apply("\"created_at\"", DatabaseBackendType::PostgreSQL),
            "EXTRACT(YEAR FROM \"created_at\")"
        );
        assert_eq!(
            t.apply("\"created_at\"", DatabaseBackendType::SQLite),
            "strftime('%Y', \"created_at\")"
        );
        assert_eq!(
            t.apply("\"created_at\"", DatabaseBackendType::MySQL),
            "YEAR(\"created_at\")"
        );
    }

    #[test]
    fn test_registry_new() {
        let registry = LookupRegistry::new();
        assert_eq!(registry.lookup_count(), 0);
        assert_eq!(registry.transform_count(), 0);
    }

    #[test]
    fn test_registry_default() {
        let registry = LookupRegistry::default();
        assert_eq!(registry.lookup_count(), 0);
        assert_eq!(registry.transform_count(), 0);
    }

    #[test]
    fn test_registry_with_defaults() {
        let registry = LookupRegistry::with_defaults();
        // Should have the default transforms registered
        assert!(registry.has_transform("lower"));
        assert!(registry.has_transform("upper"));
        assert!(registry.has_transform("length"));
        assert!(registry.has_transform("year"));
        assert!(registry.has_transform("month"));
        assert!(registry.has_transform("day"));
        assert!(registry.has_transform("hour"));
        assert!(registry.has_transform("minute"));
        assert!(registry.has_transform("second"));
        assert!(registry.has_transform("date"));
        assert!(registry.has_transform("abs"));
        assert!(registry.has_transform("trim"));

        // Should have the ne lookup
        assert!(registry.has_lookup("ne"));
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = LookupRegistry::new();

        registry.register_lookup(
            "array_contains",
            CustomLookup::new("array_contains", "{column} @> {value}"),
        );

        assert!(registry.has_lookup("array_contains"));
        let lookup = registry.get_lookup("array_contains").unwrap();
        assert_eq!(lookup.name, "array_contains");

        assert!(!registry.has_lookup("nonexistent"));
        assert!(registry.get_lookup("nonexistent").is_none());
    }

    #[test]
    fn test_registry_register_transform() {
        let mut registry = LookupRegistry::new();

        registry.register_transform(
            "day_of_week",
            Transform::with_backends(
                "day_of_week",
                "EXTRACT(DOW FROM {column})",
                "CAST(strftime('%w', {column}) AS INTEGER)",
                "DAYOFWEEK({column})",
                TransformOutput::Integer,
            ),
        );

        assert!(registry.has_transform("day_of_week"));
        let transform = registry.get_transform("day_of_week").unwrap();
        assert_eq!(transform.name, "day_of_week");
    }

    #[test]
    fn test_registry_unregister() {
        let mut registry = LookupRegistry::new();

        registry.register_lookup("test", CustomLookup::new("test", "{column} = {value}"));
        assert!(registry.has_lookup("test"));

        let removed = registry.unregister_lookup("test");
        assert!(removed.is_some());
        assert!(!registry.has_lookup("test"));

        registry.register_transform(
            "test_t",
            Transform::new("test_t", "LOWER({column})", TransformOutput::String),
        );
        assert!(registry.has_transform("test_t"));

        let removed_t = registry.unregister_transform("test_t");
        assert!(removed_t.is_some());
        assert!(!registry.has_transform("test_t"));
    }

    #[test]
    fn test_registry_names() {
        let mut registry = LookupRegistry::new();
        registry.register_lookup("a", CustomLookup::new("a", "{column} = {value}"));
        registry.register_lookup("b", CustomLookup::new("b", "{column} != {value}"));
        registry.register_transform(
            "x",
            Transform::new("x", "F({column})", TransformOutput::String),
        );

        let lookup_names = registry.lookup_names();
        assert_eq!(lookup_names.len(), 2);
        assert!(lookup_names.contains(&"a"));
        assert!(lookup_names.contains(&"b"));

        let transform_names = registry.transform_names();
        assert_eq!(transform_names.len(), 1);
        assert!(transform_names.contains(&"x"));
    }

    #[test]
    fn test_resolve_chain_single_lookup() {
        let registry = LookupRegistry::with_defaults();
        let (transforms, lookup) = registry.resolve_chain(&["contains"]);
        assert!(transforms.is_empty());
        assert_eq!(lookup, Some("contains"));
    }

    #[test]
    fn test_resolve_chain_transform_and_lookup() {
        let registry = LookupRegistry::with_defaults();
        let (transforms, lookup) = registry.resolve_chain(&["lower", "contains"]);
        assert_eq!(transforms.len(), 1);
        assert_eq!(transforms[0].name, "lower");
        assert_eq!(lookup, Some("contains"));
    }

    #[test]
    fn test_resolve_chain_multiple_transforms() {
        let registry = LookupRegistry::with_defaults();
        let (transforms, lookup) = registry.resolve_chain(&["trim", "lower", "exact"]);
        assert_eq!(transforms.len(), 2);
        assert_eq!(transforms[0].name, "trim");
        assert_eq!(transforms[1].name, "lower");
        assert_eq!(lookup, Some("exact"));
    }

    #[test]
    fn test_resolve_chain_all_transforms() {
        let registry = LookupRegistry::with_defaults();
        let (transforms, lookup) = registry.resolve_chain(&["lower"]);
        assert_eq!(transforms.len(), 1);
        assert!(lookup.is_none());
    }

    #[test]
    fn test_apply_transforms() {
        let registry = LookupRegistry::with_defaults();

        // Single transform
        let lower = registry.get_transform("lower").unwrap();
        let sql = registry.apply_transforms("name", &[lower], DatabaseBackendType::PostgreSQL);
        assert_eq!(sql, "LOWER(\"name\")");
    }

    #[test]
    fn test_apply_chained_transforms() {
        let registry = LookupRegistry::with_defaults();

        let trim = registry.get_transform("trim").unwrap();
        let lower = registry.get_transform("lower").unwrap();
        let sql =
            registry.apply_transforms("name", &[trim, lower], DatabaseBackendType::PostgreSQL);
        assert_eq!(sql, "LOWER(TRIM(\"name\"))");
    }

    #[test]
    fn test_resolve_field_path_simple() {
        let registry = LookupRegistry::with_defaults();

        let (field, col_sql, lookup) =
            registry.resolve_field_path("name", DatabaseBackendType::PostgreSQL);
        assert_eq!(field, "name");
        assert_eq!(col_sql, "\"name\"");
        assert_eq!(lookup, "exact");
    }

    #[test]
    fn test_resolve_field_path_with_lookup() {
        let registry = LookupRegistry::with_defaults();

        let (field, col_sql, lookup) =
            registry.resolve_field_path("name__contains", DatabaseBackendType::PostgreSQL);
        assert_eq!(field, "name");
        assert_eq!(col_sql, "\"name\"");
        assert_eq!(lookup, "contains");
    }

    #[test]
    fn test_resolve_field_path_with_transform() {
        let registry = LookupRegistry::with_defaults();

        let (field, col_sql, lookup) =
            registry.resolve_field_path("name__lower__contains", DatabaseBackendType::PostgreSQL);
        assert_eq!(field, "name");
        assert_eq!(col_sql, "LOWER(\"name\")");
        assert_eq!(lookup, "contains");
    }

    #[test]
    fn test_resolve_field_path_chained_transforms() {
        let registry = LookupRegistry::with_defaults();

        let (field, col_sql, lookup) = registry
            .resolve_field_path("name__trim__lower__exact", DatabaseBackendType::PostgreSQL);
        assert_eq!(field, "name");
        assert_eq!(col_sql, "LOWER(TRIM(\"name\"))");
        assert_eq!(lookup, "exact");
    }

    #[test]
    fn test_resolve_field_path_year_transform_pg() {
        let registry = LookupRegistry::with_defaults();

        let (field, col_sql, lookup) =
            registry.resolve_field_path("created_at__year__exact", DatabaseBackendType::PostgreSQL);
        assert_eq!(field, "created_at");
        assert_eq!(col_sql, "EXTRACT(YEAR FROM \"created_at\")");
        assert_eq!(lookup, "exact");
    }

    #[test]
    fn test_resolve_field_path_year_transform_sqlite() {
        let registry = LookupRegistry::with_defaults();

        let (field, col_sql, lookup) =
            registry.resolve_field_path("created_at__year__exact", DatabaseBackendType::SQLite);
        assert_eq!(field, "created_at");
        assert_eq!(col_sql, "CAST(strftime('%Y', \"created_at\") AS INTEGER)");
        assert_eq!(lookup, "exact");
    }

    #[test]
    fn test_compile_custom_lookup_pg() {
        let lookup = CustomLookup::new("ne", "{column} != {value}");
        let mut params = Vec::new();
        let sql = compile_custom_lookup(
            &lookup,
            "\"age\"",
            &Value::Int(18),
            &mut params,
            DatabaseBackendType::PostgreSQL,
        );
        assert_eq!(sql, "\"age\" != $1");
        assert_eq!(params, vec![Value::Int(18)]);
    }

    #[test]
    fn test_compile_custom_lookup_sqlite() {
        let lookup = CustomLookup::new("ne", "{column} != {value}");
        let mut params = Vec::new();
        let sql = compile_custom_lookup(
            &lookup,
            "\"age\"",
            &Value::Int(18),
            &mut params,
            DatabaseBackendType::SQLite,
        );
        assert_eq!(sql, "\"age\" != ?");
        assert_eq!(params, vec![Value::Int(18)]);
    }

    #[test]
    fn test_compile_custom_lookup_array_contains() {
        let lookup = CustomLookup::new("array_contains", "{column} @> {value}");
        let mut params = Vec::new();
        let sql = compile_custom_lookup(
            &lookup,
            "\"tags\"",
            &Value::String("{rust}".to_string()),
            &mut params,
            DatabaseBackendType::PostgreSQL,
        );
        assert_eq!(sql, "\"tags\" @> $1");
    }

    #[test]
    fn test_transform_output_types() {
        assert_eq!(TransformOutput::String, TransformOutput::String);
        assert_ne!(TransformOutput::String, TransformOutput::Integer);
    }

    #[test]
    fn test_transform_sql_template_method() {
        let t = Transform::with_backends(
            "test",
            "PG({column})",
            "SQLITE({column})",
            "MYSQL({column})",
            TransformOutput::String,
        );

        assert_eq!(
            t.sql_template(DatabaseBackendType::PostgreSQL),
            "PG({column})"
        );
        assert_eq!(
            t.sql_template(DatabaseBackendType::SQLite),
            "SQLITE({column})"
        );
        assert_eq!(
            t.sql_template(DatabaseBackendType::MySQL),
            "MYSQL({column})"
        );
    }

    #[test]
    fn test_registry_counts() {
        let mut registry = LookupRegistry::new();
        assert_eq!(registry.lookup_count(), 0);
        assert_eq!(registry.transform_count(), 0);

        registry.register_lookup("a", CustomLookup::new("a", ""));
        registry.register_lookup("b", CustomLookup::new("b", ""));
        assert_eq!(registry.lookup_count(), 2);

        registry.register_transform("x", Transform::new("x", "", TransformOutput::String));
        assert_eq!(registry.transform_count(), 1);
    }

    #[test]
    fn test_custom_lookup_complex_template() {
        let lookup = CustomLookup::new("json_has_key", "{column}::jsonb ? {value}");
        let sql = lookup.compile("data", "$1");
        assert_eq!(sql, "\"data\"::jsonb ? $1");
    }

    #[test]
    fn test_resolve_field_path_custom_lookup() {
        let mut registry = LookupRegistry::with_defaults();
        registry.register_lookup(
            "day_of_week_eq",
            CustomLookup::new("day_of_week_eq", "EXTRACT(DOW FROM {column}) = {value}"),
        );

        let (field, _col_sql, lookup) = registry.resolve_field_path(
            "created_at__day_of_week_eq",
            DatabaseBackendType::PostgreSQL,
        );
        assert_eq!(field, "created_at");
        assert_eq!(lookup, "day_of_week_eq");
    }

    #[test]
    fn test_empty_transforms_applied() {
        let registry = LookupRegistry::new();
        let sql = registry.apply_transforms("name", &[], DatabaseBackendType::PostgreSQL);
        assert_eq!(sql, "\"name\"");
    }
}
