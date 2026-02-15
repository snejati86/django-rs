//! Model trait and metadata for the ORM.
//!
//! The [`Model`] trait is the core abstraction that all ORM models implement.
//! It provides access to metadata, field values, and construction from database
//! rows. This mirrors Django's `django.db.models.Model` base class.
//!
//! [`ModelMeta`] captures the equivalent of Django's `class Meta` options,
//! including table name, ordering, indexes, and constraints.

use crate::fields::FieldDef;
use crate::query::compiler::{InheritanceType, OrderBy};
use crate::value::Value;
use django_rs_core::DjangoError;

/// A database row abstraction used for constructing model instances.
///
/// This is re-exported from the backends crate when available, but defined
/// here as a simplified version for the trait definition.
pub use crate::query::compiler::Row;

/// The core trait for all ORM models.
///
/// Any struct that represents a database table must implement this trait.
/// In practice, this will be derived via a proc macro (P11), but it can
/// also be implemented manually.
///
/// # Examples
///
/// ```
/// use django_rs_db::model::{Model, ModelMeta};
/// use django_rs_db::fields::{FieldDef, FieldType};
/// use django_rs_db::value::Value;
/// use django_rs_db::query::compiler::{InheritanceType, Row, OrderBy};
/// use django_rs_core::DjangoError;
///
/// struct Article {
///     id: i64,
///     title: String,
/// }
///
/// impl Model for Article {
///     fn meta() -> &'static ModelMeta {
///         use std::sync::LazyLock;
///         static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
///             app_label: "blog",
///             model_name: "article",
///             db_table: "blog_article".to_string(),
///             verbose_name: "article".to_string(),
///             verbose_name_plural: "articles".to_string(),
///             ordering: vec![],
///             unique_together: vec![],
///             indexes: vec![],
///             abstract_model: false,
///             fields: vec![],
///             constraints: vec![],
///             inheritance_type: InheritanceType::None,
///         });
///         &META
///     }
///
///     fn table_name() -> &'static str { "blog_article" }
///     fn app_label() -> &'static str { "blog" }
///
///     fn pk(&self) -> Option<&Value> { None }
///     fn set_pk(&mut self, value: Value) {
///         if let Value::Int(id) = value { self.id = id; }
///     }
///     fn field_values(&self) -> Vec<(&'static str, Value)> {
///         vec![("id", Value::Int(self.id)), ("title", Value::String(self.title.clone()))]
///     }
///     fn from_row(row: &Row) -> Result<Self, DjangoError> {
///         Ok(Article {
///             id: row.get::<i64>("id")?,
///             title: row.get::<String>("title")?,
///         })
///     }
/// }
/// ```
pub trait Model: Send + Sync + 'static {
    /// Returns the static metadata for this model type.
    fn meta() -> &'static ModelMeta;

    /// Returns the database table name.
    fn table_name() -> &'static str;

    /// Returns the application label this model belongs to.
    fn app_label() -> &'static str;

    /// Returns a reference to the primary key value, or `None` if unsaved.
    fn pk(&self) -> Option<&Value>;

    /// Sets the primary key value on this instance (used after INSERT).
    fn set_pk(&mut self, value: Value);

    /// Returns the name of the primary key field (e.g., "id").
    fn pk_field_name() -> &'static str {
        "id"
    }

    /// Returns all field name-value pairs for this instance.
    fn field_values(&self) -> Vec<(&'static str, Value)>;

    /// Returns field name-value pairs excluding the primary key.
    /// Used for INSERT operations where the PK is auto-generated.
    fn non_pk_field_values(&self) -> Vec<(&'static str, Value)> {
        let pk_name = Self::pk_field_name();
        self.field_values()
            .into_iter()
            .filter(|(name, _)| *name != pk_name)
            .collect()
    }

    /// Constructs a model instance from a database row.
    fn from_row(row: &Row) -> Result<Self, DjangoError>
    where
        Self: Sized;

    /// Returns the inheritance type for this model.
    ///
    /// Override this for models that use multi-table or proxy inheritance.
    fn inheritance_type() -> InheritanceType {
        InheritanceType::None
    }

    /// Returns field name-value pairs that belong to the parent table.
    ///
    /// Used for multi-table inheritance INSERT/UPDATE operations.
    /// Only relevant when `inheritance_type()` returns `MultiTable`.
    fn parent_field_values(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// Returns field name-value pairs that belong to the child table only.
    ///
    /// Used for multi-table inheritance INSERT/UPDATE operations.
    /// Only relevant when `inheritance_type()` returns `MultiTable`.
    fn child_field_values(&self) -> Vec<(&'static str, Value)> {
        self.non_pk_field_values()
    }
}

/// Metadata about a model, equivalent to Django's `class Meta`.
///
/// This struct captures all the options that can be specified in a Django
/// model's `Meta` inner class, including the database table name, ordering,
/// indexes, and constraints.
pub struct ModelMeta {
    /// The application label (e.g., "auth", "blog").
    pub app_label: &'static str,
    /// The model name in lowercase (e.g., "user", "article").
    pub model_name: &'static str,
    /// The database table name.
    pub db_table: String,
    /// Human-readable singular name.
    pub verbose_name: String,
    /// Human-readable plural name.
    pub verbose_name_plural: String,
    /// Default ordering for queries.
    pub ordering: Vec<OrderBy>,
    /// Sets of fields that must be unique together.
    pub unique_together: Vec<Vec<&'static str>>,
    /// Database indexes.
    pub indexes: Vec<Index>,
    /// Whether this is an abstract model (no table created).
    pub abstract_model: bool,
    /// Field definitions for this model.
    pub fields: Vec<FieldDef>,
    /// Database constraints (CHECK, UNIQUE).
    pub constraints: Vec<crate::constraints::BoxedConstraint>,
    /// The type of model inheritance.
    pub inheritance_type: InheritanceType,
}

/// A database index definition.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Index {
    /// Optional name for the index.
    pub name: Option<String>,
    /// The columns included in this index.
    pub fields: Vec<String>,
    /// Whether this is a unique index.
    pub unique: bool,
    /// The index type (B-tree by default; PostgreSQL supports additional types).
    #[serde(default)]
    pub index_type: IndexType,
}

impl Index {
    /// Generates the CREATE INDEX DDL statement for this index.
    pub fn create_sql(&self, table: &str) -> String {
        let unique_str = if self.unique { "UNIQUE " } else { "" };
        let idx_name = self.name.as_deref().unwrap_or("idx");
        let cols: Vec<String> = self.fields.iter().map(|f| format!("\"{f}\"")).collect();
        let using = self.index_type.sql_using_clause();
        format!(
            "CREATE {unique_str}INDEX \"{idx_name}\" ON \"{table}\" {using} ({})",
            cols.join(", ")
        )
    }
}

/// The type of database index.
///
/// PostgreSQL supports several index types beyond the standard B-tree.
/// Each type is optimized for different query patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum IndexType {
    /// Standard B-tree index (default for all backends).
    #[default]
    BTree,
    /// PostgreSQL GIN (Generalized Inverted Index).
    /// Optimal for array fields, full-text search, JSONB, and hstore.
    Gin,
    /// PostgreSQL GiST (Generalized Search Tree).
    /// Supports range types, geometric types, and full-text search.
    Gist,
    /// PostgreSQL BRIN (Block Range Index).
    /// Space-efficient for large, naturally ordered tables.
    Brin,
    /// PostgreSQL SP-GiST (Space-Partitioned Generalized Search Tree).
    /// For non-balanced data structures like radix trees and quad-trees.
    SpGist,
    /// PostgreSQL Bloom index.
    /// Probabilistic index for multi-column equality queries.
    Bloom,
}

impl IndexType {
    /// Returns the SQL USING clause for this index type.
    pub const fn sql_using_clause(&self) -> &'static str {
        match self {
            Self::BTree => "USING btree",
            Self::Gin => "USING gin",
            Self::Gist => "USING gist",
            Self::Brin => "USING brin",
            Self::SpGist => "USING spgist",
            Self::Bloom => "USING bloom",
        }
    }
}

/// A PostgreSQL GIN index definition.
///
/// GIN indexes are optimized for values that contain multiple elements,
/// such as arrays, JSONB, hstore, and full-text search tsvectors.
#[derive(Debug, Clone)]
pub struct GinIndex {
    /// The index name.
    pub name: String,
    /// The columns to index.
    pub fields: Vec<String>,
}

impl GinIndex {
    /// Creates a new GIN index.
    pub fn new(name: impl Into<String>, fields: Vec<&str>) -> Self {
        Self {
            name: name.into(),
            fields: fields.into_iter().map(String::from).collect(),
        }
    }
}

impl From<GinIndex> for Index {
    fn from(gin: GinIndex) -> Self {
        Self {
            name: Some(gin.name),
            fields: gin.fields,
            unique: false,
            index_type: IndexType::Gin,
        }
    }
}

/// A PostgreSQL GiST index definition.
///
/// GiST indexes support range types, geometric data, and can be used
/// for full-text search.
#[derive(Debug, Clone)]
pub struct GistIndex {
    /// The index name.
    pub name: String,
    /// The columns to index.
    pub fields: Vec<String>,
}

impl GistIndex {
    /// Creates a new GiST index.
    pub fn new(name: impl Into<String>, fields: Vec<&str>) -> Self {
        Self {
            name: name.into(),
            fields: fields.into_iter().map(String::from).collect(),
        }
    }
}

impl From<GistIndex> for Index {
    fn from(gist: GistIndex) -> Self {
        Self {
            name: Some(gist.name),
            fields: gist.fields,
            unique: false,
            index_type: IndexType::Gist,
        }
    }
}

/// A PostgreSQL BRIN index definition.
///
/// BRIN indexes are compact and efficient for very large tables where the
/// physical ordering of rows correlates with column values.
#[derive(Debug, Clone)]
pub struct BrinIndex {
    /// The index name.
    pub name: String,
    /// The columns to index.
    pub fields: Vec<String>,
}

impl BrinIndex {
    /// Creates a new BRIN index.
    pub fn new(name: impl Into<String>, fields: Vec<&str>) -> Self {
        Self {
            name: name.into(),
            fields: fields.into_iter().map(String::from).collect(),
        }
    }
}

impl From<BrinIndex> for Index {
    fn from(brin: BrinIndex) -> Self {
        Self {
            name: Some(brin.name),
            fields: brin.fields,
            unique: false,
            index_type: IndexType::Brin,
        }
    }
}

/// A PostgreSQL SP-GiST index definition.
///
/// SP-GiST indexes are for non-balanced data structures such as
/// radix trees and quad-trees.
#[derive(Debug, Clone)]
pub struct SpGistIndex {
    /// The index name.
    pub name: String,
    /// The columns to index.
    pub fields: Vec<String>,
}

impl SpGistIndex {
    /// Creates a new SP-GiST index.
    pub fn new(name: impl Into<String>, fields: Vec<&str>) -> Self {
        Self {
            name: name.into(),
            fields: fields.into_iter().map(String::from).collect(),
        }
    }
}

impl From<SpGistIndex> for Index {
    fn from(spgist: SpGistIndex) -> Self {
        Self {
            name: Some(spgist.name),
            fields: spgist.fields,
            unique: false,
            index_type: IndexType::SpGist,
        }
    }
}

/// A PostgreSQL Bloom index definition.
///
/// Bloom indexes use a probabilistic data structure (Bloom filter) to
/// efficiently handle equality queries on many columns simultaneously.
/// Requires the `bloom` extension.
#[derive(Debug, Clone)]
pub struct BloomIndex {
    /// The index name.
    pub name: String,
    /// The columns to index.
    pub fields: Vec<String>,
}

impl BloomIndex {
    /// Creates a new Bloom index.
    pub fn new(name: impl Into<String>, fields: Vec<&str>) -> Self {
        Self {
            name: name.into(),
            fields: fields.into_iter().map(String::from).collect(),
        }
    }
}

impl From<BloomIndex> for Index {
    fn from(bloom: BloomIndex) -> Self {
        Self {
            name: Some(bloom.name),
            fields: bloom.fields,
            unique: false,
            index_type: IndexType::Bloom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::FieldType;

    struct TestModel {
        id: i64,
        name: String,
    }

    impl Model for TestModel {
        fn meta() -> &'static ModelMeta {
            use std::sync::LazyLock;
            static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
                app_label: "test",
                model_name: "testmodel",
                db_table: "test_testmodel".to_string(),
                verbose_name: "test model".to_string(),
                verbose_name_plural: "test models".to_string(),
                ordering: vec![],
                unique_together: vec![],
                indexes: vec![],
                abstract_model: false,
                fields: vec![
                    FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                    FieldDef::new("name", FieldType::CharField).max_length(100),
                ],
                constraints: vec![],
                inheritance_type: InheritanceType::None,
            });
            &META
        }

        fn table_name() -> &'static str {
            "test_testmodel"
        }

        fn app_label() -> &'static str {
            "test"
        }

        fn pk(&self) -> Option<&Value> {
            if self.id == 0 {
                None
            } else {
                Some(&Value::Int(0)) // placeholder; real impl would store Value
            }
        }

        fn set_pk(&mut self, value: Value) {
            if let Value::Int(id) = value {
                self.id = id;
            }
        }

        fn field_values(&self) -> Vec<(&'static str, Value)> {
            vec![
                ("id", Value::Int(self.id)),
                ("name", Value::String(self.name.clone())),
            ]
        }

        fn from_row(row: &Row) -> Result<Self, DjangoError> {
            Ok(TestModel {
                id: row.get::<i64>("id")?,
                name: row.get::<String>("name")?,
            })
        }
    }

    #[test]
    fn test_model_meta() {
        let meta = TestModel::meta();
        assert_eq!(meta.app_label, "test");
        assert_eq!(meta.model_name, "testmodel");
        assert_eq!(meta.db_table, "test_testmodel");
        assert!(!meta.abstract_model);
        assert_eq!(meta.fields.len(), 2);
    }

    #[test]
    fn test_model_table_name() {
        assert_eq!(TestModel::table_name(), "test_testmodel");
    }

    #[test]
    fn test_model_app_label() {
        assert_eq!(TestModel::app_label(), "test");
    }

    #[test]
    fn test_model_field_values() {
        let m = TestModel {
            id: 1,
            name: "Alice".to_string(),
        };
        let values = m.field_values();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0], ("id", Value::Int(1)));
        assert_eq!(values[1], ("name", Value::String("Alice".to_string())));
    }

    #[test]
    fn test_model_from_row() {
        let row = Row::new(
            vec!["id".to_string(), "name".to_string()],
            vec![Value::Int(1), Value::String("Alice".to_string())],
        );
        let m = TestModel::from_row(&row).unwrap();
        assert_eq!(m.id, 1);
        assert_eq!(m.name, "Alice");
    }

    #[test]
    fn test_index() {
        let idx = Index {
            name: Some("idx_name".to_string()),
            fields: vec!["name".to_string()],
            unique: false,
            index_type: IndexType::BTree,
        };
        assert_eq!(idx.name.as_deref(), Some("idx_name"));
        assert!(!idx.unique);
        assert_eq!(idx.index_type, IndexType::BTree);
    }

    #[test]
    fn test_index_create_sql_btree() {
        let idx = Index {
            name: Some("idx_name".to_string()),
            fields: vec!["name".to_string()],
            unique: false,
            index_type: IndexType::BTree,
        };
        let sql = idx.create_sql("users");
        assert_eq!(
            sql,
            "CREATE INDEX \"idx_name\" ON \"users\" USING btree (\"name\")"
        );
    }

    #[test]
    fn test_index_create_sql_unique() {
        let idx = Index {
            name: Some("idx_email_unique".to_string()),
            fields: vec!["email".to_string()],
            unique: true,
            index_type: IndexType::BTree,
        };
        let sql = idx.create_sql("users");
        assert!(sql.starts_with("CREATE UNIQUE INDEX"));
    }

    #[test]
    fn test_gin_index() {
        let gin = GinIndex::new("idx_tags_gin", vec!["tags"]);
        let idx: Index = gin.into();
        assert_eq!(idx.index_type, IndexType::Gin);
        assert_eq!(idx.name.as_deref(), Some("idx_tags_gin"));
        let sql = idx.create_sql("posts");
        assert!(sql.contains("USING gin"));
    }

    #[test]
    fn test_gist_index() {
        let gist = GistIndex::new("idx_location_gist", vec!["location"]);
        let idx: Index = gist.into();
        assert_eq!(idx.index_type, IndexType::Gist);
        let sql = idx.create_sql("venues");
        assert!(sql.contains("USING gist"));
    }

    #[test]
    fn test_brin_index() {
        let brin = BrinIndex::new("idx_created_brin", vec!["created_at"]);
        let idx: Index = brin.into();
        assert_eq!(idx.index_type, IndexType::Brin);
        let sql = idx.create_sql("events");
        assert!(sql.contains("USING brin"));
    }

    #[test]
    fn test_spgist_index() {
        let spgist = SpGistIndex::new("idx_ip_spgist", vec!["ip_addr"]);
        let idx: Index = spgist.into();
        assert_eq!(idx.index_type, IndexType::SpGist);
        let sql = idx.create_sql("connections");
        assert!(sql.contains("USING spgist"));
    }

    #[test]
    fn test_bloom_index() {
        let bloom = BloomIndex::new("idx_multi_bloom", vec!["col1", "col2", "col3"]);
        let idx: Index = bloom.into();
        assert_eq!(idx.index_type, IndexType::Bloom);
        assert_eq!(idx.fields.len(), 3);
        let sql = idx.create_sql("items");
        assert!(sql.contains("USING bloom"));
        assert!(sql.contains("\"col1\""));
        assert!(sql.contains("\"col2\""));
        assert!(sql.contains("\"col3\""));
    }

    #[test]
    fn test_index_type_sql_using() {
        assert_eq!(IndexType::BTree.sql_using_clause(), "USING btree");
        assert_eq!(IndexType::Gin.sql_using_clause(), "USING gin");
        assert_eq!(IndexType::Gist.sql_using_clause(), "USING gist");
        assert_eq!(IndexType::Brin.sql_using_clause(), "USING brin");
        assert_eq!(IndexType::SpGist.sql_using_clause(), "USING spgist");
        assert_eq!(IndexType::Bloom.sql_using_clause(), "USING bloom");
    }

    #[test]
    fn test_index_type_default() {
        let idx_type: IndexType = Default::default();
        assert_eq!(idx_type, IndexType::BTree);
    }

    #[test]
    fn test_multi_column_index_sql() {
        let idx = Index {
            name: Some("idx_user_project".to_string()),
            fields: vec!["user_id".to_string(), "project_id".to_string()],
            unique: true,
            index_type: IndexType::BTree,
        };
        let sql = idx.create_sql("memberships");
        assert_eq!(
            sql,
            "CREATE UNIQUE INDEX \"idx_user_project\" ON \"memberships\" USING btree (\"user_id\", \"project_id\")"
        );
    }
}
