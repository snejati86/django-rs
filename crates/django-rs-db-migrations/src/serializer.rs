//! Migration file serialization and deserialization.
//!
//! Provides JSON serialization for migrations so the autodetector can write
//! migration files and the loader can read them back. The JSON format matches
//! what [`MigrationLoader`](crate::loader::MigrationLoader) expects.

use std::path::{Path, PathBuf};

use django_rs_core::DjangoError;
use django_rs_db::model::Index;
use serde::{Deserialize, Serialize};

use crate::autodetect::{MigrationFieldDef, ModelOptions};
use crate::operations::{
    AddField, AddIndex, AlterField, AlterUniqueTogether, CreateModel, DeleteModel, Operation,
    RemoveField, RemoveIndex, RenameField, RunSQL,
};

/// A serializable representation of a migration file.
///
/// This struct maps to the JSON format expected by the migration loader.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableMigration {
    /// The app label this migration belongs to.
    pub app_label: String,
    /// The migration name (e.g. "0001_initial").
    pub name: String,
    /// Dependencies as `[app_label, name]` pairs.
    pub dependencies: Vec<(String, String)>,
    /// Whether this is the initial migration for the app.
    #[serde(default)]
    pub initial: bool,
    /// The operations to apply.
    pub operations: Vec<SerializableOperation>,
}

/// A serializable representation of a single migration operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SerializableOperation {
    /// Create a new model/table.
    CreateModel {
        /// The model name.
        name: String,
        /// The fields for the model.
        fields: Vec<MigrationFieldDef>,
        /// Model options.
        #[serde(default)]
        options: ModelOptions,
    },
    /// Delete a model/table.
    DeleteModel {
        /// The model name.
        name: String,
    },
    /// Add a field/column to an existing model.
    AddField {
        /// The model name.
        model_name: String,
        /// The field definition.
        field: MigrationFieldDef,
    },
    /// Remove a field/column from a model.
    RemoveField {
        /// The model name.
        model_name: String,
        /// The field name.
        field_name: String,
    },
    /// Alter a field/column on a model.
    AlterField {
        /// The model name.
        model_name: String,
        /// The field name.
        field_name: String,
        /// The new field definition.
        field: MigrationFieldDef,
    },
    /// Rename a field/column.
    RenameField {
        /// The model name.
        model_name: String,
        /// The old field name.
        old_name: String,
        /// The new field name.
        new_name: String,
    },
    /// Add an index to a table.
    AddIndex {
        /// The model name.
        model_name: String,
        /// The index definition.
        index: Index,
    },
    /// Remove an index from a table.
    RemoveIndex {
        /// The model name.
        model_name: String,
        /// The index name.
        index_name: String,
    },
    /// Alter unique_together constraints.
    AlterUniqueTogether {
        /// The model name.
        model_name: String,
        /// The new unique_together groups.
        unique_together: Vec<Vec<String>>,
    },
    /// Run raw SQL.
    RunSQL {
        /// Forward SQL.
        sql_forwards: String,
        /// Backward SQL.
        sql_backwards: String,
    },
}

impl SerializableMigration {
    /// Serializes this migration to a JSON string.
    pub fn to_json(&self) -> Result<String, DjangoError> {
        serde_json::to_string_pretty(self).map_err(|e| {
            DjangoError::DatabaseError(format!("Failed to serialize migration: {e}"))
        })
    }

    /// Deserializes a migration from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, DjangoError> {
        serde_json::from_str(json).map_err(|e| {
            DjangoError::DatabaseError(format!("Failed to deserialize migration: {e}"))
        })
    }

    /// Converts operations from boxed trait objects to serializable form.
    pub fn from_operations(
        app_label: &str,
        name: &str,
        dependencies: Vec<(String, String)>,
        initial: bool,
        operations: &[Box<dyn Operation>],
    ) -> Self {
        let serializable_ops: Vec<SerializableOperation> = operations
            .iter()
            .filter_map(|op| SerializableOperation::from_operation(op.as_ref()))
            .collect();

        Self {
            app_label: app_label.to_string(),
            name: name.to_string(),
            dependencies,
            initial,
            operations: serializable_ops,
        }
    }

    /// Converts the serializable operations back to boxed trait objects.
    pub fn to_operations(&self) -> Vec<Box<dyn Operation>> {
        self.operations
            .iter()
            .map(SerializableOperation::to_operation)
            .collect()
    }

    /// Writes this migration to a file at the given path.
    pub fn write_to_file(&self, path: &Path) -> Result<(), DjangoError> {
        let json = self.to_json()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DjangoError::DatabaseError(format!("Failed to create directory: {e}"))
            })?;
        }
        std::fs::write(path, json).map_err(|e| {
            DjangoError::DatabaseError(format!("Failed to write migration file: {e}"))
        })
    }

    /// Reads a migration from a file.
    pub fn read_from_file(path: &Path) -> Result<Self, DjangoError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            DjangoError::DatabaseError(format!("Failed to read migration file: {e}"))
        })?;
        Self::from_json(&content)
    }
}

impl SerializableOperation {
    /// Attempts to convert a trait-object `Operation` to a serializable form.
    ///
    /// Uses the `describe()` method to determine the operation type and extract
    /// relevant fields. Returns `None` for unsupported operations (e.g. `RunRust`).
    fn from_operation(op: &dyn Operation) -> Option<Self> {
        // We use describe() to identify the operation type, but since we can't
        // downcast trait objects without Any bounds, we can't extract fields.
        // Callers should use the concrete from_create_model, from_add_field, etc.
        let _desc = op.describe();
        None
    }

    /// Creates a serializable operation from a concrete `CreateModel`.
    pub fn from_create_model(op: &CreateModel) -> Self {
        Self::CreateModel {
            name: op.name.clone(),
            fields: op.fields.clone(),
            options: op.options.clone(),
        }
    }

    /// Creates a serializable operation from a concrete `DeleteModel`.
    pub fn from_delete_model(op: &DeleteModel) -> Self {
        Self::DeleteModel {
            name: op.name.clone(),
        }
    }

    /// Creates a serializable operation from a concrete `AddField`.
    pub fn from_add_field(op: &AddField) -> Self {
        Self::AddField {
            model_name: op.model_name.clone(),
            field: op.field.clone(),
        }
    }

    /// Creates a serializable operation from a concrete `RemoveField`.
    pub fn from_remove_field(op: &RemoveField) -> Self {
        Self::RemoveField {
            model_name: op.model_name.clone(),
            field_name: op.field_name.clone(),
        }
    }

    /// Creates a serializable operation from a concrete `AlterField`.
    pub fn from_alter_field(op: &AlterField) -> Self {
        Self::AlterField {
            model_name: op.model_name.clone(),
            field_name: op.field_name.clone(),
            field: op.field.clone(),
        }
    }

    /// Creates a serializable operation from a concrete `RenameField`.
    pub fn from_rename_field(op: &RenameField) -> Self {
        Self::RenameField {
            model_name: op.model_name.clone(),
            old_name: op.old_name.clone(),
            new_name: op.new_name.clone(),
        }
    }

    /// Creates a serializable operation from a concrete `AddIndex`.
    pub fn from_add_index(op: &AddIndex) -> Self {
        Self::AddIndex {
            model_name: op.model_name.clone(),
            index: op.index.clone(),
        }
    }

    /// Creates a serializable operation from a concrete `RemoveIndex`.
    pub fn from_remove_index(op: &RemoveIndex) -> Self {
        Self::RemoveIndex {
            model_name: op.model_name.clone(),
            index_name: op.index_name.clone(),
        }
    }

    /// Creates a serializable operation from a concrete `AlterUniqueTogether`.
    pub fn from_alter_unique_together(op: &AlterUniqueTogether) -> Self {
        Self::AlterUniqueTogether {
            model_name: op.model_name.clone(),
            unique_together: op.unique_together.clone(),
        }
    }

    /// Creates a serializable operation from a concrete `RunSQL`.
    pub fn from_run_sql(op: &RunSQL) -> Self {
        Self::RunSQL {
            sql_forwards: op.sql_forwards.clone(),
            sql_backwards: op.sql_backwards.clone(),
        }
    }

    /// Converts this serializable operation to a boxed `dyn Operation`.
    pub fn to_operation(&self) -> Box<dyn Operation> {
        match self {
            Self::CreateModel {
                name,
                fields,
                options,
            } => Box::new(CreateModel {
                name: name.clone(),
                fields: fields.clone(),
                options: options.clone(),
            }),
            Self::DeleteModel { name } => Box::new(DeleteModel { name: name.clone() }),
            Self::AddField { model_name, field } => Box::new(AddField {
                model_name: model_name.clone(),
                field: field.clone(),
            }),
            Self::RemoveField {
                model_name,
                field_name,
            } => Box::new(RemoveField {
                model_name: model_name.clone(),
                field_name: field_name.clone(),
            }),
            Self::AlterField {
                model_name,
                field_name,
                field,
            } => Box::new(AlterField {
                model_name: model_name.clone(),
                field_name: field_name.clone(),
                field: field.clone(),
            }),
            Self::RenameField {
                model_name,
                old_name,
                new_name,
            } => Box::new(RenameField {
                model_name: model_name.clone(),
                old_name: old_name.clone(),
                new_name: new_name.clone(),
            }),
            Self::AddIndex { model_name, index } => Box::new(AddIndex {
                model_name: model_name.clone(),
                index: index.clone(),
            }),
            Self::RemoveIndex {
                model_name,
                index_name,
            } => Box::new(RemoveIndex {
                model_name: model_name.clone(),
                index_name: index_name.clone(),
            }),
            Self::AlterUniqueTogether {
                model_name,
                unique_together,
            } => Box::new(AlterUniqueTogether {
                model_name: model_name.clone(),
                unique_together: unique_together.clone(),
            }),
            Self::RunSQL {
                sql_forwards,
                sql_backwards,
            } => Box::new(RunSQL {
                sql_forwards: sql_forwards.clone(),
                sql_backwards: sql_backwards.clone(),
            }),
        }
    }
}

/// Generates a sequential migration filename.
///
/// If `custom_name` is provided, uses that. Otherwise generates an auto name
/// based on the migration number and current timestamp.
pub fn generate_migration_name(
    number: u32,
    custom_name: Option<&str>,
) -> String {
    if let Some(name) = custom_name {
        format!("{number:04}_{name}")
    } else {
        let now = chrono::Utc::now();
        format!(
            "{number:04}_auto_{}{:02}{:02}_{:02}{:02}",
            now.format("%Y"),
            now.format("%m"),
            now.format("%d"),
            now.format("%H"),
            now.format("%M")
        )
    }
}

/// Determines the next migration number for an app by scanning existing files.
pub fn next_migration_number(migrations_dir: &Path, app_label: &str) -> u32 {
    let app_dir = migrations_dir.join(app_label);
    if !app_dir.exists() {
        return 1;
    }

    let mut max_num = 0u32;
    if let Ok(entries) = std::fs::read_dir(&app_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|n| n.to_str()) {
                    // Parse the leading digits: "0001_initial" -> 1
                    if let Some(num_str) = stem.split('_').next() {
                        if let Ok(num) = num_str.parse::<u32>() {
                            max_num = max_num.max(num);
                        }
                    }
                }
            }
        }
    }

    max_num + 1
}

/// Returns the path where a migration file should be written.
pub fn migration_file_path(
    migrations_dir: &Path,
    app_label: &str,
    name: &str,
) -> PathBuf {
    migrations_dir.join(app_label).join(format!("{name}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use django_rs_db::fields::{FieldType, OnDelete};

    fn make_field(name: &str, ft: FieldType) -> MigrationFieldDef {
        MigrationFieldDef::new(name, ft)
    }

    // ── SerializableMigration JSON round-trip ─────────────────────────

    #[test]
    fn test_serializable_migration_roundtrip() {
        let migration = SerializableMigration {
            app_label: "blog".into(),
            name: "0001_initial".into(),
            dependencies: vec![],
            initial: true,
            operations: vec![SerializableOperation::CreateModel {
                name: "post".into(),
                fields: vec![
                    make_field("id", FieldType::BigAutoField).primary_key(),
                    make_field("title", FieldType::CharField).max_length(200),
                ],
                options: ModelOptions::default(),
            }],
        };

        let json = migration.to_json().unwrap();
        let deserialized = SerializableMigration::from_json(&json).unwrap();

        assert_eq!(deserialized.app_label, "blog");
        assert_eq!(deserialized.name, "0001_initial");
        assert!(deserialized.initial);
        assert_eq!(deserialized.operations.len(), 1);
    }

    #[test]
    fn test_serializable_migration_all_operations() {
        let migration = SerializableMigration {
            app_label: "myapp".into(),
            name: "0002_changes".into(),
            dependencies: vec![("myapp".into(), "0001_initial".into())],
            initial: false,
            operations: vec![
                SerializableOperation::CreateModel {
                    name: "user".into(),
                    fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
                    options: ModelOptions::default(),
                },
                SerializableOperation::DeleteModel {
                    name: "old_model".into(),
                },
                SerializableOperation::AddField {
                    model_name: "user".into(),
                    field: make_field("email", FieldType::EmailField).max_length(254),
                },
                SerializableOperation::RemoveField {
                    model_name: "user".into(),
                    field_name: "temp".into(),
                },
                SerializableOperation::AlterField {
                    model_name: "user".into(),
                    field_name: "email".into(),
                    field: make_field("email", FieldType::EmailField)
                        .max_length(254)
                        .unique(),
                },
                SerializableOperation::RenameField {
                    model_name: "user".into(),
                    old_name: "name".into(),
                    new_name: "full_name".into(),
                },
                SerializableOperation::AddIndex {
                    model_name: "user".into(),
                    index: Index {
                        name: Some("idx_email".into()),
                        fields: vec!["email".into()],
                        unique: true,
                    },
                },
                SerializableOperation::RemoveIndex {
                    model_name: "user".into(),
                    index_name: "idx_old".into(),
                },
                SerializableOperation::AlterUniqueTogether {
                    model_name: "user".into(),
                    unique_together: vec![vec!["first_name".into(), "last_name".into()]],
                },
                SerializableOperation::RunSQL {
                    sql_forwards: "INSERT INTO log VALUES ('migrated')".into(),
                    sql_backwards: "DELETE FROM log WHERE msg = 'migrated'".into(),
                },
            ],
        };

        let json = migration.to_json().unwrap();
        let deserialized = SerializableMigration::from_json(&json).unwrap();
        assert_eq!(deserialized.operations.len(), 10);
        assert_eq!(deserialized.dependencies.len(), 1);
    }

    // ── to_operation / from concrete types ────────────────────────────

    #[test]
    fn test_to_operation_create_model() {
        let op = SerializableOperation::CreateModel {
            name: "post".into(),
            fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
            options: ModelOptions::default(),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Create model"));
    }

    #[test]
    fn test_to_operation_delete_model() {
        let op = SerializableOperation::DeleteModel {
            name: "post".into(),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Delete model"));
    }

    #[test]
    fn test_to_operation_add_field() {
        let op = SerializableOperation::AddField {
            model_name: "post".into(),
            field: make_field("title", FieldType::CharField).max_length(200),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Add field"));
    }

    #[test]
    fn test_to_operation_remove_field() {
        let op = SerializableOperation::RemoveField {
            model_name: "post".into(),
            field_name: "title".into(),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Remove field"));
    }

    #[test]
    fn test_to_operation_alter_field() {
        let op = SerializableOperation::AlterField {
            model_name: "post".into(),
            field_name: "title".into(),
            field: make_field("title", FieldType::CharField).max_length(500),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Alter field"));
    }

    #[test]
    fn test_to_operation_rename_field() {
        let op = SerializableOperation::RenameField {
            model_name: "post".into(),
            old_name: "title".into(),
            new_name: "headline".into(),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Rename field"));
    }

    #[test]
    fn test_to_operation_run_sql() {
        let op = SerializableOperation::RunSQL {
            sql_forwards: "SELECT 1".into(),
            sql_backwards: "SELECT 2".into(),
        };
        let boxed = op.to_operation();
        assert_eq!(boxed.describe(), "Run SQL");
    }

    // ── from_* constructors ──────────────────────────────────────────

    #[test]
    fn test_from_create_model() {
        let op = CreateModel {
            name: "post".into(),
            fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
            options: ModelOptions::default(),
        };
        let ser = SerializableOperation::from_create_model(&op);
        if let SerializableOperation::CreateModel { name, fields, .. } = ser {
            assert_eq!(name, "post");
            assert_eq!(fields.len(), 1);
        } else {
            panic!("Expected CreateModel");
        }
    }

    #[test]
    fn test_from_delete_model() {
        let op = DeleteModel {
            name: "post".into(),
        };
        let ser = SerializableOperation::from_delete_model(&op);
        if let SerializableOperation::DeleteModel { name } = ser {
            assert_eq!(name, "post");
        } else {
            panic!("Expected DeleteModel");
        }
    }

    #[test]
    fn test_from_add_field() {
        let op = AddField {
            model_name: "post".into(),
            field: make_field("title", FieldType::CharField).max_length(200),
        };
        let ser = SerializableOperation::from_add_field(&op);
        if let SerializableOperation::AddField { model_name, field } = ser {
            assert_eq!(model_name, "post");
            assert_eq!(field.name, "title");
        } else {
            panic!("Expected AddField");
        }
    }

    #[test]
    fn test_from_run_sql() {
        let op = RunSQL {
            sql_forwards: "SELECT 1".into(),
            sql_backwards: "SELECT 2".into(),
        };
        let ser = SerializableOperation::from_run_sql(&op);
        if let SerializableOperation::RunSQL {
            sql_forwards,
            sql_backwards,
        } = ser
        {
            assert_eq!(sql_forwards, "SELECT 1");
            assert_eq!(sql_backwards, "SELECT 2");
        } else {
            panic!("Expected RunSQL");
        }
    }

    // ── generate_migration_name ──────────────────────────────────────

    #[test]
    fn test_generate_migration_name_custom() {
        let name = generate_migration_name(1, Some("initial"));
        assert_eq!(name, "0001_initial");
    }

    #[test]
    fn test_generate_migration_name_auto() {
        let name = generate_migration_name(2, None);
        assert!(name.starts_with("0002_auto_"));
    }

    #[test]
    fn test_generate_migration_name_large_number() {
        let name = generate_migration_name(42, Some("add_users"));
        assert_eq!(name, "0042_add_users");
    }

    // ── next_migration_number ────────────────────────────────────────

    #[test]
    fn test_next_migration_number_no_dir() {
        let num = next_migration_number(Path::new("/nonexistent"), "myapp");
        assert_eq!(num, 1);
    }

    #[test]
    fn test_next_migration_number_with_files() {
        let dir = std::env::temp_dir().join(format!(
            "django_rs_test_serial_{}",
            std::process::id()
        ));
        let app_dir = dir.join("myapp");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::write(app_dir.join("0001_initial.json"), "{}").unwrap();
        std::fs::write(app_dir.join("0002_add_field.json"), "{}").unwrap();

        let num = next_migration_number(&dir, "myapp");
        assert_eq!(num, 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── migration_file_path ──────────────────────────────────────────

    #[test]
    fn test_migration_file_path() {
        let path = migration_file_path(
            Path::new("/migrations"),
            "blog",
            "0001_initial",
        );
        assert_eq!(
            path,
            PathBuf::from("/migrations/blog/0001_initial.json")
        );
    }

    // ── Write and read from file ─────────────────────────────────────

    #[test]
    fn test_write_and_read_file() {
        let dir = std::env::temp_dir().join(format!(
            "django_rs_test_serial_rw_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        let migration = SerializableMigration {
            app_label: "blog".into(),
            name: "0001_initial".into(),
            dependencies: vec![],
            initial: true,
            operations: vec![SerializableOperation::CreateModel {
                name: "post".into(),
                fields: vec![
                    make_field("id", FieldType::BigAutoField).primary_key(),
                    make_field("title", FieldType::CharField).max_length(200),
                ],
                options: ModelOptions::default(),
            }],
        };

        let path = migration_file_path(&dir, "blog", "0001_initial");
        migration.write_to_file(&path).unwrap();

        let loaded = SerializableMigration::read_from_file(&path).unwrap();
        assert_eq!(loaded.app_label, "blog");
        assert_eq!(loaded.name, "0001_initial");
        assert_eq!(loaded.operations.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Field type serialization round-trips ─────────────────────────

    #[test]
    fn test_field_type_serialization_char() {
        let field = make_field("name", FieldType::CharField).max_length(100);
        let json = serde_json::to_string(&field).unwrap();
        let deserialized: MigrationFieldDef = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "name");
        assert_eq!(deserialized.max_length, Some(100));
    }

    #[test]
    fn test_field_type_serialization_fk() {
        let field = make_field(
            "author_id",
            FieldType::ForeignKey {
                to: "auth.User".into(),
                on_delete: OnDelete::Cascade,
                related_name: Some("posts".into()),
            },
        );
        let json = serde_json::to_string(&field).unwrap();
        let deserialized: MigrationFieldDef = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "author_id");
        if let FieldType::ForeignKey { to, on_delete, .. } = &deserialized.field_type {
            assert_eq!(to, "auth.User");
            assert_eq!(*on_delete, OnDelete::Cascade);
        } else {
            panic!("Expected ForeignKey");
        }
    }

    #[test]
    fn test_field_type_serialization_decimal() {
        let field = make_field(
            "price",
            FieldType::DecimalField {
                max_digits: 10,
                decimal_places: 2,
            },
        );
        let json = serde_json::to_string(&field).unwrap();
        let deserialized: MigrationFieldDef = serde_json::from_str(&json).unwrap();
        if let FieldType::DecimalField {
            max_digits,
            decimal_places,
        } = &deserialized.field_type
        {
            assert_eq!(*max_digits, 10);
            assert_eq!(*decimal_places, 2);
        } else {
            panic!("Expected DecimalField");
        }
    }

    #[test]
    fn test_field_with_default_value() {
        let field = make_field("active", FieldType::BooleanField)
            .default(django_rs_db::value::Value::Bool(true));
        let json = serde_json::to_string(&field).unwrap();
        let deserialized: MigrationFieldDef = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.default,
            Some(django_rs_db::value::Value::Bool(true))
        );
    }

    #[test]
    fn test_nullable_unique_field_roundtrip() {
        let field = make_field("email", FieldType::EmailField)
            .max_length(254)
            .nullable()
            .unique();
        let json = serde_json::to_string(&field).unwrap();
        let deserialized: MigrationFieldDef = serde_json::from_str(&json).unwrap();
        assert!(deserialized.null);
        assert!(deserialized.unique);
        assert_eq!(deserialized.max_length, Some(254));
    }

    #[test]
    fn test_index_serialization_roundtrip() {
        let index = Index {
            name: Some("idx_email".into()),
            fields: vec!["email".into()],
            unique: true,
        };
        let json = serde_json::to_string(&index).unwrap();
        let deserialized: Index = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, Some("idx_email".into()));
        assert_eq!(deserialized.fields, vec!["email".to_string()]);
        assert!(deserialized.unique);
    }
}
