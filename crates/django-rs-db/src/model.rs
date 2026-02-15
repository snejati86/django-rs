//! Model trait and metadata for the ORM.
//!
//! The [`Model`] trait is the core abstraction that all ORM models implement.
//! It provides access to metadata, field values, and construction from database
//! rows. This mirrors Django's `django.db.models.Model` base class.
//!
//! [`ModelMeta`] captures the equivalent of Django's `class Meta` options,
//! including table name, ordering, indexes, and constraints.

use crate::fields::FieldDef;
use crate::query::compiler::OrderBy;
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
/// use django_rs_db::query::compiler::{Row, OrderBy};
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
        };
        assert_eq!(idx.name.as_deref(), Some("idx_name"));
        assert!(!idx.unique);
    }
}
