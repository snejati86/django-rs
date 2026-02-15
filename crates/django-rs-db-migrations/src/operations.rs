//! Migration operations that describe schema changes.
//!
//! Each operation knows how to apply itself forwards and backwards. Operations
//! are the fundamental building blocks of migrations, corresponding to
//! Django's `django.db.migrations.operations`.

use django_rs_core::DjangoError;
use django_rs_db::model::Index;

use crate::autodetect::{MigrationFieldDef, ModelOptions, ModelState, ProjectState};
use crate::schema_editor::SchemaEditor;

/// A single migration operation that can be applied forwards or backwards.
///
/// Operations modify both the in-memory project state and produce DDL SQL
/// for the database schema.
pub trait Operation: Send + Sync {
    /// Returns a human-readable description of this operation.
    fn describe(&self) -> String;

    /// Applies this operation to the in-memory project state (forward direction).
    fn state_forwards(&self, app_label: &str, state: &mut ProjectState);

    /// Generates the DDL SQL to apply this operation (forward direction).
    fn database_forwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        from_state: &ProjectState,
        to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError>;

    /// Generates the DDL SQL to reverse this operation (backward direction).
    fn database_backwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        from_state: &ProjectState,
        to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError>;

    /// Returns whether this operation is reversible.
    fn reversible(&self) -> bool;
}

/// Creates a new database table.
///
/// Generates a `CREATE TABLE` statement with all specified fields and
/// constraints.
#[derive(Debug, Clone)]
pub struct CreateModel {
    /// The model name.
    pub name: String,
    /// The fields for the new table.
    pub fields: Vec<MigrationFieldDef>,
    /// Model-level options (indexes, unique_together, etc.).
    pub options: ModelOptions,
}

impl Operation for CreateModel {
    fn describe(&self) -> String {
        format!("Create model {}", self.name)
    }

    fn state_forwards(&self, app_label: &str, state: &mut ProjectState) {
        let model = ModelState {
            app_label: app_label.to_string(),
            name: self.name.clone(),
            fields: self.fields.clone(),
            options: self.options.clone(),
        };
        state.add_model(model);
    }

    fn database_forwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let key = (app_label.to_string(), self.name.clone());
        let model = to_state
            .models
            .get(&key)
            .ok_or_else(|| DjangoError::DatabaseError(format!("Model {} not found in state", self.name)))?;
        Ok(schema_editor.create_table(model))
    }

    fn database_backwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let table_name = format!("{app_label}_{}", self.name);
        Ok(schema_editor.drop_table(&table_name))
    }

    fn reversible(&self) -> bool {
        true
    }
}

/// Drops a database table.
///
/// Generates a `DROP TABLE` statement. This is reversible only if
/// the model state is available in the "from" state.
#[derive(Debug, Clone)]
pub struct DeleteModel {
    /// The model name to delete.
    pub name: String,
}

impl Operation for DeleteModel {
    fn describe(&self) -> String {
        format!("Delete model {}", self.name)
    }

    fn state_forwards(&self, app_label: &str, state: &mut ProjectState) {
        let key = (app_label.to_string(), self.name.clone());
        state.models.remove(&key);
    }

    fn database_forwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let table_name = format!("{app_label}_{}", self.name);
        Ok(schema_editor.drop_table(&table_name))
    }

    fn database_backwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let key = (app_label.to_string(), self.name.clone());
        let model = from_state
            .models
            .get(&key)
            .ok_or_else(|| DjangoError::DatabaseError(format!("Model {} not found in from_state", self.name)))?;
        Ok(schema_editor.create_table(model))
    }

    fn reversible(&self) -> bool {
        true
    }
}

/// Adds a column to an existing table.
///
/// Generates an `ALTER TABLE ... ADD COLUMN` statement.
#[derive(Debug, Clone)]
pub struct AddField {
    /// The model name the field is being added to.
    pub model_name: String,
    /// The field to add.
    pub field: MigrationFieldDef,
}

impl Operation for AddField {
    fn describe(&self) -> String {
        format!("Add field {} to {}", self.field.name, self.model_name)
    }

    fn state_forwards(&self, app_label: &str, state: &mut ProjectState) {
        let key = (app_label.to_string(), self.model_name.clone());
        if let Some(model) = state.models.get_mut(&key) {
            model.fields.push(self.field.clone());
        }
    }

    fn database_forwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let table_name = format!("{app_label}_{}", self.model_name);
        let fd = self.field.to_field_def();
        Ok(schema_editor.add_column(&table_name, &fd))
    }

    fn database_backwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let table_name = format!("{app_label}_{}", self.model_name);
        Ok(schema_editor.drop_column(&table_name, &self.field.column))
    }

    fn reversible(&self) -> bool {
        true
    }
}

/// Removes a column from an existing table.
///
/// Generates an `ALTER TABLE ... DROP COLUMN` statement.
#[derive(Debug, Clone)]
pub struct RemoveField {
    /// The model name the field is being removed from.
    pub model_name: String,
    /// The name of the field to remove.
    pub field_name: String,
}

impl Operation for RemoveField {
    fn describe(&self) -> String {
        format!("Remove field {} from {}", self.field_name, self.model_name)
    }

    fn state_forwards(&self, app_label: &str, state: &mut ProjectState) {
        let key = (app_label.to_string(), self.model_name.clone());
        if let Some(model) = state.models.get_mut(&key) {
            model.fields.retain(|f| f.name != self.field_name);
        }
    }

    fn database_forwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let table_name = format!("{app_label}_{}", self.model_name);
        Ok(schema_editor.drop_column(&table_name, &self.field_name))
    }

    fn database_backwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let key = (app_label.to_string(), self.model_name.clone());
        let model = from_state
            .models
            .get(&key)
            .ok_or_else(|| DjangoError::DatabaseError("Model not found".into()))?;
        let field = model
            .fields
            .iter()
            .find(|f| f.name == self.field_name)
            .ok_or_else(|| DjangoError::DatabaseError("Field not found".into()))?;
        let table_name = format!("{app_label}_{}", self.model_name);
        let fd = field.to_field_def();
        Ok(schema_editor.add_column(&table_name, &fd))
    }

    fn reversible(&self) -> bool {
        true
    }
}

/// Alters a column on an existing table.
///
/// Generates `ALTER TABLE ... ALTER COLUMN` or equivalent DDL.
/// On SQLite this triggers a table recreation.
#[derive(Debug, Clone)]
pub struct AlterField {
    /// The model name containing the field.
    pub model_name: String,
    /// The name of the field being altered.
    pub field_name: String,
    /// The new field definition.
    pub field: MigrationFieldDef,
}

impl Operation for AlterField {
    fn describe(&self) -> String {
        format!(
            "Alter field {} on {}",
            self.field_name, self.model_name
        )
    }

    fn state_forwards(&self, app_label: &str, state: &mut ProjectState) {
        let key = (app_label.to_string(), self.model_name.clone());
        if let Some(model) = state.models.get_mut(&key) {
            if let Some(f) = model.fields.iter_mut().find(|f| f.name == self.field_name) {
                *f = self.field.clone();
            }
        }
    }

    fn database_forwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let table_name = format!("{app_label}_{}", self.model_name);
        let key = (app_label.to_string(), self.model_name.clone());
        let old_model = from_state
            .models
            .get(&key)
            .ok_or_else(|| DjangoError::DatabaseError("Model not found".into()))?;
        let old_field = old_model
            .fields
            .iter()
            .find(|f| f.name == self.field_name)
            .ok_or_else(|| DjangoError::DatabaseError("Old field not found".into()))?;
        let old_fd = old_field.to_field_def();
        let new_fd = self.field.to_field_def();
        Ok(schema_editor.alter_column(&table_name, &old_fd, &new_fd))
    }

    fn database_backwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        // Reverse: apply the old field definition
        let table_name = format!("{app_label}_{}", self.model_name);
        let key = (app_label.to_string(), self.model_name.clone());
        let old_model = from_state
            .models
            .get(&key)
            .ok_or_else(|| DjangoError::DatabaseError("Model not found".into()))?;
        let old_field = old_model
            .fields
            .iter()
            .find(|f| f.name == self.field_name)
            .ok_or_else(|| DjangoError::DatabaseError("Old field not found".into()))?;
        let new_fd = self.field.to_field_def();
        let old_fd = old_field.to_field_def();
        Ok(schema_editor.alter_column(&table_name, &new_fd, &old_fd))
    }

    fn reversible(&self) -> bool {
        true
    }
}

/// Renames a column on an existing table.
///
/// Generates `ALTER TABLE ... RENAME COLUMN` DDL.
#[derive(Debug, Clone)]
pub struct RenameField {
    /// The model name containing the field.
    pub model_name: String,
    /// The old field name.
    pub old_name: String,
    /// The new field name.
    pub new_name: String,
}

impl Operation for RenameField {
    fn describe(&self) -> String {
        format!(
            "Rename field {} to {} on {}",
            self.old_name, self.new_name, self.model_name
        )
    }

    fn state_forwards(&self, app_label: &str, state: &mut ProjectState) {
        let key = (app_label.to_string(), self.model_name.clone());
        if let Some(model) = state.models.get_mut(&key) {
            if let Some(f) = model.fields.iter_mut().find(|f| f.name == self.old_name) {
                f.name.clone_from(&self.new_name);
                f.column.clone_from(&self.new_name);
            }
        }
    }

    fn database_forwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let table_name = format!("{app_label}_{}", self.model_name);
        Ok(schema_editor.rename_column(&table_name, &self.old_name, &self.new_name))
    }

    fn database_backwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let table_name = format!("{app_label}_{}", self.model_name);
        Ok(schema_editor.rename_column(&table_name, &self.new_name, &self.old_name))
    }

    fn reversible(&self) -> bool {
        true
    }
}

/// Adds an index to a table.
///
/// Generates a `CREATE INDEX` statement.
#[derive(Debug, Clone)]
pub struct AddIndex {
    /// The model name the index is for.
    pub model_name: String,
    /// The index definition.
    pub index: Index,
}

impl Operation for AddIndex {
    fn describe(&self) -> String {
        format!(
            "Add index {} on {}",
            self.index.name.as_deref().unwrap_or("unnamed"),
            self.model_name
        )
    }

    fn state_forwards(&self, app_label: &str, state: &mut ProjectState) {
        let key = (app_label.to_string(), self.model_name.clone());
        if let Some(model) = state.models.get_mut(&key) {
            model.options.indexes.push(self.index.clone());
        }
    }

    fn database_forwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let table_name = format!("{app_label}_{}", self.model_name);
        Ok(schema_editor.create_index(&table_name, &self.index))
    }

    fn database_backwards(
        &self,
        _app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let idx_name = self
            .index
            .name
            .as_deref()
            .unwrap_or("unnamed_index");
        Ok(schema_editor.drop_index(idx_name))
    }

    fn reversible(&self) -> bool {
        true
    }
}

/// Removes an index from a table.
///
/// Generates a `DROP INDEX` statement.
#[derive(Debug, Clone)]
pub struct RemoveIndex {
    /// The model name the index belongs to.
    pub model_name: String,
    /// The name of the index to remove.
    pub index_name: String,
}

impl Operation for RemoveIndex {
    fn describe(&self) -> String {
        format!("Remove index {} from {}", self.index_name, self.model_name)
    }

    fn state_forwards(&self, app_label: &str, state: &mut ProjectState) {
        let key = (app_label.to_string(), self.model_name.clone());
        if let Some(model) = state.models.get_mut(&key) {
            model
                .options
                .indexes
                .retain(|i| i.name.as_deref() != Some(&self.index_name));
        }
    }

    fn database_forwards(
        &self,
        _app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        Ok(schema_editor.drop_index(&self.index_name))
    }

    fn database_backwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let key = (app_label.to_string(), self.model_name.clone());
        let model = from_state
            .models
            .get(&key)
            .ok_or_else(|| DjangoError::DatabaseError("Model not found".into()))?;
        let index = model
            .options
            .indexes
            .iter()
            .find(|i| i.name.as_deref() == Some(&self.index_name))
            .ok_or_else(|| DjangoError::DatabaseError("Index not found".into()))?;
        let table_name = format!("{app_label}_{}", self.model_name);
        Ok(schema_editor.create_index(&table_name, index))
    }

    fn reversible(&self) -> bool {
        true
    }
}

/// Alters the `unique_together` constraint set on a model.
///
/// Drops old unique constraints and creates new ones.
#[derive(Debug, Clone)]
pub struct AlterUniqueTogether {
    /// The model name.
    pub model_name: String,
    /// The new set of `unique_together` field groups.
    pub unique_together: Vec<Vec<String>>,
}

impl Operation for AlterUniqueTogether {
    fn describe(&self) -> String {
        format!("Alter unique_together for {}", self.model_name)
    }

    fn state_forwards(&self, app_label: &str, state: &mut ProjectState) {
        let key = (app_label.to_string(), self.model_name.clone());
        if let Some(model) = state.models.get_mut(&key) {
            model.options.unique_together.clone_from(&self.unique_together);
        }
    }

    fn database_forwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        let table_name = format!("{app_label}_{}", self.model_name);
        let mut sqls = Vec::new();
        for group in &self.unique_together {
            let cols: Vec<&str> = group.iter().map(String::as_str).collect();
            sqls.extend(schema_editor.add_unique_constraint(&table_name, &cols));
        }
        Ok(sqls)
    }

    fn database_backwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        // Reverse: re-apply the old unique_together
        let key = (app_label.to_string(), self.model_name.clone());
        let table_name = format!("{app_label}_{}", self.model_name);
        let mut sqls = Vec::new();
        if let Some(model) = from_state.models.get(&key) {
            for group in &model.options.unique_together {
                let cols: Vec<&str> = group.iter().map(String::as_str).collect();
                sqls.extend(schema_editor.add_unique_constraint(&table_name, &cols));
            }
        }
        Ok(sqls)
    }

    fn reversible(&self) -> bool {
        true
    }
}

/// Runs raw SQL in a migration.
///
/// Both forward and backward SQL must be provided for reversibility.
#[derive(Debug, Clone)]
pub struct RunSQL {
    /// SQL to run in the forward direction.
    pub sql_forwards: String,
    /// SQL to run in the backward direction (empty string = irreversible).
    pub sql_backwards: String,
}

impl Operation for RunSQL {
    fn describe(&self) -> String {
        "Run SQL".to_string()
    }

    fn state_forwards(&self, _app_label: &str, _state: &mut ProjectState) {
        // Raw SQL does not change the project state
    }

    fn database_forwards(
        &self,
        _app_label: &str,
        _schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        Ok(vec![self.sql_forwards.clone()])
    }

    fn database_backwards(
        &self,
        _app_label: &str,
        _schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        if self.sql_backwards.is_empty() {
            Err(DjangoError::DatabaseError(
                "RunSQL operation is not reversible (no backwards SQL provided)".into(),
            ))
        } else {
            Ok(vec![self.sql_backwards.clone()])
        }
    }

    fn reversible(&self) -> bool {
        !self.sql_backwards.is_empty()
    }
}

/// Type alias for the closure type used in `RunRust` operations.
pub type RustMigrationFn = Box<dyn Fn() -> Result<(), DjangoError> + Send + Sync>;

/// Runs arbitrary Rust code in a migration.
///
/// The closures are executed during migration application / reversal.
pub struct RunRust {
    /// A description of what this code does.
    pub description: String,
    /// The forward closure.
    pub forwards: RustMigrationFn,
    /// The backward closure (None = irreversible).
    pub backwards: Option<RustMigrationFn>,
}

impl Operation for RunRust {
    fn describe(&self) -> String {
        format!("Run Rust: {}", self.description)
    }

    fn state_forwards(&self, _app_label: &str, _state: &mut ProjectState) {
        // Rust code does not change the project state
    }

    fn database_forwards(
        &self,
        _app_label: &str,
        _schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        (self.forwards)()?;
        Ok(vec![])
    }

    fn database_backwards(
        &self,
        _app_label: &str,
        _schema_editor: &dyn SchemaEditor,
        _from_state: &ProjectState,
        _to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError> {
        match &self.backwards {
            Some(f) => {
                f()?;
                Ok(vec![])
            }
            None => Err(DjangoError::DatabaseError(
                "RunRust operation is not reversible".into(),
            )),
        }
    }

    fn reversible(&self) -> bool {
        self.backwards.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autodetect::MigrationFieldDef;
    use crate::schema_editor::PostgresSchemaEditor;
    use django_rs_db::fields::FieldType;
    use django_rs_db::model::IndexType;

    fn pg_editor() -> PostgresSchemaEditor {
        PostgresSchemaEditor
    }

    fn make_field(name: &str, ft: FieldType) -> MigrationFieldDef {
        MigrationFieldDef::new(name, ft)
    }

    // ── CreateModel ─────────────────────────────────────────────────

    #[test]
    fn test_create_model_describe() {
        let op = CreateModel {
            name: "post".into(),
            fields: vec![],
            options: ModelOptions::default(),
        };
        assert_eq!(op.describe(), "Create model post");
    }

    #[test]
    fn test_create_model_state_forwards() {
        let op = CreateModel {
            name: "post".into(),
            fields: vec![make_field("id", FieldType::BigAutoField).primary_key()],
            options: ModelOptions::default(),
        };
        let mut state = ProjectState::new();
        op.state_forwards("blog", &mut state);
        assert!(state.models.contains_key(&("blog".into(), "post".into())));
    }

    #[test]
    fn test_create_model_database_forwards() {
        let op = CreateModel {
            name: "post".into(),
            fields: vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200),
            ],
            options: ModelOptions::default(),
        };
        let mut state = ProjectState::new();
        op.state_forwards("blog", &mut state);
        let sqls = op
            .database_forwards("blog", &pg_editor(), &ProjectState::new(), &state)
            .unwrap();
        assert!(!sqls.is_empty());
        assert!(sqls[0].contains("CREATE TABLE"));
    }

    #[test]
    fn test_create_model_reversible() {
        let op = CreateModel {
            name: "post".into(),
            fields: vec![],
            options: ModelOptions::default(),
        };
        assert!(op.reversible());
    }

    #[test]
    fn test_create_model_database_backwards() {
        let op = CreateModel {
            name: "post".into(),
            fields: vec![],
            options: ModelOptions::default(),
        };
        let sqls = op
            .database_backwards("blog", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert!(!sqls.is_empty());
        assert!(sqls[0].contains("DROP TABLE"));
    }

    // ── DeleteModel ─────────────────────────────────────────────────

    #[test]
    fn test_delete_model_describe() {
        let op = DeleteModel {
            name: "post".into(),
        };
        assert_eq!(op.describe(), "Delete model post");
    }

    #[test]
    fn test_delete_model_state_forwards() {
        let mut state = ProjectState::new();
        state.add_model(ModelState::new("blog", "post", vec![]));
        let op = DeleteModel {
            name: "post".into(),
        };
        op.state_forwards("blog", &mut state);
        assert!(!state.models.contains_key(&("blog".into(), "post".into())));
    }

    #[test]
    fn test_delete_model_database_forwards() {
        let op = DeleteModel {
            name: "post".into(),
        };
        let sqls = op
            .database_forwards("blog", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert!(sqls[0].contains("DROP TABLE"));
    }

    // ── AddField ────────────────────────────────────────────────────

    #[test]
    fn test_add_field_describe() {
        let op = AddField {
            model_name: "post".into(),
            field: make_field("title", FieldType::CharField),
        };
        assert_eq!(op.describe(), "Add field title to post");
    }

    #[test]
    fn test_add_field_state_forwards() {
        let mut state = ProjectState::new();
        state.add_model(ModelState::new("blog", "post", vec![]));
        let op = AddField {
            model_name: "post".into(),
            field: make_field("title", FieldType::CharField),
        };
        op.state_forwards("blog", &mut state);
        let model = state.models.get(&("blog".into(), "post".into())).unwrap();
        assert_eq!(model.fields.len(), 1);
    }

    #[test]
    fn test_add_field_database_forwards() {
        let op = AddField {
            model_name: "post".into(),
            field: make_field("title", FieldType::CharField).max_length(200),
        };
        let sqls = op
            .database_forwards("blog", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert!(sqls[0].contains("ALTER TABLE"));
        assert!(sqls[0].contains("ADD COLUMN"));
    }

    // ── RemoveField ─────────────────────────────────────────────────

    #[test]
    fn test_remove_field_describe() {
        let op = RemoveField {
            model_name: "post".into(),
            field_name: "title".into(),
        };
        assert_eq!(op.describe(), "Remove field title from post");
    }

    #[test]
    fn test_remove_field_state_forwards() {
        let mut state = ProjectState::new();
        state.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("title", FieldType::CharField)],
        ));
        let op = RemoveField {
            model_name: "post".into(),
            field_name: "title".into(),
        };
        op.state_forwards("blog", &mut state);
        let model = state.models.get(&("blog".into(), "post".into())).unwrap();
        assert!(model.fields.is_empty());
    }

    #[test]
    fn test_remove_field_database_forwards() {
        let op = RemoveField {
            model_name: "post".into(),
            field_name: "title".into(),
        };
        let sqls = op
            .database_forwards("blog", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert!(sqls[0].contains("ALTER TABLE"));
        assert!(sqls[0].contains("DROP COLUMN"));
    }

    // ── AlterField ──────────────────────────────────────────────────

    #[test]
    fn test_alter_field_describe() {
        let op = AlterField {
            model_name: "post".into(),
            field_name: "title".into(),
            field: make_field("title", FieldType::CharField).max_length(500),
        };
        assert_eq!(op.describe(), "Alter field title on post");
    }

    #[test]
    fn test_alter_field_state_forwards() {
        let mut state = ProjectState::new();
        state.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("title", FieldType::CharField).max_length(200)],
        ));
        let op = AlterField {
            model_name: "post".into(),
            field_name: "title".into(),
            field: make_field("title", FieldType::CharField).max_length(500),
        };
        op.state_forwards("blog", &mut state);
        let model = state.models.get(&("blog".into(), "post".into())).unwrap();
        assert_eq!(model.fields[0].max_length, Some(500));
    }

    // ── RenameField ─────────────────────────────────────────────────

    #[test]
    fn test_rename_field_describe() {
        let op = RenameField {
            model_name: "post".into(),
            old_name: "title".into(),
            new_name: "headline".into(),
        };
        assert_eq!(op.describe(), "Rename field title to headline on post");
    }

    #[test]
    fn test_rename_field_state_forwards() {
        let mut state = ProjectState::new();
        state.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("title", FieldType::CharField)],
        ));
        let op = RenameField {
            model_name: "post".into(),
            old_name: "title".into(),
            new_name: "headline".into(),
        };
        op.state_forwards("blog", &mut state);
        let model = state.models.get(&("blog".into(), "post".into())).unwrap();
        assert_eq!(model.fields[0].name, "headline");
    }

    #[test]
    fn test_rename_field_database_forwards() {
        let op = RenameField {
            model_name: "post".into(),
            old_name: "title".into(),
            new_name: "headline".into(),
        };
        let sqls = op
            .database_forwards("blog", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert!(sqls[0].contains("RENAME COLUMN"));
    }

    #[test]
    fn test_rename_field_database_backwards() {
        let op = RenameField {
            model_name: "post".into(),
            old_name: "title".into(),
            new_name: "headline".into(),
        };
        let sqls = op
            .database_backwards("blog", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert!(sqls[0].contains("RENAME COLUMN"));
        assert!(sqls[0].contains("headline"));
        assert!(sqls[0].contains("title"));
    }

    // ── AddIndex ────────────────────────────────────────────────────

    #[test]
    fn test_add_index_describe() {
        let op = AddIndex {
            model_name: "post".into(),
            index: Index {
                name: Some("idx_title".into()),
                fields: vec!["title".into()],
                unique: false,
                    index_type: IndexType::default(),
            },
        };
        assert_eq!(op.describe(), "Add index idx_title on post");
    }

    #[test]
    fn test_add_index_database_forwards() {
        let op = AddIndex {
            model_name: "post".into(),
            index: Index {
                name: Some("idx_title".into()),
                fields: vec!["title".into()],
                unique: false,
                    index_type: IndexType::default(),
            },
        };
        let sqls = op
            .database_forwards("blog", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert!(sqls[0].contains("CREATE INDEX"));
    }

    // ── RemoveIndex ─────────────────────────────────────────────────

    #[test]
    fn test_remove_index_describe() {
        let op = RemoveIndex {
            model_name: "post".into(),
            index_name: "idx_title".into(),
        };
        assert_eq!(op.describe(), "Remove index idx_title from post");
    }

    #[test]
    fn test_remove_index_database_forwards() {
        let op = RemoveIndex {
            model_name: "post".into(),
            index_name: "idx_title".into(),
        };
        let sqls = op
            .database_forwards("blog", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert!(sqls[0].contains("DROP INDEX"));
    }

    // ── AlterUniqueTogether ─────────────────────────────────────────

    #[test]
    fn test_alter_unique_together_describe() {
        let op = AlterUniqueTogether {
            model_name: "post".into(),
            unique_together: vec![vec!["author".into(), "slug".into()]],
        };
        assert!(op.describe().contains("unique_together"));
    }

    #[test]
    fn test_alter_unique_together_database_forwards() {
        let op = AlterUniqueTogether {
            model_name: "post".into(),
            unique_together: vec![vec!["author".into(), "slug".into()]],
        };
        let sqls = op
            .database_forwards("blog", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert!(!sqls.is_empty());
        assert!(sqls[0].contains("UNIQUE"));
    }

    // ── RunSQL ──────────────────────────────────────────────────────

    #[test]
    fn test_run_sql_describe() {
        let op = RunSQL {
            sql_forwards: "SELECT 1".into(),
            sql_backwards: "SELECT 2".into(),
        };
        assert_eq!(op.describe(), "Run SQL");
    }

    #[test]
    fn test_run_sql_reversible() {
        let op = RunSQL {
            sql_forwards: "SELECT 1".into(),
            sql_backwards: "SELECT 2".into(),
        };
        assert!(op.reversible());
    }

    #[test]
    fn test_run_sql_irreversible() {
        let op = RunSQL {
            sql_forwards: "SELECT 1".into(),
            sql_backwards: String::new(),
        };
        assert!(!op.reversible());
    }

    #[test]
    fn test_run_sql_database_forwards() {
        let op = RunSQL {
            sql_forwards: "INSERT INTO log VALUES (1)".into(),
            sql_backwards: "DELETE FROM log WHERE id = 1".into(),
        };
        let sqls = op
            .database_forwards("app", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert_eq!(sqls, vec!["INSERT INTO log VALUES (1)"]);
    }

    #[test]
    fn test_run_sql_database_backwards_irreversible() {
        let op = RunSQL {
            sql_forwards: "SELECT 1".into(),
            sql_backwards: String::new(),
        };
        let result = op.database_backwards("app", &pg_editor(), &ProjectState::new(), &ProjectState::new());
        assert!(result.is_err());
    }

    // ── RunRust ─────────────────────────────────────────────────────

    #[test]
    fn test_run_rust_describe() {
        let op = RunRust {
            description: "Seed initial data".into(),
            forwards: Box::new(|| Ok(())),
            backwards: None,
        };
        assert_eq!(op.describe(), "Run Rust: Seed initial data");
    }

    #[test]
    fn test_run_rust_reversible() {
        let op = RunRust {
            description: "test".into(),
            forwards: Box::new(|| Ok(())),
            backwards: Some(Box::new(|| Ok(()))),
        };
        assert!(op.reversible());
    }

    #[test]
    fn test_run_rust_irreversible() {
        let op = RunRust {
            description: "test".into(),
            forwards: Box::new(|| Ok(())),
            backwards: None,
        };
        assert!(!op.reversible());
    }

    #[test]
    fn test_run_rust_database_forwards() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let called = Arc::new(AtomicBool::new(false));
        let called2 = called.clone();
        let op = RunRust {
            description: "test".into(),
            forwards: Box::new(move || {
                called2.store(true, Ordering::SeqCst);
                Ok(())
            }),
            backwards: None,
        };
        let sqls = op
            .database_forwards("app", &pg_editor(), &ProjectState::new(), &ProjectState::new())
            .unwrap();
        assert!(sqls.is_empty());
        assert!(called.load(Ordering::SeqCst));
    }
}
