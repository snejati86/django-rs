//! Migration squashing to optimize multiple migrations into one.
//!
//! The [`MigrationSquasher`] combines a sequence of migrations into a single
//! optimized migration. This mirrors Django's `squashmigrations` command.

use crate::autodetect::{MigrationFieldDef, ModelOptions};
use crate::operations::{
    AddField, AddIndex, AlterField, AlterUniqueTogether, CreateModel, DeleteModel, Operation,
    RemoveField, RemoveIndex, RenameField, RunSQL,
};
use django_rs_db::model::Index;

/// Combines multiple migrations into a single optimized migration.
///
/// Optimizations performed:
/// - `CreateModel` + `AddField` on the same model -> merged `CreateModel`
/// - `CreateModel` + `DeleteModel` on the same model -> both removed
/// - `AddField` + `RemoveField` on the same field -> both removed
/// - `AddField` + `AlterField` on the same field -> `AddField` with new definition
/// - `AddField` + `RenameField` on the same field -> `AddField` with new name
/// - `AddIndex` + `RemoveIndex` on the same index -> both removed
pub struct MigrationSquasher;

impl MigrationSquasher {
    /// Squashes a sequence of operations into an optimized list.
    ///
    /// Takes all operations from multiple migrations and produces a minimal
    /// equivalent set.
    pub fn squash(operations: Vec<SquashableOp>) -> Vec<SquashableOp> {
        let mut result = operations;

        // Run optimization passes until stable
        loop {
            let before = result.len();
            result = Self::optimize_pass(result);
            if result.len() == before {
                break;
            }
        }

        result
    }

    /// Runs a single optimization pass.
    fn optimize_pass(operations: Vec<SquashableOp>) -> Vec<SquashableOp> {
        let mut result: Vec<SquashableOp> = Vec::new();

        for op in operations {
            let merged = Self::try_merge(&mut result, op);
            if let Some(remaining) = merged {
                result.push(remaining);
            }
        }

        result
    }

    /// Tries to merge an operation with the existing list.
    ///
    /// Returns `None` if the operation was merged, or `Some(op)` if it could
    /// not be merged and should be added to the list.
    fn try_merge(existing: &mut Vec<SquashableOp>, op: SquashableOp) -> Option<SquashableOp> {
        match &op {
            // DeleteModel cancels CreateModel
            SquashableOp::DeleteModel { name } => {
                let create_idx = existing.iter().position(
                    |e| matches!(e, SquashableOp::CreateModel { name: cn, .. } if cn == name),
                );
                if let Some(idx) = create_idx {
                    // Also remove any AddField / AlterField / RemoveField for this model
                    existing.remove(idx);
                    existing.retain(|e| {
                        !matches!(e, SquashableOp::AddField { model_name, .. }
                            | SquashableOp::RemoveField { model_name, .. }
                            | SquashableOp::AlterField { model_name, .. }
                            | SquashableOp::RenameField { model_name, .. }
                            | SquashableOp::AddIndex { model_name, .. }
                            | SquashableOp::RemoveIndex { model_name, .. }
                            | SquashableOp::AlterUniqueTogether { model_name, .. }
                            if model_name == name)
                    });
                    return None;
                }
            }

            // AddField merges into CreateModel
            SquashableOp::AddField {
                model_name, field, ..
            } => {
                let create_idx = existing.iter().position(
                    |e| matches!(e, SquashableOp::CreateModel { name, .. } if name == model_name),
                );
                if let Some(idx) = create_idx {
                    if let SquashableOp::CreateModel { fields, .. } = &mut existing[idx] {
                        fields.push(field.clone());
                    }
                    return None;
                }
            }

            // RemoveField cancels AddField or removes from CreateModel
            SquashableOp::RemoveField {
                model_name,
                field_name,
            } => {
                // Check standalone AddField first
                let add_idx = existing.iter().position(|e| {
                    matches!(e, SquashableOp::AddField { model_name: mn, field, .. }
                        if mn == model_name && field.name == *field_name)
                });
                if let Some(idx) = add_idx {
                    existing.remove(idx);
                    return None;
                }
                // Check inside CreateModel
                let create_idx = existing.iter().position(|e| {
                    matches!(e, SquashableOp::CreateModel { name, fields, .. }
                        if name == model_name && fields.iter().any(|f| f.name == *field_name))
                });
                if let Some(idx) = create_idx {
                    if let SquashableOp::CreateModel { fields, .. } = &mut existing[idx] {
                        fields.retain(|f| f.name != *field_name);
                    }
                    return None;
                }
            }

            // AlterField updates AddField or field in CreateModel in-place
            SquashableOp::AlterField {
                model_name,
                field_name,
                field,
            } => {
                // Check standalone AddField first
                let add_idx = existing.iter().position(|e| {
                    matches!(e, SquashableOp::AddField { model_name: mn, field: f, .. }
                        if mn == model_name && f.name == *field_name)
                });
                if let Some(idx) = add_idx {
                    if let SquashableOp::AddField {
                        field: existing_field,
                        ..
                    } = &mut existing[idx]
                    {
                        *existing_field = field.clone();
                    }
                    return None;
                }
                // Check inside CreateModel
                let create_idx = existing.iter().position(|e| {
                    matches!(e, SquashableOp::CreateModel { name, fields, .. }
                        if name == model_name && fields.iter().any(|f| f.name == *field_name))
                });
                if let Some(idx) = create_idx {
                    if let SquashableOp::CreateModel { fields, .. } = &mut existing[idx] {
                        if let Some(f) = fields.iter_mut().find(|f| f.name == *field_name) {
                            *f = field.clone();
                        }
                    }
                    return None;
                }
            }

            // RenameField updates AddField or field in CreateModel in-place
            SquashableOp::RenameField {
                model_name,
                old_name,
                new_name,
            } => {
                // Check standalone AddField first
                let add_idx = existing.iter().position(|e| {
                    matches!(e, SquashableOp::AddField { model_name: mn, field, .. }
                        if mn == model_name && field.name == *old_name)
                });
                if let Some(idx) = add_idx {
                    if let SquashableOp::AddField { field, .. } = &mut existing[idx] {
                        field.name.clone_from(new_name);
                        field.column.clone_from(new_name);
                    }
                    return None;
                }
                // Check inside CreateModel
                let create_idx = existing.iter().position(|e| {
                    matches!(e, SquashableOp::CreateModel { name, fields, .. }
                        if name == model_name && fields.iter().any(|f| f.name == *old_name))
                });
                if let Some(idx) = create_idx {
                    if let SquashableOp::CreateModel { fields, .. } = &mut existing[idx] {
                        if let Some(f) = fields.iter_mut().find(|f| f.name == *old_name) {
                            f.name.clone_from(new_name);
                            f.column.clone_from(new_name);
                        }
                    }
                    return None;
                }
            }

            // RemoveIndex cancels AddIndex
            SquashableOp::RemoveIndex {
                model_name,
                index_name,
            } => {
                let add_idx = existing.iter().position(|e| {
                    matches!(e, SquashableOp::AddIndex { model_name: mn, index, .. }
                        if mn == model_name && index.name.as_deref() == Some(index_name.as_str()))
                });
                if let Some(idx) = add_idx {
                    existing.remove(idx);
                    return None;
                }
            }

            _ => {}
        }

        Some(op)
    }
}

/// A squashable representation of a migration operation.
///
/// This enum mirrors the operation types but owns its data for squashing.
/// It can be converted to/from boxed `dyn Operation` for actual use.
#[derive(Debug, Clone)]
pub enum SquashableOp {
    /// Create a model.
    CreateModel {
        /// Model name.
        name: String,
        /// Fields.
        fields: Vec<MigrationFieldDef>,
        /// Options.
        options: ModelOptions,
    },
    /// Delete a model.
    DeleteModel {
        /// Model name.
        name: String,
    },
    /// Add a field.
    AddField {
        /// Model name.
        model_name: String,
        /// Field definition.
        field: MigrationFieldDef,
    },
    /// Remove a field.
    RemoveField {
        /// Model name.
        model_name: String,
        /// Field name.
        field_name: String,
    },
    /// Alter a field.
    AlterField {
        /// Model name.
        model_name: String,
        /// Field name.
        field_name: String,
        /// New field definition.
        field: MigrationFieldDef,
    },
    /// Rename a field.
    RenameField {
        /// Model name.
        model_name: String,
        /// Old name.
        old_name: String,
        /// New name.
        new_name: String,
    },
    /// Add an index.
    AddIndex {
        /// Model name.
        model_name: String,
        /// Index definition.
        index: Index,
    },
    /// Remove an index.
    RemoveIndex {
        /// Model name.
        model_name: String,
        /// Index name.
        index_name: String,
    },
    /// Alter unique_together.
    AlterUniqueTogether {
        /// Model name.
        model_name: String,
        /// New unique_together groups.
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

impl SquashableOp {
    /// Converts this squashable operation to a boxed `dyn Operation`.
    pub fn to_operation(self) -> Box<dyn Operation> {
        match self {
            SquashableOp::CreateModel {
                name,
                fields,
                options,
            } => Box::new(CreateModel {
                name,
                fields,
                options,
            }),
            SquashableOp::DeleteModel { name } => Box::new(DeleteModel { name }),
            SquashableOp::AddField { model_name, field } => {
                Box::new(AddField { model_name, field })
            }
            SquashableOp::RemoveField {
                model_name,
                field_name,
            } => Box::new(RemoveField {
                model_name,
                field_name,
            }),
            SquashableOp::AlterField {
                model_name,
                field_name,
                field,
            } => Box::new(AlterField {
                model_name,
                field_name,
                field,
            }),
            SquashableOp::RenameField {
                model_name,
                old_name,
                new_name,
            } => Box::new(RenameField {
                model_name,
                old_name,
                new_name,
            }),
            SquashableOp::AddIndex { model_name, index } => {
                Box::new(AddIndex { model_name, index })
            }
            SquashableOp::RemoveIndex {
                model_name,
                index_name,
            } => Box::new(RemoveIndex {
                model_name,
                index_name,
            }),
            SquashableOp::AlterUniqueTogether {
                model_name,
                unique_together,
            } => Box::new(AlterUniqueTogether {
                model_name,
                unique_together,
            }),
            SquashableOp::RunSQL {
                sql_forwards,
                sql_backwards,
            } => Box::new(RunSQL {
                sql_forwards,
                sql_backwards,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use django_rs_db::fields::FieldType;
    use django_rs_db::model::IndexType;

    fn make_field(name: &str, ft: FieldType) -> MigrationFieldDef {
        MigrationFieldDef::new(name, ft)
    }

    // ── CreateModel + AddField = merged CreateModel ─────────────────

    #[test]
    fn test_squash_create_model_add_field() {
        let ops = vec![
            SquashableOp::CreateModel {
                name: "post".into(),
                fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
                options: ModelOptions::default(),
            },
            SquashableOp::AddField {
                model_name: "post".into(),
                field: make_field("title", FieldType::CharField).max_length(200),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        assert_eq!(result.len(), 1);
        if let SquashableOp::CreateModel { fields, .. } = &result[0] {
            assert_eq!(fields.len(), 2);
        } else {
            panic!("Expected CreateModel");
        }
    }

    // ── CreateModel + DeleteModel = nothing ─────────────────────────

    #[test]
    fn test_squash_create_delete_model() {
        let ops = vec![
            SquashableOp::CreateModel {
                name: "temp".into(),
                fields: vec![],
                options: ModelOptions::default(),
            },
            SquashableOp::DeleteModel {
                name: "temp".into(),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        assert!(result.is_empty());
    }

    // ── AddField + RemoveField = nothing ────────────────────────────

    #[test]
    fn test_squash_add_remove_field() {
        let ops = vec![
            SquashableOp::AddField {
                model_name: "post".into(),
                field: make_field("temp", FieldType::CharField),
            },
            SquashableOp::RemoveField {
                model_name: "post".into(),
                field_name: "temp".into(),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        assert!(result.is_empty());
    }

    // ── AddField + AlterField = AddField with new definition ────────

    #[test]
    fn test_squash_add_alter_field() {
        let ops = vec![
            SquashableOp::AddField {
                model_name: "post".into(),
                field: make_field("title", FieldType::CharField).max_length(100),
            },
            SquashableOp::AlterField {
                model_name: "post".into(),
                field_name: "title".into(),
                field: make_field("title", FieldType::CharField).max_length(200),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        assert_eq!(result.len(), 1);
        if let SquashableOp::AddField { field, .. } = &result[0] {
            assert_eq!(field.max_length, Some(200));
        } else {
            panic!("Expected AddField");
        }
    }

    // ── AddField + RenameField = AddField with new name ─────────────

    #[test]
    fn test_squash_add_rename_field() {
        let ops = vec![
            SquashableOp::AddField {
                model_name: "post".into(),
                field: make_field("title", FieldType::CharField).max_length(200),
            },
            SquashableOp::RenameField {
                model_name: "post".into(),
                old_name: "title".into(),
                new_name: "headline".into(),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        assert_eq!(result.len(), 1);
        if let SquashableOp::AddField { field, .. } = &result[0] {
            assert_eq!(field.name, "headline");
        } else {
            panic!("Expected AddField");
        }
    }

    // ── AddIndex + RemoveIndex = nothing ────────────────────────────

    #[test]
    fn test_squash_add_remove_index() {
        let ops = vec![
            SquashableOp::AddIndex {
                model_name: "post".into(),
                index: Index {
                    name: Some("idx_title".into()),
                    fields: vec!["title".into()],
                    unique: false,
                    index_type: IndexType::default(),
                },
            },
            SquashableOp::RemoveIndex {
                model_name: "post".into(),
                index_name: "idx_title".into(),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        assert!(result.is_empty());
    }

    // ── No optimization needed ──────────────────────────────────────

    #[test]
    fn test_squash_no_optimization() {
        let ops = vec![
            SquashableOp::CreateModel {
                name: "post".into(),
                fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
                options: ModelOptions::default(),
            },
            SquashableOp::CreateModel {
                name: "comment".into(),
                fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
                options: ModelOptions::default(),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        assert_eq!(result.len(), 2);
    }

    // ── RunSQL is not optimized ─────────────────────────────────────

    #[test]
    fn test_squash_run_sql_preserved() {
        let ops = vec![SquashableOp::RunSQL {
            sql_forwards: "SELECT 1".into(),
            sql_backwards: "SELECT 2".into(),
        }];

        let result = MigrationSquasher::squash(ops);
        assert_eq!(result.len(), 1);
    }

    // ── Complex squash scenario ─────────────────────────────────────

    #[test]
    fn test_squash_complex() {
        let ops = vec![
            SquashableOp::CreateModel {
                name: "post".into(),
                fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
                options: ModelOptions::default(),
            },
            SquashableOp::AddField {
                model_name: "post".into(),
                field: make_field("title", FieldType::CharField).max_length(100),
            },
            SquashableOp::AlterField {
                model_name: "post".into(),
                field_name: "title".into(),
                field: make_field("title", FieldType::CharField).max_length(200),
            },
            SquashableOp::AddField {
                model_name: "post".into(),
                field: make_field("temp_field", FieldType::IntegerField),
            },
            SquashableOp::RemoveField {
                model_name: "post".into(),
                field_name: "temp_field".into(),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        // Should be: CreateModel with id + title(max_length=200)
        assert_eq!(result.len(), 1);
        if let SquashableOp::CreateModel { fields, .. } = &result[0] {
            assert_eq!(fields.len(), 2);
            let title_field = fields.iter().find(|f| f.name == "title").unwrap();
            assert_eq!(title_field.max_length, Some(200));
        } else {
            panic!("Expected CreateModel");
        }
    }

    // ── CreateModel + AddField + DeleteModel = nothing ──────────────

    #[test]
    fn test_squash_create_add_delete() {
        let ops = vec![
            SquashableOp::CreateModel {
                name: "temp".into(),
                fields: vec![],
                options: ModelOptions::default(),
            },
            SquashableOp::AddField {
                model_name: "temp".into(),
                field: make_field("x", FieldType::IntegerField),
            },
            SquashableOp::DeleteModel {
                name: "temp".into(),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        assert!(result.is_empty());
    }

    // ── to_operation conversion ─────────────────────────────────────

    #[test]
    fn test_to_operation_create_model() {
        let op = SquashableOp::CreateModel {
            name: "post".into(),
            fields: vec![],
            options: ModelOptions::default(),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Create model"));
    }

    #[test]
    fn test_to_operation_delete_model() {
        let op = SquashableOp::DeleteModel {
            name: "post".into(),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Delete model"));
    }

    #[test]
    fn test_to_operation_add_field() {
        let op = SquashableOp::AddField {
            model_name: "post".into(),
            field: make_field("title", FieldType::CharField),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Add field"));
    }

    #[test]
    fn test_to_operation_remove_field() {
        let op = SquashableOp::RemoveField {
            model_name: "post".into(),
            field_name: "title".into(),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Remove field"));
    }

    #[test]
    fn test_to_operation_alter_field() {
        let op = SquashableOp::AlterField {
            model_name: "post".into(),
            field_name: "title".into(),
            field: make_field("title", FieldType::CharField),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Alter field"));
    }

    #[test]
    fn test_to_operation_rename_field() {
        let op = SquashableOp::RenameField {
            model_name: "post".into(),
            old_name: "title".into(),
            new_name: "headline".into(),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Rename field"));
    }

    #[test]
    fn test_to_operation_add_index() {
        let op = SquashableOp::AddIndex {
            model_name: "post".into(),
            index: Index {
                name: Some("idx".into()),
                fields: vec!["title".into()],
                unique: false,
                index_type: IndexType::default(),
            },
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Add index"));
    }

    #[test]
    fn test_to_operation_remove_index() {
        let op = SquashableOp::RemoveIndex {
            model_name: "post".into(),
            index_name: "idx".into(),
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("Remove index"));
    }

    #[test]
    fn test_to_operation_alter_unique_together() {
        let op = SquashableOp::AlterUniqueTogether {
            model_name: "post".into(),
            unique_together: vec![vec!["a".into(), "b".into()]],
        };
        let boxed = op.to_operation();
        assert!(boxed.describe().contains("unique_together"));
    }

    #[test]
    fn test_to_operation_run_sql() {
        let op = SquashableOp::RunSQL {
            sql_forwards: "SELECT 1".into(),
            sql_backwards: "SELECT 2".into(),
        };
        let boxed = op.to_operation();
        assert_eq!(boxed.describe(), "Run SQL");
    }

    // ── Multiple AddField merge into CreateModel ────────────────────

    #[test]
    fn test_squash_multiple_add_fields_into_create() {
        let ops = vec![
            SquashableOp::CreateModel {
                name: "user".into(),
                fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
                options: ModelOptions::default(),
            },
            SquashableOp::AddField {
                model_name: "user".into(),
                field: make_field("name", FieldType::CharField).max_length(100),
            },
            SquashableOp::AddField {
                model_name: "user".into(),
                field: make_field("email", FieldType::EmailField).max_length(254),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        assert_eq!(result.len(), 1);
        if let SquashableOp::CreateModel { fields, .. } = &result[0] {
            assert_eq!(fields.len(), 3);
        } else {
            panic!("Expected CreateModel");
        }
    }

    // ── DeleteModel only removes operations for that model ──────────

    #[test]
    fn test_squash_delete_doesnt_affect_other_models() {
        let ops = vec![
            SquashableOp::CreateModel {
                name: "post".into(),
                fields: vec![],
                options: ModelOptions::default(),
            },
            SquashableOp::CreateModel {
                name: "temp".into(),
                fields: vec![],
                options: ModelOptions::default(),
            },
            SquashableOp::DeleteModel {
                name: "temp".into(),
            },
        ];

        let result = MigrationSquasher::squash(ops);
        assert_eq!(result.len(), 1);
        if let SquashableOp::CreateModel { name, .. } = &result[0] {
            assert_eq!(name, "post");
        } else {
            panic!("Expected CreateModel for post");
        }
    }
}
