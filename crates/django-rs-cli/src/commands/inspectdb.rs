//! The `inspectdb` management command.
//!
//! Inspects the database schema and generates Rust model definitions.
//! This mirrors Django's `inspectdb` command.

use std::fmt::Write as _;

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Introspects the database and generates Model code.
///
/// Reads the database schema and outputs Rust structs with `Model` trait
/// implementations for each discovered table.
pub struct InspectdbCommand;

/// A column descriptor produced by database introspection.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    /// The column name.
    pub name: String,
    /// The SQL data type (e.g. "VARCHAR(255)", "INTEGER").
    pub data_type: String,
    /// Whether the column allows NULL values.
    pub nullable: bool,
    /// Whether this column is a primary key.
    pub primary_key: bool,
    /// Optional foreign key reference ("table.column").
    pub foreign_key: Option<String>,
}

/// A table descriptor produced by database introspection.
#[derive(Debug, Clone)]
pub struct TableInfo {
    /// The table name.
    pub name: String,
    /// The columns in this table.
    pub columns: Vec<ColumnInfo>,
}

/// Maps an SQL data type string to a Rust type string.
pub fn sql_type_to_rust_type(sql_type: &str) -> &'static str {
    let upper = sql_type.to_uppercase();
    if upper.starts_with("VARCHAR") || upper.starts_with("TEXT") || upper.starts_with("CHAR") {
        "String"
    } else if upper.starts_with("INT") || upper == "INTEGER" || upper.starts_with("BIGINT") {
        "i64"
    } else if upper.starts_with("SMALLINT") || upper.starts_with("TINYINT") {
        "i32"
    } else if upper.starts_with("BOOL") {
        "bool"
    } else if upper.starts_with("FLOAT")
        || upper.starts_with("DOUBLE")
        || upper.starts_with("REAL")
        || upper.starts_with("DECIMAL")
        || upper.starts_with("NUMERIC")
    {
        "f64"
    } else if upper.starts_with("DATE") || upper.starts_with("TIMESTAMP") {
        "String"
    } else if upper.starts_with("BLOB") || upper.starts_with("BYTEA") {
        "Vec<u8>"
    } else {
        "String"
    }
}

/// Maps an SQL data type string to a `FieldType` variant name.
pub fn sql_type_to_field_type(sql_type: &str) -> &'static str {
    let upper = sql_type.to_uppercase();
    if upper.starts_with("VARCHAR") || upper.starts_with("CHAR") {
        "CharField"
    } else if upper.starts_with("TEXT") {
        "TextField"
    } else if upper == "INTEGER" || upper.starts_with("INT") {
        "IntegerField"
    } else if upper.starts_with("BIGINT") {
        "BigIntegerField"
    } else if upper.starts_with("SMALLINT") {
        "SmallIntegerField"
    } else if upper.starts_with("BOOL") {
        "BooleanField"
    } else if upper.starts_with("FLOAT") || upper.starts_with("DOUBLE") || upper.starts_with("REAL")
    {
        "FloatField"
    } else if upper.starts_with("DECIMAL") || upper.starts_with("NUMERIC") {
        "DecimalField"
    } else if upper.starts_with("DATE") && !upper.starts_with("DATETIME") {
        "DateField"
    } else if upper.starts_with("DATETIME") || upper.starts_with("TIMESTAMP") {
        "DateTimeField"
    } else if upper.starts_with("BLOB") || upper.starts_with("BYTEA") {
        "BinaryField"
    } else {
        "TextField"
    }
}

/// Converts a snake_case table name to a PascalCase struct name.
pub fn table_name_to_struct_name(table_name: &str) -> String {
    table_name
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |c| {
                c.to_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

/// Generates a Rust model struct and trait implementation from a `TableInfo`.
pub fn generate_model_code(table: &TableInfo) -> String {
    let struct_name = table_name_to_struct_name(&table.name);
    let mut code = String::new();

    // Struct definition
    let _ = writeln!(
        code,
        "/// Auto-generated model for the `{}` table.",
        table.name
    );
    let _ = writeln!(code, "pub struct {struct_name} {{");

    for col in &table.columns {
        let rust_type = sql_type_to_rust_type(&col.data_type);
        if col.nullable && !col.primary_key {
            let _ = writeln!(code, "    pub {}: Option<{rust_type}>,", col.name);
        } else {
            let _ = writeln!(code, "    pub {}: {rust_type},", col.name);
        }
    }

    code.push_str("}\n");

    code
}

#[async_trait]
impl ManagementCommand for InspectdbCommand {
    fn name(&self) -> &'static str {
        "inspectdb"
    }

    fn help(&self) -> &'static str {
        "Inspect database and generate Model definitions"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("table")
                .help("Specific table(s) to inspect")
                .num_args(0..),
        )
        .arg(
            clap::Arg::new("database")
                .long("database")
                .default_value("default")
                .help("Database alias to inspect"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        _settings: &Settings,
    ) -> Result<(), DjangoError> {
        let database = matches
            .get_one::<String>("database")
            .map_or("default", String::as_str);
        let tables: Vec<&String> = matches
            .get_many::<String>("table")
            .map_or_else(Vec::new, Iterator::collect);

        tracing::info!("Inspecting database '{database}'");

        if tables.is_empty() {
            tracing::info!("Inspecting all tables");
        } else {
            tracing::info!(
                "Inspecting tables: {}",
                tables
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        // In a full implementation, this would connect to the database and
        // introspect the schema. For demonstration, we show what the output
        // format looks like.
        tracing::info!("Database inspection complete. In a full implementation, model code would be printed to stdout.");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_type_to_rust_type() {
        assert_eq!(sql_type_to_rust_type("VARCHAR(255)"), "String");
        assert_eq!(sql_type_to_rust_type("TEXT"), "String");
        assert_eq!(sql_type_to_rust_type("INTEGER"), "i64");
        assert_eq!(sql_type_to_rust_type("BIGINT"), "i64");
        assert_eq!(sql_type_to_rust_type("SMALLINT"), "i32");
        assert_eq!(sql_type_to_rust_type("BOOLEAN"), "bool");
        assert_eq!(sql_type_to_rust_type("FLOAT"), "f64");
        assert_eq!(sql_type_to_rust_type("DOUBLE PRECISION"), "f64");
        assert_eq!(sql_type_to_rust_type("DECIMAL(10,2)"), "f64");
        assert_eq!(sql_type_to_rust_type("DATE"), "String");
        assert_eq!(sql_type_to_rust_type("TIMESTAMP"), "String");
        assert_eq!(sql_type_to_rust_type("BLOB"), "Vec<u8>");
        assert_eq!(sql_type_to_rust_type("BYTEA"), "Vec<u8>");
        assert_eq!(sql_type_to_rust_type("UNKNOWN"), "String");
    }

    #[test]
    fn test_sql_type_to_field_type() {
        assert_eq!(sql_type_to_field_type("VARCHAR(255)"), "CharField");
        assert_eq!(sql_type_to_field_type("TEXT"), "TextField");
        assert_eq!(sql_type_to_field_type("INTEGER"), "IntegerField");
        assert_eq!(sql_type_to_field_type("BIGINT"), "BigIntegerField");
        assert_eq!(sql_type_to_field_type("BOOLEAN"), "BooleanField");
        assert_eq!(sql_type_to_field_type("FLOAT"), "FloatField");
        assert_eq!(sql_type_to_field_type("DECIMAL(10,2)"), "DecimalField");
        assert_eq!(sql_type_to_field_type("DATE"), "DateField");
        assert_eq!(sql_type_to_field_type("DATETIME"), "DateTimeField");
        assert_eq!(sql_type_to_field_type("TIMESTAMP"), "DateTimeField");
        assert_eq!(sql_type_to_field_type("BLOB"), "BinaryField");
    }

    #[test]
    fn test_table_name_to_struct_name() {
        assert_eq!(table_name_to_struct_name("auth_user"), "AuthUser");
        assert_eq!(table_name_to_struct_name("blog_post"), "BlogPost");
        assert_eq!(table_name_to_struct_name("my_app_model"), "MyAppModel");
        assert_eq!(table_name_to_struct_name("simple"), "Simple");
    }

    #[test]
    fn test_generate_model_code() {
        let table = TableInfo {
            name: "blog_post".to_string(),
            columns: vec![
                ColumnInfo {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    foreign_key: None,
                },
                ColumnInfo {
                    name: "title".to_string(),
                    data_type: "VARCHAR(200)".to_string(),
                    nullable: false,
                    primary_key: false,
                    foreign_key: None,
                },
                ColumnInfo {
                    name: "content".to_string(),
                    data_type: "TEXT".to_string(),
                    nullable: true,
                    primary_key: false,
                    foreign_key: None,
                },
            ],
        };

        let code = generate_model_code(&table);
        assert!(code.contains("pub struct BlogPost"));
        assert!(code.contains("pub id: i64"));
        assert!(code.contains("pub title: String"));
        assert!(code.contains("pub content: Option<String>"));
    }

    #[test]
    fn test_generate_model_code_with_nullable_pk() {
        let table = TableInfo {
            name: "test".to_string(),
            columns: vec![ColumnInfo {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: true,
                primary_key: true,
                foreign_key: None,
            }],
        };

        let code = generate_model_code(&table);
        // Primary keys should not be Optional even if nullable
        assert!(code.contains("pub id: i64"));
        assert!(!code.contains("Option"));
    }

    #[test]
    fn test_command_metadata() {
        let cmd = InspectdbCommand;
        assert_eq!(cmd.name(), "inspectdb");
        assert_eq!(
            cmd.help(),
            "Inspect database and generate Model definitions"
        );
    }
}
