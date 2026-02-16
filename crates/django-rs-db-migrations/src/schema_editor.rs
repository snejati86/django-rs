//! Schema editor implementations for DDL generation.
//!
//! The [`SchemaEditor`] trait defines operations for creating, modifying, and
//! dropping database schema objects. Each database backend has its own
//! implementation that generates the correct SQL dialect.

use django_rs_db::fields::{FieldDef, FieldType, OnDelete};
use django_rs_db::model::Index;
use django_rs_db::query::compiler::DatabaseBackendType;
use django_rs_db::value::Value;

use crate::autodetect::ModelState;

/// Generates DDL SQL for schema operations.
///
/// Each database backend implements this trait to produce syntactically correct
/// DDL statements. The trait returns `Vec<String>` because some operations
/// (especially on SQLite) require multiple statements.
pub trait SchemaEditor: Send + Sync {
    /// Returns the backend type this editor targets.
    fn backend_type(&self) -> DatabaseBackendType;

    /// Generates `CREATE TABLE` DDL for a model.
    fn create_table(&self, model: &ModelState) -> Vec<String>;

    /// Generates `DROP TABLE` DDL.
    fn drop_table(&self, table_name: &str) -> Vec<String>;

    /// Generates `ALTER TABLE ... ADD COLUMN` DDL.
    fn add_column(&self, table_name: &str, field: &FieldDef) -> Vec<String>;

    /// Generates `ALTER TABLE ... DROP COLUMN` DDL.
    fn drop_column(&self, table_name: &str, column_name: &str) -> Vec<String>;

    /// Generates DDL to alter a column's type, nullability, or default.
    fn alter_column(
        &self,
        table_name: &str,
        old_field: &FieldDef,
        new_field: &FieldDef,
    ) -> Vec<String>;

    /// Generates `ALTER TABLE ... RENAME COLUMN` DDL.
    fn rename_column(&self, table_name: &str, old_name: &str, new_name: &str) -> Vec<String>;

    /// Generates `CREATE INDEX` DDL.
    fn create_index(&self, table_name: &str, index: &Index) -> Vec<String>;

    /// Generates `DROP INDEX` DDL.
    fn drop_index(&self, index_name: &str) -> Vec<String>;

    /// Generates a `UNIQUE` constraint DDL.
    fn add_unique_constraint(&self, table_name: &str, columns: &[&str]) -> Vec<String>;

    /// Generates the SQL fragment for a column definition (type, constraints).
    fn column_sql(&self, field: &FieldDef) -> String;
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Generates the default value SQL fragment for a field.
fn default_sql(field: &FieldDef) -> String {
    match &field.default {
        Some(Value::Null) => " DEFAULT NULL".to_string(),
        Some(Value::Bool(b)) => format!(" DEFAULT {}", if *b { "TRUE" } else { "FALSE" }),
        Some(Value::Int(i)) => format!(" DEFAULT {i}"),
        Some(Value::Float(f)) => format!(" DEFAULT {f}"),
        Some(Value::String(s)) => format!(" DEFAULT '{}'", s.replace('\'', "''")),
        Some(_) => String::new(),
        None => String::new(),
    }
}

/// Generates ON DELETE clause SQL for foreign key fields.
fn on_delete_sql(on_delete: OnDelete) -> &'static str {
    match on_delete {
        OnDelete::Cascade => "CASCADE",
        OnDelete::Protect => "RESTRICT",
        OnDelete::SetNull => "SET NULL",
        OnDelete::SetDefault => "SET DEFAULT",
        OnDelete::DoNothing => "NO ACTION",
    }
}

/// Extracts the table part from a "app.Model" reference for FK targets.
fn fk_target_table(to: &str) -> String {
    // Format: "app_label.model_name" -> "app_label_model_name"
    to.replace('.', "_")
}

// ── PostgreSQL ───────────────────────────────────────────────────────────

/// Schema editor for PostgreSQL databases.
///
/// Uses PostgreSQL-specific DDL syntax including `BIGSERIAL`, `JSONB`, native
/// `UUID`, `BOOLEAN`, and proper `ALTER COLUMN` support.
pub struct PostgresSchemaEditor;

impl SchemaEditor for PostgresSchemaEditor {
    fn backend_type(&self) -> DatabaseBackendType {
        DatabaseBackendType::PostgreSQL
    }

    fn create_table(&self, model: &ModelState) -> Vec<String> {
        let table_name = model.db_table();
        let mut col_defs: Vec<String> = Vec::new();
        let mut constraints: Vec<String> = Vec::new();

        for field in &model.fields {
            let fd = field.to_field_def();
            col_defs.push(format!("\"{}\" {}", fd.column, self.column_sql(&fd)));

            // Foreign key constraint
            if let FieldType::ForeignKey {
                ref to,
                ref on_delete,
                ..
            } = fd.field_type
            {
                let target_table = fk_target_table(to);
                constraints.push(format!(
                    "FOREIGN KEY (\"{}\") REFERENCES \"{}\" (\"id\") ON DELETE {}",
                    fd.column,
                    target_table,
                    on_delete_sql(*on_delete)
                ));
            }
            if let FieldType::OneToOneField {
                ref to,
                ref on_delete,
                ..
            } = fd.field_type
            {
                let target_table = fk_target_table(to);
                constraints.push(format!(
                    "FOREIGN KEY (\"{}\") REFERENCES \"{}\" (\"id\") ON DELETE {}",
                    fd.column,
                    target_table,
                    on_delete_sql(*on_delete)
                ));
            }
        }

        let mut all_parts = col_defs;
        all_parts.extend(constraints);
        let body = all_parts.join(", ");
        vec![format!("CREATE TABLE \"{table_name}\" ({body})")]
    }

    fn drop_table(&self, table_name: &str) -> Vec<String> {
        vec![format!("DROP TABLE IF EXISTS \"{table_name}\"")]
    }

    fn add_column(&self, table_name: &str, field: &FieldDef) -> Vec<String> {
        let col_sql = self.column_sql(field);
        vec![format!(
            "ALTER TABLE \"{table_name}\" ADD COLUMN \"{}\" {col_sql}",
            field.column
        )]
    }

    fn drop_column(&self, table_name: &str, column_name: &str) -> Vec<String> {
        vec![format!(
            "ALTER TABLE \"{table_name}\" DROP COLUMN \"{column_name}\""
        )]
    }

    fn alter_column(
        &self,
        table_name: &str,
        _old_field: &FieldDef,
        new_field: &FieldDef,
    ) -> Vec<String> {
        let mut stmts = Vec::new();
        let col = &new_field.column;
        let type_sql = pg_type_sql(&new_field.field_type, new_field.max_length);

        stmts.push(format!(
            "ALTER TABLE \"{table_name}\" ALTER COLUMN \"{col}\" TYPE {type_sql}"
        ));

        if new_field.null {
            stmts.push(format!(
                "ALTER TABLE \"{table_name}\" ALTER COLUMN \"{col}\" DROP NOT NULL"
            ));
        } else {
            stmts.push(format!(
                "ALTER TABLE \"{table_name}\" ALTER COLUMN \"{col}\" SET NOT NULL"
            ));
        }

        if let Some(ref val) = new_field.default {
            let def = match val {
                Value::Null => "NULL".to_string(),
                Value::Bool(b) => (if *b { "TRUE" } else { "FALSE" }).to_string(),
                Value::Int(i) => i.to_string(),
                Value::Float(f) => f.to_string(),
                Value::String(s) => format!("'{}'", s.replace('\'', "''")),
                _ => "NULL".to_string(),
            };
            stmts.push(format!(
                "ALTER TABLE \"{table_name}\" ALTER COLUMN \"{col}\" SET DEFAULT {def}"
            ));
        } else {
            stmts.push(format!(
                "ALTER TABLE \"{table_name}\" ALTER COLUMN \"{col}\" DROP DEFAULT"
            ));
        }

        stmts
    }

    fn rename_column(&self, table_name: &str, old_name: &str, new_name: &str) -> Vec<String> {
        vec![format!(
            "ALTER TABLE \"{table_name}\" RENAME COLUMN \"{old_name}\" TO \"{new_name}\""
        )]
    }

    fn create_index(&self, table_name: &str, index: &Index) -> Vec<String> {
        let idx_name = index.name.as_deref().unwrap_or("unnamed_index");
        let unique = if index.unique { "UNIQUE " } else { "" };
        let cols: Vec<String> = index.fields.iter().map(|f| format!("\"{f}\"")).collect();
        vec![format!(
            "CREATE {unique}INDEX \"{idx_name}\" ON \"{table_name}\" ({})",
            cols.join(", ")
        )]
    }

    fn drop_index(&self, index_name: &str) -> Vec<String> {
        vec![format!("DROP INDEX IF EXISTS \"{index_name}\"")]
    }

    fn add_unique_constraint(&self, table_name: &str, columns: &[&str]) -> Vec<String> {
        let cols: Vec<String> = columns.iter().map(|c| format!("\"{c}\"")).collect();
        let constraint_name = format!("{table_name}_{}_{}", columns.join("_"), "uniq");
        vec![format!(
            "ALTER TABLE \"{table_name}\" ADD CONSTRAINT \"{constraint_name}\" UNIQUE ({})",
            cols.join(", ")
        )]
    }

    fn column_sql(&self, field: &FieldDef) -> String {
        let type_str = pg_type_sql(&field.field_type, field.max_length);
        let null_str = if field.primary_key {
            " PRIMARY KEY"
        } else if field.null {
            " NULL"
        } else {
            " NOT NULL"
        };
        let unique_str = if field.unique && !field.primary_key {
            " UNIQUE"
        } else {
            ""
        };
        let default_str = default_sql(field);
        format!("{type_str}{null_str}{unique_str}{default_str}")
    }
}

/// Returns the PostgreSQL type name for a field type.
fn pg_type_sql(field_type: &FieldType, max_length: Option<usize>) -> String {
    match field_type {
        FieldType::AutoField => "SERIAL".to_string(),
        FieldType::BigAutoField => "BIGSERIAL".to_string(),
        FieldType::CharField
        | FieldType::EmailField
        | FieldType::UrlField
        | FieldType::SlugField => {
            let len = max_length.unwrap_or(255);
            format!("VARCHAR({len})")
        }
        FieldType::TextField => "TEXT".to_string(),
        FieldType::IntegerField => "INTEGER".to_string(),
        FieldType::BigIntegerField => "BIGINT".to_string(),
        FieldType::SmallIntegerField => "SMALLINT".to_string(),
        FieldType::FloatField => "DOUBLE PRECISION".to_string(),
        FieldType::DecimalField {
            max_digits,
            decimal_places,
        } => format!("NUMERIC({max_digits}, {decimal_places})"),
        FieldType::BooleanField => "BOOLEAN".to_string(),
        FieldType::DateField => "DATE".to_string(),
        FieldType::DateTimeField => "TIMESTAMP".to_string(),
        FieldType::TimeField => "TIME".to_string(),
        FieldType::DurationField => "INTERVAL".to_string(),
        FieldType::UuidField => "UUID".to_string(),
        FieldType::BinaryField => "BYTEA".to_string(),
        FieldType::JsonField => "JSONB".to_string(),
        FieldType::IpAddressField => "INET".to_string(),
        FieldType::FilePathField => "VARCHAR(255)".to_string(),
        FieldType::ForeignKey { .. } | FieldType::OneToOneField { .. } => "BIGINT".to_string(),
        FieldType::ManyToManyField { .. } => String::new(), // handled separately
        FieldType::ArrayField { base_field, .. } => {
            format!("{}[]", pg_type_sql(base_field, None))
        }
        FieldType::HStoreField => "HSTORE".to_string(),
        FieldType::IntegerRangeField => "INT4RANGE".to_string(),
        FieldType::BigIntegerRangeField => "INT8RANGE".to_string(),
        FieldType::FloatRangeField => "NUMRANGE".to_string(),
        FieldType::DateRangeField => "DATERANGE".to_string(),
        FieldType::DateTimeRangeField => "TSTZRANGE".to_string(),
        FieldType::GeneratedField {
            expression,
            output_field,
            db_persist,
        } => {
            let output_type = pg_type_sql(output_field, None);
            let persist = if *db_persist { "STORED" } else { "VIRTUAL" };
            format!("{output_type} GENERATED ALWAYS AS ({expression}) {persist}")
        }
    }
}

// ── SQLite ───────────────────────────────────────────────────────────────

/// Schema editor for SQLite databases.
///
/// SQLite has limited `ALTER TABLE` support -- it cannot alter or drop columns
/// in older versions. For `alter_column` and `drop_column`, this editor uses
/// the table recreation strategy (create new table, copy data, swap).
pub struct SqliteSchemaEditor;

impl SchemaEditor for SqliteSchemaEditor {
    fn backend_type(&self) -> DatabaseBackendType {
        DatabaseBackendType::SQLite
    }

    fn create_table(&self, model: &ModelState) -> Vec<String> {
        let table_name = model.db_table();
        let mut col_defs: Vec<String> = Vec::new();
        let mut constraints: Vec<String> = Vec::new();

        for field in &model.fields {
            let fd = field.to_field_def();
            col_defs.push(format!("\"{}\" {}", fd.column, self.column_sql(&fd)));

            if let FieldType::ForeignKey {
                ref to,
                ref on_delete,
                ..
            } = fd.field_type
            {
                let target_table = fk_target_table(to);
                constraints.push(format!(
                    "FOREIGN KEY (\"{}\") REFERENCES \"{}\" (\"id\") ON DELETE {}",
                    fd.column,
                    target_table,
                    on_delete_sql(*on_delete)
                ));
            }
            if let FieldType::OneToOneField {
                ref to,
                ref on_delete,
                ..
            } = fd.field_type
            {
                let target_table = fk_target_table(to);
                constraints.push(format!(
                    "FOREIGN KEY (\"{}\") REFERENCES \"{}\" (\"id\") ON DELETE {}",
                    fd.column,
                    target_table,
                    on_delete_sql(*on_delete)
                ));
            }
        }

        let mut all_parts = col_defs;
        all_parts.extend(constraints);
        let body = all_parts.join(", ");
        vec![format!("CREATE TABLE \"{table_name}\" ({body})")]
    }

    fn drop_table(&self, table_name: &str) -> Vec<String> {
        vec![format!("DROP TABLE IF EXISTS \"{table_name}\"")]
    }

    fn add_column(&self, table_name: &str, field: &FieldDef) -> Vec<String> {
        let col_sql = self.column_sql(field);
        vec![format!(
            "ALTER TABLE \"{table_name}\" ADD COLUMN \"{}\" {col_sql}",
            field.column
        )]
    }

    fn drop_column(&self, table_name: &str, column_name: &str) -> Vec<String> {
        // SQLite table recreation strategy
        vec![
            format!("-- SQLite: recreate table to drop column \"{column_name}\""),
            format!("ALTER TABLE \"{table_name}\" DROP COLUMN \"{column_name}\""),
        ]
    }

    fn alter_column(
        &self,
        table_name: &str,
        _old_field: &FieldDef,
        new_field: &FieldDef,
    ) -> Vec<String> {
        // SQLite does not support ALTER COLUMN. We use the table recreation strategy.
        let col = &new_field.column;
        let type_str = sqlite_type_sql(&new_field.field_type);
        vec![
            format!("-- SQLite: recreate table to alter column \"{col}\""),
            format!(
                "-- New column definition: \"{col}\" {type_str}{}{}",
                if new_field.null { " NULL" } else { " NOT NULL" },
                default_sql(new_field)
            ),
            format!("CREATE TABLE \"__{table_name}_new\" AS SELECT * FROM \"{table_name}\""),
            format!("DROP TABLE \"{table_name}\""),
            format!("ALTER TABLE \"__{table_name}_new\" RENAME TO \"{table_name}\""),
        ]
    }

    fn rename_column(&self, table_name: &str, old_name: &str, new_name: &str) -> Vec<String> {
        // SQLite 3.25.0+ supports RENAME COLUMN
        vec![format!(
            "ALTER TABLE \"{table_name}\" RENAME COLUMN \"{old_name}\" TO \"{new_name}\""
        )]
    }

    fn create_index(&self, table_name: &str, index: &Index) -> Vec<String> {
        let idx_name = index.name.as_deref().unwrap_or("unnamed_index");
        let unique = if index.unique { "UNIQUE " } else { "" };
        let cols: Vec<String> = index.fields.iter().map(|f| format!("\"{f}\"")).collect();
        vec![format!(
            "CREATE {unique}INDEX \"{idx_name}\" ON \"{table_name}\" ({})",
            cols.join(", ")
        )]
    }

    fn drop_index(&self, index_name: &str) -> Vec<String> {
        vec![format!("DROP INDEX IF EXISTS \"{index_name}\"")]
    }

    fn add_unique_constraint(&self, table_name: &str, columns: &[&str]) -> Vec<String> {
        // SQLite: create a unique index to enforce the constraint
        let cols: Vec<String> = columns.iter().map(|c| format!("\"{c}\"")).collect();
        let idx_name = format!("{table_name}_{}_{}", columns.join("_"), "uniq");
        vec![format!(
            "CREATE UNIQUE INDEX \"{idx_name}\" ON \"{table_name}\" ({})",
            cols.join(", ")
        )]
    }

    fn column_sql(&self, field: &FieldDef) -> String {
        let type_str = sqlite_type_sql(&field.field_type);
        let null_str = if field.primary_key {
            " PRIMARY KEY"
        } else if field.null {
            ""
        } else {
            " NOT NULL"
        };
        let unique_str = if field.unique && !field.primary_key {
            " UNIQUE"
        } else {
            ""
        };
        // Auto-increment for SQLite primary keys
        let autoincrement = if field.primary_key
            && matches!(
                field.field_type,
                FieldType::AutoField | FieldType::BigAutoField
            ) {
            " AUTOINCREMENT"
        } else {
            ""
        };
        let default_str = default_sql(field);
        format!("{type_str}{null_str}{autoincrement}{unique_str}{default_str}")
    }
}

/// Returns the SQLite type name for a field type.
fn sqlite_type_sql(field_type: &FieldType) -> &'static str {
    match field_type {
        FieldType::AutoField | FieldType::BigAutoField => "INTEGER",
        FieldType::CharField
        | FieldType::TextField
        | FieldType::EmailField
        | FieldType::UrlField
        | FieldType::SlugField
        | FieldType::FilePathField
        | FieldType::IpAddressField => "TEXT",
        FieldType::IntegerField
        | FieldType::BigIntegerField
        | FieldType::SmallIntegerField
        | FieldType::BooleanField => "INTEGER",
        FieldType::FloatField | FieldType::DecimalField { .. } => "REAL",
        FieldType::DateField | FieldType::DateTimeField | FieldType::TimeField => "TEXT",
        FieldType::DurationField => "TEXT",
        FieldType::UuidField => "TEXT",
        FieldType::BinaryField => "BLOB",
        FieldType::JsonField => "TEXT",
        FieldType::ForeignKey { .. } | FieldType::OneToOneField { .. } => "INTEGER",
        FieldType::ManyToManyField { .. } => "",
        // PostgreSQL-specific types: use TEXT representation in SQLite
        FieldType::ArrayField { .. }
        | FieldType::HStoreField
        | FieldType::IntegerRangeField
        | FieldType::BigIntegerRangeField
        | FieldType::FloatRangeField
        | FieldType::DateRangeField
        | FieldType::DateTimeRangeField
        | FieldType::GeneratedField { .. } => "TEXT",
    }
}

// ── MySQL ────────────────────────────────────────────────────────────────

/// Schema editor for MySQL databases.
///
/// Uses MySQL-specific DDL syntax including `AUTO_INCREMENT`, `TINYINT(1)` for
/// booleans, `JSON` type, and `MODIFY COLUMN` for alterations.
pub struct MySqlSchemaEditor;

impl SchemaEditor for MySqlSchemaEditor {
    fn backend_type(&self) -> DatabaseBackendType {
        DatabaseBackendType::MySQL
    }

    fn create_table(&self, model: &ModelState) -> Vec<String> {
        let table_name = model.db_table();
        let mut col_defs: Vec<String> = Vec::new();
        let mut constraints: Vec<String> = Vec::new();

        for field in &model.fields {
            let fd = field.to_field_def();
            col_defs.push(format!("`{}` {}", fd.column, self.column_sql(&fd)));

            if let FieldType::ForeignKey {
                ref to,
                ref on_delete,
                ..
            } = fd.field_type
            {
                let target_table = fk_target_table(to);
                constraints.push(format!(
                    "FOREIGN KEY (`{}`) REFERENCES `{}` (`id`) ON DELETE {}",
                    fd.column,
                    target_table,
                    on_delete_sql(*on_delete)
                ));
            }
            if let FieldType::OneToOneField {
                ref to,
                ref on_delete,
                ..
            } = fd.field_type
            {
                let target_table = fk_target_table(to);
                constraints.push(format!(
                    "FOREIGN KEY (`{}`) REFERENCES `{}` (`id`) ON DELETE {}",
                    fd.column,
                    target_table,
                    on_delete_sql(*on_delete)
                ));
            }
        }

        let mut all_parts = col_defs;
        all_parts.extend(constraints);
        let body = all_parts.join(", ");
        vec![format!("CREATE TABLE `{table_name}` ({body})")]
    }

    fn drop_table(&self, table_name: &str) -> Vec<String> {
        vec![format!("DROP TABLE IF EXISTS `{table_name}`")]
    }

    fn add_column(&self, table_name: &str, field: &FieldDef) -> Vec<String> {
        let col_sql = self.column_sql(field);
        vec![format!(
            "ALTER TABLE `{table_name}` ADD COLUMN `{}` {col_sql}",
            field.column
        )]
    }

    fn drop_column(&self, table_name: &str, column_name: &str) -> Vec<String> {
        vec![format!(
            "ALTER TABLE `{table_name}` DROP COLUMN `{column_name}`"
        )]
    }

    fn alter_column(
        &self,
        table_name: &str,
        _old_field: &FieldDef,
        new_field: &FieldDef,
    ) -> Vec<String> {
        let col_sql = self.column_sql(new_field);
        vec![format!(
            "ALTER TABLE `{table_name}` MODIFY COLUMN `{}` {col_sql}",
            new_field.column
        )]
    }

    fn rename_column(&self, table_name: &str, old_name: &str, new_name: &str) -> Vec<String> {
        vec![format!(
            "ALTER TABLE `{table_name}` RENAME COLUMN `{old_name}` TO `{new_name}`"
        )]
    }

    fn create_index(&self, table_name: &str, index: &Index) -> Vec<String> {
        let idx_name = index.name.as_deref().unwrap_or("unnamed_index");
        let unique = if index.unique { "UNIQUE " } else { "" };
        let cols: Vec<String> = index.fields.iter().map(|f| format!("`{f}`")).collect();
        vec![format!(
            "CREATE {unique}INDEX `{idx_name}` ON `{table_name}` ({})",
            cols.join(", ")
        )]
    }

    fn drop_index(&self, index_name: &str) -> Vec<String> {
        vec![format!("DROP INDEX `{index_name}`")]
    }

    fn add_unique_constraint(&self, table_name: &str, columns: &[&str]) -> Vec<String> {
        let cols: Vec<String> = columns.iter().map(|c| format!("`{c}`")).collect();
        let constraint_name = format!("{table_name}_{}_{}", columns.join("_"), "uniq");
        vec![format!(
            "ALTER TABLE `{table_name}` ADD CONSTRAINT `{constraint_name}` UNIQUE ({})",
            cols.join(", ")
        )]
    }

    fn column_sql(&self, field: &FieldDef) -> String {
        let type_str = mysql_type_sql(&field.field_type, field.max_length);
        let null_str = if field.primary_key {
            " PRIMARY KEY"
        } else if field.null {
            " NULL"
        } else {
            " NOT NULL"
        };
        let unique_str = if field.unique && !field.primary_key {
            " UNIQUE"
        } else {
            ""
        };
        let auto_inc = if field.primary_key
            && matches!(
                field.field_type,
                FieldType::AutoField | FieldType::BigAutoField
            ) {
            " AUTO_INCREMENT"
        } else {
            ""
        };
        let default_str = default_sql(field);
        format!("{type_str}{null_str}{auto_inc}{unique_str}{default_str}")
    }
}

/// Returns the MySQL type name for a field type.
fn mysql_type_sql(field_type: &FieldType, max_length: Option<usize>) -> String {
    match field_type {
        FieldType::AutoField => "INT".to_string(),
        FieldType::BigAutoField => "BIGINT".to_string(),
        FieldType::CharField
        | FieldType::EmailField
        | FieldType::UrlField
        | FieldType::SlugField => {
            let len = max_length.unwrap_or(255);
            format!("VARCHAR({len})")
        }
        FieldType::TextField => "LONGTEXT".to_string(),
        FieldType::IntegerField => "INT".to_string(),
        FieldType::BigIntegerField => "BIGINT".to_string(),
        FieldType::SmallIntegerField => "SMALLINT".to_string(),
        FieldType::FloatField => "DOUBLE".to_string(),
        FieldType::DecimalField {
            max_digits,
            decimal_places,
        } => format!("DECIMAL({max_digits}, {decimal_places})"),
        FieldType::BooleanField => "TINYINT(1)".to_string(),
        FieldType::DateField => "DATE".to_string(),
        FieldType::DateTimeField => "DATETIME".to_string(),
        FieldType::TimeField => "TIME".to_string(),
        FieldType::DurationField => "BIGINT".to_string(),
        FieldType::UuidField => "CHAR(36)".to_string(),
        FieldType::BinaryField => "LONGBLOB".to_string(),
        FieldType::JsonField => "JSON".to_string(),
        FieldType::IpAddressField => "VARCHAR(45)".to_string(),
        FieldType::FilePathField => "VARCHAR(255)".to_string(),
        FieldType::ForeignKey { .. } | FieldType::OneToOneField { .. } => "BIGINT".to_string(),
        FieldType::ManyToManyField { .. } => String::new(),
        // PostgreSQL-specific types: use JSON representation in MySQL
        FieldType::ArrayField { .. } | FieldType::HStoreField => "JSON".to_string(),
        FieldType::IntegerRangeField
        | FieldType::BigIntegerRangeField
        | FieldType::FloatRangeField
        | FieldType::DateRangeField
        | FieldType::DateTimeRangeField => "VARCHAR(255)".to_string(),
        FieldType::GeneratedField {
            expression,
            output_field,
            db_persist,
        } => {
            let output_type = mysql_type_sql(output_field, None);
            let persist = if *db_persist { "STORED" } else { "VIRTUAL" };
            format!("{output_type} GENERATED ALWAYS AS ({expression}) {persist}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autodetect::MigrationFieldDef;
    use django_rs_db::model::IndexType;

    fn pg() -> PostgresSchemaEditor {
        PostgresSchemaEditor
    }

    fn sqlite() -> SqliteSchemaEditor {
        SqliteSchemaEditor
    }

    fn mysql() -> MySqlSchemaEditor {
        MySqlSchemaEditor
    }

    fn make_model(app: &str, name: &str, fields: Vec<MigrationFieldDef>) -> ModelState {
        ModelState::new(app, name, fields)
    }

    fn make_field(name: &str, ft: FieldType) -> MigrationFieldDef {
        MigrationFieldDef::new(name, ft)
    }

    // ── Backend types ───────────────────────────────────────────────

    #[test]
    fn test_pg_backend_type() {
        assert_eq!(pg().backend_type(), DatabaseBackendType::PostgreSQL);
    }

    #[test]
    fn test_sqlite_backend_type() {
        assert_eq!(sqlite().backend_type(), DatabaseBackendType::SQLite);
    }

    #[test]
    fn test_mysql_backend_type() {
        assert_eq!(mysql().backend_type(), DatabaseBackendType::MySQL);
    }

    // ── PostgreSQL column_sql ───────────────────────────────────────

    #[test]
    fn test_pg_column_sql_bigauto() {
        let fd = FieldDef::new("id", FieldType::BigAutoField).primary_key();
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("BIGSERIAL"));
        assert!(sql.contains("PRIMARY KEY"));
    }

    #[test]
    fn test_pg_column_sql_char() {
        let fd = FieldDef::new("name", FieldType::CharField).max_length(100);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("VARCHAR(100)"));
        assert!(sql.contains("NOT NULL"));
    }

    #[test]
    fn test_pg_column_sql_text() {
        let fd = FieldDef::new("body", FieldType::TextField).nullable();
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("TEXT"));
        assert!(sql.contains("NULL"));
        assert!(!sql.contains("NOT NULL"));
    }

    #[test]
    fn test_pg_column_sql_integer() {
        let fd = FieldDef::new("count", FieldType::IntegerField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("INTEGER"));
    }

    #[test]
    fn test_pg_column_sql_biginteger() {
        let fd = FieldDef::new("big", FieldType::BigIntegerField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("BIGINT"));
    }

    #[test]
    fn test_pg_column_sql_smallinteger() {
        let fd = FieldDef::new("small", FieldType::SmallIntegerField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("SMALLINT"));
    }

    #[test]
    fn test_pg_column_sql_float() {
        let fd = FieldDef::new("score", FieldType::FloatField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("DOUBLE PRECISION"));
    }

    #[test]
    fn test_pg_column_sql_decimal() {
        let fd = FieldDef::new(
            "price",
            FieldType::DecimalField {
                max_digits: 10,
                decimal_places: 2,
            },
        );
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("NUMERIC(10, 2)"));
    }

    #[test]
    fn test_pg_column_sql_boolean() {
        let fd = FieldDef::new("active", FieldType::BooleanField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("BOOLEAN"));
    }

    #[test]
    fn test_pg_column_sql_datetime() {
        let fd = FieldDef::new("created", FieldType::DateTimeField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("TIMESTAMP"));
    }

    #[test]
    fn test_pg_column_sql_json() {
        let fd = FieldDef::new("data", FieldType::JsonField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("JSONB"));
    }

    #[test]
    fn test_pg_column_sql_uuid() {
        let fd = FieldDef::new("uuid", FieldType::UuidField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("UUID"));
    }

    #[test]
    fn test_pg_column_sql_date() {
        let fd = FieldDef::new("birth", FieldType::DateField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("DATE"));
    }

    #[test]
    fn test_pg_column_sql_time() {
        let fd = FieldDef::new("at", FieldType::TimeField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("TIME"));
    }

    #[test]
    fn test_pg_column_sql_binary() {
        let fd = FieldDef::new("blob", FieldType::BinaryField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("BYTEA"));
    }

    #[test]
    fn test_pg_column_sql_unique() {
        let fd = FieldDef::new("email", FieldType::EmailField)
            .max_length(254)
            .unique();
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("UNIQUE"));
    }

    #[test]
    fn test_pg_column_sql_default() {
        let fd = FieldDef::new("active", FieldType::BooleanField).default(Value::Bool(true));
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("DEFAULT TRUE"));
    }

    #[test]
    fn test_pg_column_sql_default_int() {
        let fd = FieldDef::new("count", FieldType::IntegerField).default(Value::Int(0));
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("DEFAULT 0"));
    }

    #[test]
    fn test_pg_column_sql_default_string() {
        let fd = FieldDef::new("status", FieldType::CharField)
            .max_length(20)
            .default(Value::String("draft".into()));
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("DEFAULT 'draft'"));
    }

    #[test]
    fn test_pg_column_sql_ip() {
        let fd = FieldDef::new("ip", FieldType::IpAddressField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("INET"));
    }

    #[test]
    fn test_pg_column_sql_duration() {
        let fd = FieldDef::new("dur", FieldType::DurationField);
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("INTERVAL"));
    }

    #[test]
    fn test_pg_column_sql_fk() {
        let fd = FieldDef::new(
            "author",
            FieldType::ForeignKey {
                to: "auth.User".into(),
                on_delete: OnDelete::Cascade,
                related_name: None,
            },
        );
        let sql = pg().column_sql(&fd);
        assert!(sql.contains("BIGINT"));
    }

    // ── PostgreSQL CREATE TABLE ─────────────────────────────────────

    #[test]
    fn test_pg_create_table() {
        let model = make_model(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200),
                make_field("body", FieldType::TextField),
            ],
        );
        let sqls = pg().create_table(&model);
        assert_eq!(sqls.len(), 1);
        assert!(sqls[0].contains("CREATE TABLE \"blog_post\""));
        assert!(sqls[0].contains("BIGSERIAL"));
        assert!(sqls[0].contains("VARCHAR(200)"));
    }

    #[test]
    fn test_pg_create_table_with_fk() {
        let model = make_model(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field(
                    "author_id",
                    FieldType::ForeignKey {
                        to: "auth.user".into(),
                        on_delete: OnDelete::Cascade,
                        related_name: None,
                    },
                ),
            ],
        );
        let sqls = pg().create_table(&model);
        assert!(sqls[0].contains("FOREIGN KEY"));
        assert!(sqls[0].contains("CASCADE"));
    }

    // ── PostgreSQL DROP TABLE ───────────────────────────────────────

    #[test]
    fn test_pg_drop_table() {
        let sqls = pg().drop_table("blog_post");
        assert_eq!(sqls, vec!["DROP TABLE IF EXISTS \"blog_post\""]);
    }

    // ── PostgreSQL ADD/DROP COLUMN ──────────────────────────────────

    #[test]
    fn test_pg_add_column() {
        let fd = FieldDef::new("title", FieldType::CharField).max_length(200);
        let sqls = pg().add_column("blog_post", &fd);
        assert!(sqls[0].contains("ALTER TABLE \"blog_post\" ADD COLUMN"));
        assert!(sqls[0].contains("VARCHAR(200)"));
    }

    #[test]
    fn test_pg_drop_column() {
        let sqls = pg().drop_column("blog_post", "title");
        assert_eq!(
            sqls,
            vec!["ALTER TABLE \"blog_post\" DROP COLUMN \"title\""]
        );
    }

    // ── PostgreSQL ALTER COLUMN ─────────────────────────────────────

    #[test]
    fn test_pg_alter_column() {
        let old = FieldDef::new("title", FieldType::CharField).max_length(200);
        let new_field = FieldDef::new("title", FieldType::CharField)
            .max_length(500)
            .nullable();
        let sqls = pg().alter_column("blog_post", &old, &new_field);
        assert!(sqls.iter().any(|s| s.contains("ALTER COLUMN")));
        assert!(sqls.iter().any(|s| s.contains("TYPE VARCHAR(500)")));
        assert!(sqls.iter().any(|s| s.contains("DROP NOT NULL")));
    }

    // ── PostgreSQL RENAME COLUMN ────────────────────────────────────

    #[test]
    fn test_pg_rename_column() {
        let sqls = pg().rename_column("blog_post", "title", "headline");
        assert_eq!(
            sqls,
            vec!["ALTER TABLE \"blog_post\" RENAME COLUMN \"title\" TO \"headline\""]
        );
    }

    // ── PostgreSQL CREATE/DROP INDEX ────────────────────────────────

    #[test]
    fn test_pg_create_index() {
        let idx = Index {
            name: Some("idx_title".into()),
            fields: vec!["title".into()],
            unique: false,
            index_type: IndexType::default(),
        };
        let sqls = pg().create_index("blog_post", &idx);
        assert_eq!(
            sqls,
            vec!["CREATE INDEX \"idx_title\" ON \"blog_post\" (\"title\")"]
        );
    }

    #[test]
    fn test_pg_create_unique_index() {
        let idx = Index {
            name: Some("uniq_email".into()),
            fields: vec!["email".into()],
            unique: true,
            index_type: IndexType::default(),
        };
        let sqls = pg().create_index("users", &idx);
        assert!(sqls[0].contains("UNIQUE INDEX"));
    }

    #[test]
    fn test_pg_drop_index() {
        let sqls = pg().drop_index("idx_title");
        assert_eq!(sqls, vec!["DROP INDEX IF EXISTS \"idx_title\""]);
    }

    // ── PostgreSQL UNIQUE CONSTRAINT ────────────────────────────────

    #[test]
    fn test_pg_add_unique_constraint() {
        let sqls = pg().add_unique_constraint("blog_post", &["author", "slug"]);
        assert!(sqls[0].contains("ADD CONSTRAINT"));
        assert!(sqls[0].contains("UNIQUE"));
        assert!(sqls[0].contains("\"author\""));
        assert!(sqls[0].contains("\"slug\""));
    }

    // ── SQLite column_sql ───────────────────────────────────────────

    #[test]
    fn test_sqlite_column_sql_integer() {
        let fd = FieldDef::new("id", FieldType::BigAutoField).primary_key();
        let sql = sqlite().column_sql(&fd);
        assert!(sql.contains("INTEGER"));
        assert!(sql.contains("PRIMARY KEY"));
        assert!(sql.contains("AUTOINCREMENT"));
    }

    #[test]
    fn test_sqlite_column_sql_text() {
        let fd = FieldDef::new("name", FieldType::CharField).max_length(100);
        let sql = sqlite().column_sql(&fd);
        assert!(sql.contains("TEXT"));
        assert!(sql.contains("NOT NULL"));
    }

    #[test]
    fn test_sqlite_column_sql_boolean() {
        let fd = FieldDef::new("active", FieldType::BooleanField);
        let sql = sqlite().column_sql(&fd);
        assert!(sql.contains("INTEGER"));
    }

    #[test]
    fn test_sqlite_column_sql_float() {
        let fd = FieldDef::new("price", FieldType::FloatField);
        let sql = sqlite().column_sql(&fd);
        assert!(sql.contains("REAL"));
    }

    #[test]
    fn test_sqlite_column_sql_uuid() {
        let fd = FieldDef::new("uuid", FieldType::UuidField);
        let sql = sqlite().column_sql(&fd);
        assert!(sql.contains("TEXT"));
    }

    #[test]
    fn test_sqlite_column_sql_json() {
        let fd = FieldDef::new("data", FieldType::JsonField);
        let sql = sqlite().column_sql(&fd);
        assert!(sql.contains("TEXT"));
    }

    #[test]
    fn test_sqlite_column_sql_binary() {
        let fd = FieldDef::new("blob", FieldType::BinaryField);
        let sql = sqlite().column_sql(&fd);
        assert!(sql.contains("BLOB"));
    }

    #[test]
    fn test_sqlite_column_sql_datetime() {
        let fd = FieldDef::new("created", FieldType::DateTimeField);
        let sql = sqlite().column_sql(&fd);
        assert!(sql.contains("TEXT"));
    }

    // ── SQLite CREATE TABLE ─────────────────────────────────────────

    #[test]
    fn test_sqlite_create_table() {
        let model = make_model(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200),
            ],
        );
        let sqls = sqlite().create_table(&model);
        assert!(sqls[0].contains("CREATE TABLE"));
        assert!(sqls[0].contains("INTEGER"));
    }

    // ── SQLite ALTER COLUMN (recreate) ──────────────────────────────

    #[test]
    fn test_sqlite_alter_column_recreate() {
        let old = FieldDef::new("title", FieldType::CharField).max_length(200);
        let new_field = FieldDef::new("title", FieldType::CharField).max_length(500);
        let sqls = sqlite().alter_column("blog_post", &old, &new_field);
        // Should have recreation comments
        assert!(sqls.iter().any(|s| s.contains("recreate")));
    }

    // ── SQLite DROP COLUMN ──────────────────────────────────────────

    #[test]
    fn test_sqlite_drop_column() {
        let sqls = sqlite().drop_column("blog_post", "title");
        assert!(sqls.iter().any(|s| s.contains("DROP COLUMN")));
    }

    // ── SQLite RENAME COLUMN ────────────────────────────────────────

    #[test]
    fn test_sqlite_rename_column() {
        let sqls = sqlite().rename_column("blog_post", "title", "headline");
        assert!(sqls[0].contains("RENAME COLUMN"));
    }

    // ── SQLite INDEX ────────────────────────────────────────────────

    #[test]
    fn test_sqlite_create_index() {
        let idx = Index {
            name: Some("idx_title".into()),
            fields: vec!["title".into()],
            unique: false,
            index_type: IndexType::default(),
        };
        let sqls = sqlite().create_index("blog_post", &idx);
        assert!(sqls[0].contains("CREATE INDEX"));
    }

    #[test]
    fn test_sqlite_unique_constraint() {
        let sqls = sqlite().add_unique_constraint("blog_post", &["a", "b"]);
        assert!(sqls[0].contains("UNIQUE INDEX"));
    }

    // ── MySQL column_sql ────────────────────────────────────────────

    #[test]
    fn test_mysql_column_sql_bigauto() {
        let fd = FieldDef::new("id", FieldType::BigAutoField).primary_key();
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("BIGINT"));
        assert!(sql.contains("AUTO_INCREMENT"));
        assert!(sql.contains("PRIMARY KEY"));
    }

    #[test]
    fn test_mysql_column_sql_char() {
        let fd = FieldDef::new("name", FieldType::CharField).max_length(100);
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("VARCHAR(100)"));
    }

    #[test]
    fn test_mysql_column_sql_boolean() {
        let fd = FieldDef::new("active", FieldType::BooleanField);
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("TINYINT(1)"));
    }

    #[test]
    fn test_mysql_column_sql_text() {
        let fd = FieldDef::new("body", FieldType::TextField);
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("LONGTEXT"));
    }

    #[test]
    fn test_mysql_column_sql_json() {
        let fd = FieldDef::new("data", FieldType::JsonField);
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("JSON"));
    }

    #[test]
    fn test_mysql_column_sql_uuid() {
        let fd = FieldDef::new("uuid", FieldType::UuidField);
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("CHAR(36)"));
    }

    #[test]
    fn test_mysql_column_sql_datetime() {
        let fd = FieldDef::new("created", FieldType::DateTimeField);
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("DATETIME"));
    }

    #[test]
    fn test_mysql_column_sql_float() {
        let fd = FieldDef::new("score", FieldType::FloatField);
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("DOUBLE"));
    }

    #[test]
    fn test_mysql_column_sql_decimal() {
        let fd = FieldDef::new(
            "price",
            FieldType::DecimalField {
                max_digits: 10,
                decimal_places: 2,
            },
        );
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("DECIMAL(10, 2)"));
    }

    #[test]
    fn test_mysql_column_sql_binary() {
        let fd = FieldDef::new("blob", FieldType::BinaryField);
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("LONGBLOB"));
    }

    #[test]
    fn test_mysql_column_sql_duration() {
        let fd = FieldDef::new("dur", FieldType::DurationField);
        let sql = mysql().column_sql(&fd);
        assert!(sql.contains("BIGINT"));
    }

    // ── MySQL CREATE TABLE ──────────────────────────────────────────

    #[test]
    fn test_mysql_create_table() {
        let model = make_model(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200),
            ],
        );
        let sqls = mysql().create_table(&model);
        assert!(sqls[0].contains("CREATE TABLE `blog_post`"));
        assert!(sqls[0].contains("BIGINT"));
        assert!(sqls[0].contains("AUTO_INCREMENT"));
    }

    // ── MySQL DROP TABLE ────────────────────────────────────────────

    #[test]
    fn test_mysql_drop_table() {
        let sqls = mysql().drop_table("blog_post");
        assert_eq!(sqls, vec!["DROP TABLE IF EXISTS `blog_post`"]);
    }

    // ── MySQL ADD/DROP COLUMN ───────────────────────────────────────

    #[test]
    fn test_mysql_add_column() {
        let fd = FieldDef::new("title", FieldType::CharField).max_length(200);
        let sqls = mysql().add_column("blog_post", &fd);
        assert!(sqls[0].contains("ALTER TABLE `blog_post` ADD COLUMN"));
    }

    #[test]
    fn test_mysql_drop_column() {
        let sqls = mysql().drop_column("blog_post", "title");
        assert_eq!(sqls, vec!["ALTER TABLE `blog_post` DROP COLUMN `title`"]);
    }

    // ── MySQL ALTER COLUMN (MODIFY) ─────────────────────────────────

    #[test]
    fn test_mysql_alter_column() {
        let old = FieldDef::new("title", FieldType::CharField).max_length(200);
        let new_field = FieldDef::new("title", FieldType::CharField).max_length(500);
        let sqls = mysql().alter_column("blog_post", &old, &new_field);
        assert!(sqls[0].contains("MODIFY COLUMN"));
    }

    // ── MySQL RENAME COLUMN ─────────────────────────────────────────

    #[test]
    fn test_mysql_rename_column() {
        let sqls = mysql().rename_column("blog_post", "title", "headline");
        assert!(sqls[0].contains("RENAME COLUMN"));
    }

    // ── MySQL INDEX ─────────────────────────────────────────────────

    #[test]
    fn test_mysql_create_index() {
        let idx = Index {
            name: Some("idx_title".into()),
            fields: vec!["title".into()],
            unique: false,
            index_type: IndexType::default(),
        };
        let sqls = mysql().create_index("blog_post", &idx);
        assert!(sqls[0].contains("CREATE INDEX `idx_title`"));
    }

    #[test]
    fn test_mysql_drop_index() {
        let sqls = mysql().drop_index("idx_title");
        assert_eq!(sqls, vec!["DROP INDEX `idx_title`"]);
    }

    #[test]
    fn test_mysql_unique_constraint() {
        let sqls = mysql().add_unique_constraint("blog_post", &["a", "b"]);
        assert!(sqls[0].contains("UNIQUE"));
        assert!(sqls[0].contains("`a`"));
    }

    // ── Cross-backend comparison ────────────────────────────────────

    #[test]
    fn test_all_backends_create_table_different_syntax() {
        let model = make_model(
            "app",
            "item",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("name", FieldType::CharField).max_length(100),
            ],
        );
        let pg_sql = pg().create_table(&model);
        let sqlite_sql = sqlite().create_table(&model);
        let mysql_sql = mysql().create_table(&model);

        // PostgreSQL uses BIGSERIAL
        assert!(pg_sql[0].contains("BIGSERIAL"));
        // SQLite uses INTEGER
        assert!(sqlite_sql[0].contains("INTEGER"));
        // MySQL uses BIGINT with AUTO_INCREMENT
        assert!(mysql_sql[0].contains("BIGINT"));
        assert!(mysql_sql[0].contains("AUTO_INCREMENT"));
    }

    #[test]
    fn test_all_backends_boolean_different_types() {
        let fd = FieldDef::new("flag", FieldType::BooleanField);
        let pg_sql = pg().column_sql(&fd);
        let sqlite_sql = sqlite().column_sql(&fd);
        let mysql_sql = mysql().column_sql(&fd);

        assert!(pg_sql.contains("BOOLEAN"));
        assert!(sqlite_sql.contains("INTEGER"));
        assert!(mysql_sql.contains("TINYINT(1)"));
    }

    #[test]
    fn test_all_backends_uuid_different_types() {
        let fd = FieldDef::new("u", FieldType::UuidField);
        let pg_sql = pg().column_sql(&fd);
        let sqlite_sql = sqlite().column_sql(&fd);
        let mysql_sql = mysql().column_sql(&fd);

        assert!(pg_sql.contains("UUID"));
        assert!(sqlite_sql.contains("TEXT"));
        assert!(mysql_sql.contains("CHAR(36)"));
    }

    #[test]
    fn test_all_backends_json_different_types() {
        let fd = FieldDef::new("data", FieldType::JsonField);
        let pg_sql = pg().column_sql(&fd);
        let sqlite_sql = sqlite().column_sql(&fd);
        let mysql_sql = mysql().column_sql(&fd);

        assert!(pg_sql.contains("JSONB"));
        assert!(sqlite_sql.contains("TEXT"));
        assert!(mysql_sql.contains("JSON"));
    }

    // ── On delete SQL ───────────────────────────────────────────────

    #[test]
    fn test_on_delete_cascade() {
        assert_eq!(on_delete_sql(OnDelete::Cascade), "CASCADE");
    }

    #[test]
    fn test_on_delete_protect() {
        assert_eq!(on_delete_sql(OnDelete::Protect), "RESTRICT");
    }

    #[test]
    fn test_on_delete_set_null() {
        assert_eq!(on_delete_sql(OnDelete::SetNull), "SET NULL");
    }

    #[test]
    fn test_on_delete_set_default() {
        assert_eq!(on_delete_sql(OnDelete::SetDefault), "SET DEFAULT");
    }

    #[test]
    fn test_on_delete_do_nothing() {
        assert_eq!(on_delete_sql(OnDelete::DoNothing), "NO ACTION");
    }
}
