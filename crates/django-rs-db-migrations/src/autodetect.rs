//! Migration auto-detection by comparing project states.
//!
//! The [`MigrationAutodetector`] compares an "old" [`ProjectState`] against a "new"
//! [`ProjectState`] and generates the minimal set of [`Operation`]s needed to
//! transform one into the other. This mirrors Django's `MigrationAutodetector`.

use std::collections::HashMap;

use django_rs_db::fields::FieldType;
use django_rs_db::model::Index;
use django_rs_db::value::Value;

use crate::operations::{
    AddField, AddIndex, AlterField, AlterUniqueTogether, CreateModel, DeleteModel, Operation,
    RemoveField, RemoveIndex, RenameField,
};

/// A snapshot of the entire project's model state at a point in time.
///
/// Contains all models across all apps, keyed by `(app_label, model_name)`.
/// This is the fundamental input to the autodetector.
#[derive(Debug, Clone, Default)]
pub struct ProjectState {
    /// All models in the project, keyed by `(app_label, model_name)`.
    pub models: HashMap<(String, String), ModelState>,
}

impl ProjectState {
    /// Creates a new empty project state.
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }

    /// Adds a model to this project state.
    pub fn add_model(&mut self, model: ModelState) {
        let key = (model.app_label.clone(), model.name.clone());
        self.models.insert(key, model);
    }
}

/// Options for a model, equivalent to Django's `class Meta`.
#[derive(Debug, Clone, Default)]
pub struct ModelOptions {
    /// The explicit database table name, if set.
    pub db_table: Option<String>,
    /// Sets of fields that must be unique together.
    pub unique_together: Vec<Vec<String>>,
    /// Database indexes.
    pub indexes: Vec<Index>,
}

/// The state of a single model at a point in time.
///
/// This is a migration-friendly representation of a model that does not
/// require the full `FieldDef` (which contains non-cloneable validators).
/// Instead it uses [`MigrationFieldDef`] which captures all schema-relevant
/// information.
#[derive(Debug, Clone)]
pub struct ModelState {
    /// The application label this model belongs to.
    pub app_label: String,
    /// The model name (lowercase).
    pub name: String,
    /// The fields of this model.
    pub fields: Vec<MigrationFieldDef>,
    /// Model-level options.
    pub options: ModelOptions,
}

impl ModelState {
    /// Creates a new model state.
    pub fn new(
        app_label: impl Into<String>,
        name: impl Into<String>,
        fields: Vec<MigrationFieldDef>,
    ) -> Self {
        Self {
            app_label: app_label.into(),
            name: name.into(),
            fields,
            options: ModelOptions::default(),
        }
    }

    /// Sets model options.
    pub fn with_options(mut self, options: ModelOptions) -> Self {
        self.options = options;
        self
    }

    /// Returns the database table name for this model.
    pub fn db_table(&self) -> String {
        self.options
            .db_table
            .clone()
            .unwrap_or_else(|| format!("{}_{}", self.app_label, self.name))
    }
}

/// A migration-friendly field definition.
///
/// Unlike [`django_rs_db::fields::FieldDef`], this struct is fully `Clone`-able
/// because it omits validators (which are runtime objects). It captures all
/// information needed for schema generation.
#[derive(Debug, Clone)]
pub struct MigrationFieldDef {
    /// The field name.
    pub name: String,
    /// The database column name.
    pub column: String,
    /// The field type.
    pub field_type: FieldType,
    /// Whether this field is the primary key.
    pub primary_key: bool,
    /// Whether NULL is allowed.
    pub null: bool,
    /// Default value.
    pub default: Option<Value>,
    /// Whether a UNIQUE constraint is applied.
    pub unique: bool,
    /// Whether a database index should be created.
    pub db_index: bool,
    /// Maximum character length.
    pub max_length: Option<usize>,
}

impl MigrationFieldDef {
    /// Creates a new migration field definition with sensible defaults.
    pub fn new(name: impl Into<String>, field_type: FieldType) -> Self {
        let name = name.into();
        let column = name.clone();
        Self {
            name,
            column,
            field_type,
            primary_key: false,
            null: false,
            default: None,
            unique: false,
            db_index: false,
            max_length: None,
        }
    }

    /// Sets the database column name.
    pub fn column(mut self, column: impl Into<String>) -> Self {
        self.column = column.into();
        self
    }

    /// Marks this field as the primary key.
    pub fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self
    }

    /// Allows NULL values.
    pub fn nullable(mut self) -> Self {
        self.null = true;
        self
    }

    /// Sets the maximum character length.
    pub fn max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self
    }

    /// Marks this field as having a UNIQUE constraint.
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// Marks this field as having a database index.
    pub fn db_index(mut self) -> Self {
        self.db_index = true;
        self
    }

    /// Sets the default value.
    pub fn default(mut self, value: impl Into<Value>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// Returns `true` if this is a relational field (FK, O2O, M2M).
    pub fn is_relation(&self) -> bool {
        matches!(
            self.field_type,
            FieldType::ForeignKey { .. }
                | FieldType::OneToOneField { .. }
                | FieldType::ManyToManyField { .. }
        )
    }

    /// Converts this to a `FieldDef` for use with the schema editor.
    pub fn to_field_def(&self) -> django_rs_db::fields::FieldDef {
        let mut fd = django_rs_db::fields::FieldDef::new(
            // SAFETY: we leak a string to get a &'static str. In migrations
            // this is acceptable as field defs are long-lived.
            Box::leak(self.name.clone().into_boxed_str()),
            self.field_type.clone(),
        )
        .column(self.column.clone());

        if self.primary_key {
            fd = fd.primary_key();
        }
        if self.null {
            fd = fd.nullable();
        }
        if self.unique {
            fd = fd.unique();
        }
        if self.db_index {
            fd = fd.db_index();
        }
        if let Some(ml) = self.max_length {
            fd = fd.max_length(ml);
        }
        if let Some(ref val) = self.default {
            fd = fd.default(val.clone());
        }
        fd
    }
}

/// Compares two [`ProjectState`]s and generates migration operations.
///
/// The autodetector detects:
/// - New models (creates `CreateModel`)
/// - Deleted models (creates `DeleteModel`)
/// - Added fields (creates `AddField`)
/// - Removed fields (creates `RemoveField`)
/// - Altered fields (creates `AlterField`)
/// - Renamed fields (heuristic: same type + one removed + one added)
/// - Changed `unique_together` (creates `AlterUniqueTogether`)
/// - Added/removed indexes (creates `AddIndex` / `RemoveIndex`)
pub struct MigrationAutodetector {
    /// The old project state (before changes).
    pub from_state: ProjectState,
    /// The new project state (after changes).
    pub to_state: ProjectState,
}

impl MigrationAutodetector {
    /// Creates a new autodetector with from and to states.
    pub fn new(from_state: ProjectState, to_state: ProjectState) -> Self {
        Self {
            from_state,
            to_state,
        }
    }

    /// Detects differences between the two states and returns operations
    /// grouped by app label.
    pub fn detect_changes(&self) -> HashMap<String, Vec<Box<dyn Operation>>> {
        let mut result: HashMap<String, Vec<Box<dyn Operation>>> = HashMap::new();

        // 1. Detect new models
        for (key, model) in &self.to_state.models {
            if !self.from_state.models.contains_key(key) {
                result.entry(key.0.clone()).or_default().push(Box::new(
                    CreateModel {
                        name: model.name.clone(),
                        fields: model.fields.clone(),
                        options: model.options.clone(),
                    },
                ));
            }
        }

        // 2. Detect deleted models
        for (key, model) in &self.from_state.models {
            if !self.to_state.models.contains_key(key) {
                result
                    .entry(key.0.clone())
                    .or_default()
                    .push(Box::new(DeleteModel {
                        name: model.name.clone(),
                    }));
            }
        }

        // 3. Detect field changes for existing models
        for (key, new_model) in &self.to_state.models {
            if let Some(old_model) = self.from_state.models.get(key) {
                let old_fields: HashMap<&str, &MigrationFieldDef> =
                    old_model.fields.iter().map(|f| (f.name.as_str(), f)).collect();
                let new_fields: HashMap<&str, &MigrationFieldDef> =
                    new_model.fields.iter().map(|f| (f.name.as_str(), f)).collect();

                let mut added: Vec<&MigrationFieldDef> = Vec::new();
                let mut removed: Vec<&MigrationFieldDef> = Vec::new();

                // Find removed fields
                for (name, field) in &old_fields {
                    if !new_fields.contains_key(name) {
                        removed.push(field);
                    }
                }

                // Find added fields
                for (name, field) in &new_fields {
                    if !old_fields.contains_key(name) {
                        added.push(field);
                    }
                }

                // Detect renames (heuristic: same type, one removed + one added)
                let mut renamed_old: Vec<String> = Vec::new();
                let mut renamed_new: Vec<String> = Vec::new();

                if added.len() == 1 && removed.len() == 1 {
                    let a = added[0];
                    let r = removed[0];
                    if field_types_match(&a.field_type, &r.field_type) {
                        result.entry(key.0.clone()).or_default().push(Box::new(
                            RenameField {
                                model_name: new_model.name.clone(),
                                old_name: r.name.clone(),
                                new_name: a.name.clone(),
                            },
                        ));
                        renamed_old.push(r.name.clone());
                        renamed_new.push(a.name.clone());
                    }
                }

                // Emit AddField for truly new fields
                for field in &added {
                    if !renamed_new.contains(&field.name) {
                        result.entry(key.0.clone()).or_default().push(Box::new(
                            AddField {
                                model_name: new_model.name.clone(),
                                field: (*field).clone(),
                            },
                        ));
                    }
                }

                // Emit RemoveField for truly removed fields
                for field in &removed {
                    if !renamed_old.contains(&field.name) {
                        result.entry(key.0.clone()).or_default().push(Box::new(
                            RemoveField {
                                model_name: new_model.name.clone(),
                                field_name: field.name.clone(),
                            },
                        ));
                    }
                }

                // Detect altered fields
                for (name, new_field) in &new_fields {
                    if let Some(old_field) = old_fields.get(name) {
                        if fields_differ(old_field, new_field) {
                            result.entry(key.0.clone()).or_default().push(Box::new(
                                AlterField {
                                    model_name: new_model.name.clone(),
                                    field_name: (*name).to_string(),
                                    field: (*new_field).clone(),
                                },
                            ));
                        }
                    }
                }

                // Detect unique_together changes
                if old_model.options.unique_together != new_model.options.unique_together {
                    result
                        .entry(key.0.clone())
                        .or_default()
                        .push(Box::new(AlterUniqueTogether {
                            model_name: new_model.name.clone(),
                            unique_together: new_model.options.unique_together.clone(),
                        }));
                }

                // Detect index changes
                let old_idx_names: Vec<Option<&str>> = old_model
                    .options
                    .indexes
                    .iter()
                    .map(|i| i.name.as_deref())
                    .collect();
                let new_idx_names: Vec<Option<&str>> = new_model
                    .options
                    .indexes
                    .iter()
                    .map(|i| i.name.as_deref())
                    .collect();

                // Removed indexes
                for idx in &old_model.options.indexes {
                    if let Some(ref idx_name) = idx.name {
                        if !new_idx_names.contains(&Some(idx_name.as_str())) {
                            result.entry(key.0.clone()).or_default().push(Box::new(
                                RemoveIndex {
                                    model_name: new_model.name.clone(),
                                    index_name: idx_name.clone(),
                                },
                            ));
                        }
                    }
                }

                // Added indexes
                for idx in &new_model.options.indexes {
                    if let Some(ref idx_name) = idx.name {
                        if !old_idx_names.contains(&Some(idx_name.as_str())) {
                            result.entry(key.0.clone()).or_default().push(Box::new(
                                AddIndex {
                                    model_name: new_model.name.clone(),
                                    index: idx.clone(),
                                },
                            ));
                        }
                    }
                }
            }
        }

        result
    }
}

/// Checks if two field types are structurally the same (for rename detection).
fn field_types_match(a: &FieldType, b: &FieldType) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Checks if two fields differ in schema-relevant properties.
fn fields_differ(a: &MigrationFieldDef, b: &MigrationFieldDef) -> bool {
    !field_types_match(&a.field_type, &b.field_type)
        || a.null != b.null
        || a.primary_key != b.primary_key
        || a.unique != b.unique
        || a.db_index != b.db_index
        || a.max_length != b.max_length
        || a.default != b.default
        || a.column != b.column
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_field(name: &str, ft: FieldType) -> MigrationFieldDef {
        MigrationFieldDef::new(name, ft)
    }

    // ── ProjectState tests ──────────────────────────────────────────

    #[test]
    fn test_project_state_new() {
        let state = ProjectState::new();
        assert!(state.models.is_empty());
    }

    #[test]
    fn test_project_state_add_model() {
        let mut state = ProjectState::new();
        let model = ModelState::new("blog", "post", vec![]);
        state.add_model(model);
        assert_eq!(state.models.len(), 1);
        assert!(state.models.contains_key(&("blog".into(), "post".into())));
    }

    // ── ModelState tests ────────────────────────────────────────────

    #[test]
    fn test_model_state_db_table_default() {
        let model = ModelState::new("blog", "post", vec![]);
        assert_eq!(model.db_table(), "blog_post");
    }

    #[test]
    fn test_model_state_db_table_custom() {
        let model = ModelState::new("blog", "post", vec![]).with_options(ModelOptions {
            db_table: Some("custom_table".into()),
            ..ModelOptions::default()
        });
        assert_eq!(model.db_table(), "custom_table");
    }

    // ── MigrationFieldDef tests ─────────────────────────────────────

    #[test]
    fn test_migration_field_def_new() {
        let f = MigrationFieldDef::new("title", FieldType::CharField);
        assert_eq!(f.name, "title");
        assert_eq!(f.column, "title");
        assert!(!f.primary_key);
        assert!(!f.null);
        assert!(!f.unique);
        assert!(!f.db_index);
        assert!(f.default.is_none());
        assert!(f.max_length.is_none());
    }

    #[test]
    fn test_migration_field_def_builder() {
        let f = MigrationFieldDef::new("email", FieldType::EmailField)
            .column("email_addr")
            .unique()
            .db_index()
            .max_length(254)
            .nullable()
            .default(Value::String(String::new()));
        assert_eq!(f.column, "email_addr");
        assert!(f.unique);
        assert!(f.db_index);
        assert_eq!(f.max_length, Some(254));
        assert!(f.null);
        assert_eq!(f.default, Some(Value::String(String::new())));
    }

    #[test]
    fn test_migration_field_def_primary_key() {
        let f = MigrationFieldDef::new("id", FieldType::BigAutoField).primary_key();
        assert!(f.primary_key);
    }

    #[test]
    fn test_migration_field_def_is_relation() {
        let fk = MigrationFieldDef::new(
            "author",
            FieldType::ForeignKey {
                to: "auth.User".into(),
                on_delete: django_rs_db::fields::OnDelete::Cascade,
                related_name: None,
            },
        );
        assert!(fk.is_relation());

        let text = MigrationFieldDef::new("title", FieldType::CharField);
        assert!(!text.is_relation());
    }

    #[test]
    fn test_migration_field_def_to_field_def() {
        let f = MigrationFieldDef::new("title", FieldType::CharField)
            .max_length(200)
            .unique()
            .nullable();
        let fd = f.to_field_def();
        assert_eq!(fd.name, "title");
        assert_eq!(fd.max_length, Some(200));
        assert!(fd.unique);
        assert!(fd.null);
    }

    // ── Autodetector: new model ─────────────────────────────────────

    #[test]
    fn test_detect_new_model() {
        let old = ProjectState::new();
        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200),
            ],
        ));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        assert_eq!(changes.len(), 1);
        let ops = changes.get("blog").unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].describe().contains("Create model"));
    }

    #[test]
    fn test_detect_deleted_model() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new("blog", "post", vec![]));
        let new_state = ProjectState::new();

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].describe().contains("Delete model"));
    }

    // ── Autodetector: field changes ─────────────────────────────────

    #[test]
    fn test_detect_added_field() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("id", FieldType::BigAutoField).primary_key()],
        ));

        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200),
            ],
        ));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].describe().contains("Add field"));
    }

    #[test]
    fn test_detect_removed_field() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200),
            ],
        ));

        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("id", FieldType::BigAutoField).primary_key()],
        ));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].describe().contains("Remove field"));
    }

    #[test]
    fn test_detect_altered_field_nullability() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200),
            ],
        ));

        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200).nullable(),
            ],
        ));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].describe().contains("Alter field"));
    }

    #[test]
    fn test_detect_altered_field_max_length() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("title", FieldType::CharField).max_length(100)],
        ));

        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("title", FieldType::CharField).max_length(200)],
        ));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert!(ops.iter().any(|op| op.describe().contains("Alter field")));
    }

    #[test]
    fn test_detect_renamed_field() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("title", FieldType::CharField).max_length(200),
            ],
        ));

        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new(
            "blog",
            "post",
            vec![
                make_field("id", FieldType::BigAutoField).primary_key(),
                make_field("headline", FieldType::CharField).max_length(200),
            ],
        ));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].describe().contains("Rename field"));
    }

    // ── Autodetector: unique_together / indexes ─────────────────────

    #[test]
    fn test_detect_unique_together_change() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new("blog", "post", vec![]));

        let mut new_state = ProjectState::new();
        new_state.add_model(
            ModelState::new("blog", "post", vec![]).with_options(ModelOptions {
                unique_together: vec![vec!["author".into(), "slug".into()]],
                ..ModelOptions::default()
            }),
        );

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert!(ops.iter().any(|op| op.describe().contains("unique_together")));
    }

    #[test]
    fn test_detect_added_index() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new("blog", "post", vec![]));

        let mut new_state = ProjectState::new();
        new_state.add_model(
            ModelState::new("blog", "post", vec![]).with_options(ModelOptions {
                indexes: vec![Index {
                    name: Some("idx_title".into()),
                    fields: vec!["title".into()],
                    unique: false,
                }],
                ..ModelOptions::default()
            }),
        );

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert!(ops.iter().any(|op| op.describe().contains("Add index")));
    }

    #[test]
    fn test_detect_removed_index() {
        let mut old = ProjectState::new();
        old.add_model(
            ModelState::new("blog", "post", vec![]).with_options(ModelOptions {
                indexes: vec![Index {
                    name: Some("idx_title".into()),
                    fields: vec!["title".into()],
                    unique: false,
                }],
                ..ModelOptions::default()
            }),
        );

        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new("blog", "post", vec![]));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert!(ops.iter().any(|op| op.describe().contains("Remove index")));
    }

    // ── Autodetector: no changes ────────────────────────────────────

    #[test]
    fn test_detect_no_changes() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("title", FieldType::CharField).max_length(200)],
        ));

        let new_state = old.clone();

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        assert!(changes.is_empty());
    }

    // ── Autodetector: multiple apps ─────────────────────────────────

    #[test]
    fn test_detect_changes_multiple_apps() {
        let old = ProjectState::new();
        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new("blog", "post", vec![]));
        new_state.add_model(ModelState::new("auth", "user", vec![]));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        assert!(changes.contains_key("blog"));
        assert!(changes.contains_key("auth"));
    }

    // ── Autodetector: field type change ──────────────────────────────

    #[test]
    fn test_detect_field_type_change() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("count", FieldType::IntegerField)],
        ));

        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("count", FieldType::BigIntegerField)],
        ));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert!(ops.iter().any(|op| op.describe().contains("Alter field")));
    }

    // ── Autodetector: added and removed together (not rename) ───────

    #[test]
    fn test_detect_multiple_added_removed_not_rename() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new(
            "blog",
            "post",
            vec![
                make_field("title", FieldType::CharField).max_length(200),
                make_field("slug", FieldType::SlugField),
            ],
        ));

        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new(
            "blog",
            "post",
            vec![
                make_field("headline", FieldType::CharField).max_length(200),
                make_field("url_path", FieldType::SlugField),
            ],
        ));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        // Should produce Add + Remove, not renames, because there are 2 of each
        let add_count = ops.iter().filter(|op| op.describe().contains("Add field")).count();
        let remove_count = ops
            .iter()
            .filter(|op| op.describe().contains("Remove field"))
            .count();
        assert_eq!(add_count, 2);
        assert_eq!(remove_count, 2);
    }

    // ── Autodetector: default change ────────────────────────────────

    #[test]
    fn test_detect_default_change() {
        let mut old = ProjectState::new();
        old.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("views", FieldType::IntegerField)],
        ));

        let mut new_state = ProjectState::new();
        new_state.add_model(ModelState::new(
            "blog",
            "post",
            vec![make_field("views", FieldType::IntegerField).default(Value::Int(0))],
        ));

        let detector = MigrationAutodetector::new(old, new_state);
        let changes = detector.detect_changes();
        let ops = changes.get("blog").unwrap();
        assert!(ops.iter().any(|op| op.describe().contains("Alter field")));
    }

    // ── Helper tests ────────────────────────────────────────────────

    #[test]
    fn test_field_types_match() {
        assert!(field_types_match(&FieldType::CharField, &FieldType::CharField));
        assert!(!field_types_match(&FieldType::CharField, &FieldType::TextField));
    }

    #[test]
    fn test_fields_differ_same() {
        let a = make_field("title", FieldType::CharField).max_length(200);
        let b = make_field("title", FieldType::CharField).max_length(200);
        assert!(!fields_differ(&a, &b));
    }

    #[test]
    fn test_fields_differ_different() {
        let a = make_field("title", FieldType::CharField).max_length(200);
        let b = make_field("title", FieldType::CharField).max_length(100);
        assert!(fields_differ(&a, &b));
    }
}
