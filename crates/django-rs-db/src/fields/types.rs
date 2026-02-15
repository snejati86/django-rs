//! Field type definitions for the ORM.
//!
//! This module defines the field type system used by model definitions. Each
//! [`FieldType`] variant corresponds to a Django model field type, and
//! [`FieldDef`] captures all metadata about a single model field.

use crate::validators::Validator;
use crate::value::Value;

/// The type of a model field, determining its SQL column type and behavior.
///
/// Each variant maps to a Django field class. Relational fields (`ForeignKey`,
/// `OneToOneField`, `ManyToManyField`) carry additional metadata about the
/// relationship.
#[derive(Debug, Clone)]
pub enum FieldType {
    /// Auto-incrementing 32-bit integer primary key.
    AutoField,
    /// Auto-incrementing 64-bit integer primary key.
    BigAutoField,
    /// Variable-length string with a max length.
    CharField,
    /// Unlimited-length text.
    TextField,
    /// 32-bit signed integer.
    IntegerField,
    /// 64-bit signed integer.
    BigIntegerField,
    /// 16-bit signed integer.
    SmallIntegerField,
    /// 64-bit floating-point number.
    FloatField,
    /// Fixed-precision decimal number.
    DecimalField {
        /// Maximum total digits.
        max_digits: u32,
        /// Digits after the decimal point.
        decimal_places: u32,
    },
    /// Boolean (true/false).
    BooleanField,
    /// Date without time.
    DateField,
    /// Date and time.
    DateTimeField,
    /// Time without date.
    TimeField,
    /// Duration / interval.
    DurationField,
    /// UUID field.
    UuidField,
    /// Raw binary data.
    BinaryField,
    /// JSON data.
    JsonField,
    /// Email address (CharField with email validation).
    EmailField,
    /// URL (CharField with URL validation).
    UrlField,
    /// Slug (URL-friendly string).
    SlugField,
    /// IP address.
    IpAddressField,
    /// File system path.
    FilePathField,
    /// Many-to-one relationship.
    ForeignKey {
        /// The target model name (e.g. "auth.User").
        to: String,
        /// Behavior when the referenced object is deleted.
        on_delete: OnDelete,
        /// The name used for the reverse relation.
        related_name: Option<String>,
    },
    /// One-to-one relationship (unique foreign key).
    OneToOneField {
        /// The target model name.
        to: String,
        /// Behavior when the referenced object is deleted.
        on_delete: OnDelete,
        /// The name used for the reverse relation.
        related_name: Option<String>,
    },
    /// Many-to-many relationship (via intermediate table).
    ManyToManyField {
        /// The target model name.
        to: String,
        /// Optional explicit intermediate ("through") model.
        through: Option<String>,
        /// The name used for the reverse relation.
        related_name: Option<String>,
    },
}

/// Behavior when a referenced object is deleted (ON DELETE action).
///
/// This mirrors Django's `on_delete` parameter for `ForeignKey` and
/// `OneToOneField`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnDelete {
    /// Delete all related objects (CASCADE).
    Cascade,
    /// Prevent deletion if related objects exist (PROTECT).
    Protect,
    /// Set the foreign key to NULL.
    SetNull,
    /// Set the foreign key to its default value.
    SetDefault,
    /// Take no action (may cause integrity errors).
    DoNothing,
}

/// Complete definition of a model field, including metadata and constraints.
///
/// This struct captures everything Django stores in a field's `__init__` and
/// metadata. It is typically constructed by the proc macro (P11) or manually
/// when implementing the [`Model`](crate::model::Model) trait.
#[derive(Debug)]
pub struct FieldDef {
    /// The Python/Rust attribute name of this field.
    pub name: &'static str,
    /// The database column name (may differ from `name`).
    pub column: String,
    /// The type of this field.
    pub field_type: FieldType,
    /// Whether this field is the primary key.
    pub primary_key: bool,
    /// Whether NULL is allowed in the database.
    pub null: bool,
    /// Whether the field may be left blank in forms.
    pub blank: bool,
    /// Default value for new instances.
    pub default: Option<Value>,
    /// Whether a UNIQUE constraint is applied.
    pub unique: bool,
    /// Whether a database index should be created.
    pub db_index: bool,
    /// Maximum character length (for CharField and similar).
    pub max_length: Option<usize>,
    /// Human-readable help text.
    pub help_text: String,
    /// Human-readable name for the field.
    pub verbose_name: String,
    /// Allowed values as (value, display_label) pairs.
    pub choices: Option<Vec<(Value, String)>>,
    /// Validators applied during model validation.
    pub validators: Vec<Box<dyn Validator>>,
    /// Whether the field is editable in forms.
    pub editable: bool,
}

impl FieldDef {
    /// Creates a new `FieldDef` with sensible defaults.
    ///
    /// Only the field name and type are required. All other attributes take
    /// their default values (non-null, no index, editable, etc.).
    pub fn new(name: &'static str, field_type: FieldType) -> Self {
        Self {
            name,
            column: name.to_string(),
            field_type,
            primary_key: false,
            null: false,
            blank: false,
            default: None,
            unique: false,
            db_index: false,
            max_length: None,
            help_text: String::new(),
            verbose_name: name.replace('_', " "),
            choices: None,
            validators: Vec::new(),
            editable: true,
        }
    }

    /// Sets the database column name.
    #[must_use]
    pub fn column(mut self, column: impl Into<String>) -> Self {
        self.column = column.into();
        self
    }

    /// Marks this field as the primary key.
    #[must_use]
    pub const fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self
    }

    /// Allows NULL values in the database.
    #[must_use]
    pub const fn nullable(mut self) -> Self {
        self.null = true;
        self
    }

    /// Sets the maximum character length.
    #[must_use]
    pub const fn max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self
    }

    /// Marks this field as having a database index.
    #[must_use]
    pub const fn db_index(mut self) -> Self {
        self.db_index = true;
        self
    }

    /// Marks this field as having a UNIQUE constraint.
    #[must_use]
    pub const fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// Sets the default value for this field.
    #[must_use]
    pub fn default(mut self, value: impl Into<Value>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// Sets the verbose (human-readable) name.
    #[must_use]
    pub fn verbose_name(mut self, name: impl Into<String>) -> Self {
        self.verbose_name = name.into();
        self
    }

    /// Sets the help text.
    #[must_use]
    pub fn help_text(mut self, text: impl Into<String>) -> Self {
        self.help_text = text.into();
        self
    }

    /// Returns `true` if this field represents a relational field.
    pub const fn is_relation(&self) -> bool {
        matches!(
            self.field_type,
            FieldType::ForeignKey { .. }
                | FieldType::OneToOneField { .. }
                | FieldType::ManyToManyField { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_def_new_defaults() {
        let f = FieldDef::new("first_name", FieldType::CharField);
        assert_eq!(f.name, "first_name");
        assert_eq!(f.column, "first_name");
        assert!(!f.primary_key);
        assert!(!f.null);
        assert!(!f.blank);
        assert!(f.default.is_none());
        assert!(!f.unique);
        assert!(!f.db_index);
        assert!(f.max_length.is_none());
        assert!(f.editable);
        assert_eq!(f.verbose_name, "first name");
    }

    #[test]
    fn test_field_def_builder() {
        let f = FieldDef::new("email", FieldType::EmailField)
            .column("email_address")
            .unique()
            .db_index()
            .max_length(254)
            .verbose_name("Email Address")
            .help_text("Enter a valid email");
        assert_eq!(f.column, "email_address");
        assert!(f.unique);
        assert!(f.db_index);
        assert_eq!(f.max_length, Some(254));
        assert_eq!(f.verbose_name, "Email Address");
        assert_eq!(f.help_text, "Enter a valid email");
    }

    #[test]
    fn test_field_def_primary_key() {
        let f = FieldDef::new("id", FieldType::AutoField).primary_key();
        assert!(f.primary_key);
    }

    #[test]
    fn test_field_def_nullable() {
        let f = FieldDef::new("bio", FieldType::TextField).nullable();
        assert!(f.null);
    }

    #[test]
    fn test_field_def_default() {
        let f = FieldDef::new("active", FieldType::BooleanField).default(Value::Bool(true));
        assert_eq!(f.default, Some(Value::Bool(true)));
    }

    #[test]
    fn test_field_def_is_relation() {
        let fk = FieldDef::new("author", FieldType::ForeignKey {
            to: "auth.User".into(),
            on_delete: OnDelete::Cascade,
            related_name: None,
        });
        assert!(fk.is_relation());

        let text = FieldDef::new("title", FieldType::CharField);
        assert!(!text.is_relation());
    }

    #[test]
    fn test_on_delete_variants() {
        assert_eq!(OnDelete::Cascade, OnDelete::Cascade);
        assert_ne!(OnDelete::Cascade, OnDelete::Protect);
        assert_ne!(OnDelete::SetNull, OnDelete::SetDefault);
    }

    #[test]
    fn test_decimal_field_type() {
        let f = FieldDef::new("price", FieldType::DecimalField {
            max_digits: 10,
            decimal_places: 2,
        });
        if let FieldType::DecimalField { max_digits, decimal_places } = &f.field_type {
            assert_eq!(*max_digits, 10);
            assert_eq!(*decimal_places, 2);
        } else {
            panic!("Expected DecimalField");
        }
    }
}
