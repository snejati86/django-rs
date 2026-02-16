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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
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
    // ── PostgreSQL-specific field types ──────────────────────────────────
    /// PostgreSQL array field. Stores a homogeneous array of another field type.
    /// SQL: `INTEGER[]`, `TEXT[]`, etc.
    ArrayField {
        /// The base field type for the array elements.
        base_field: Box<FieldType>,
        /// Optional maximum array size.
        size: Option<usize>,
    },
    /// PostgreSQL hstore field. Stores key-value pairs as `hstore` type.
    HStoreField,
    /// PostgreSQL integer range field (`int4range`).
    IntegerRangeField,
    /// PostgreSQL big integer range field (`int8range`).
    BigIntegerRangeField,
    /// PostgreSQL floating-point range field (`numrange`).
    FloatRangeField,
    /// PostgreSQL date range field (`daterange`).
    DateRangeField,
    /// PostgreSQL date-time range field (`tstzrange`).
    DateTimeRangeField,
    /// A database-computed generated field (Django 5.0+).
    /// The value is always computed by the database engine.
    GeneratedField {
        /// The SQL expression that computes the field value.
        expression: String,
        /// The output field type (determines the column type).
        output_field: Box<FieldType>,
        /// Whether the generated column is stored (STORED) or virtual (VIRTUAL).
        /// PostgreSQL only supports STORED.
        db_persist: bool,
    },
}

/// Behavior when a referenced object is deleted (ON DELETE action).
///
/// This mirrors Django's `on_delete` parameter for `ForeignKey` and
/// `OneToOneField`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

    /// Returns `true` if this is a database-generated field.
    pub const fn is_generated(&self) -> bool {
        matches!(self.field_type, FieldType::GeneratedField { .. })
    }
}

impl FieldType {
    /// Returns the SQL column type for the given field type on PostgreSQL.
    ///
    /// This is used by schema generation and migration tools.
    pub fn pg_column_type(&self) -> String {
        match self {
            Self::AutoField => "SERIAL".to_string(),
            Self::BigAutoField => "BIGSERIAL".to_string(),
            Self::CharField => "VARCHAR".to_string(),
            Self::TextField => "TEXT".to_string(),
            Self::IntegerField => "INTEGER".to_string(),
            Self::BigIntegerField => "BIGINT".to_string(),
            Self::SmallIntegerField => "SMALLINT".to_string(),
            Self::FloatField => "DOUBLE PRECISION".to_string(),
            Self::DecimalField {
                max_digits,
                decimal_places,
            } => format!("NUMERIC({max_digits}, {decimal_places})"),
            Self::BooleanField => "BOOLEAN".to_string(),
            Self::DateField => "DATE".to_string(),
            Self::DateTimeField => "TIMESTAMP".to_string(),
            Self::TimeField => "TIME".to_string(),
            Self::DurationField => "INTERVAL".to_string(),
            Self::UuidField => "UUID".to_string(),
            Self::BinaryField => "BYTEA".to_string(),
            Self::JsonField => "JSONB".to_string(),
            Self::EmailField | Self::UrlField | Self::SlugField | Self::FilePathField => {
                "VARCHAR".to_string()
            }
            Self::IpAddressField => "INET".to_string(),
            Self::ForeignKey { .. } | Self::OneToOneField { .. } => "INTEGER".to_string(),
            Self::ManyToManyField { .. } => String::new(),
            Self::ArrayField { base_field, .. } => {
                format!("{}[]", base_field.pg_column_type())
            }
            Self::HStoreField => "HSTORE".to_string(),
            Self::IntegerRangeField => "INT4RANGE".to_string(),
            Self::BigIntegerRangeField => "INT8RANGE".to_string(),
            Self::FloatRangeField => "NUMRANGE".to_string(),
            Self::DateRangeField => "DATERANGE".to_string(),
            Self::DateTimeRangeField => "TSTZRANGE".to_string(),
            Self::GeneratedField { output_field, .. } => output_field.pg_column_type(),
        }
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
        let fk = FieldDef::new(
            "author",
            FieldType::ForeignKey {
                to: "auth.User".into(),
                on_delete: OnDelete::Cascade,
                related_name: None,
            },
        );
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
        let f = FieldDef::new(
            "price",
            FieldType::DecimalField {
                max_digits: 10,
                decimal_places: 2,
            },
        );
        if let FieldType::DecimalField {
            max_digits,
            decimal_places,
        } = &f.field_type
        {
            assert_eq!(*max_digits, 10);
            assert_eq!(*decimal_places, 2);
        } else {
            panic!("Expected DecimalField");
        }
    }

    // ── PostgreSQL-specific field type tests ────────────────────────────

    #[test]
    fn test_array_field() {
        let f = FieldDef::new(
            "tags",
            FieldType::ArrayField {
                base_field: Box::new(FieldType::CharField),
                size: None,
            },
        );
        if let FieldType::ArrayField { base_field, size } = &f.field_type {
            assert!(matches!(**base_field, FieldType::CharField));
            assert!(size.is_none());
        } else {
            panic!("Expected ArrayField");
        }
    }

    #[test]
    fn test_array_field_with_size() {
        let f = FieldDef::new(
            "scores",
            FieldType::ArrayField {
                base_field: Box::new(FieldType::IntegerField),
                size: Some(10),
            },
        );
        if let FieldType::ArrayField { size, .. } = &f.field_type {
            assert_eq!(*size, Some(10));
        } else {
            panic!("Expected ArrayField");
        }
    }

    #[test]
    fn test_hstore_field() {
        let f = FieldDef::new("metadata", FieldType::HStoreField);
        assert!(matches!(f.field_type, FieldType::HStoreField));
    }

    #[test]
    fn test_range_fields() {
        let int_range = FieldType::IntegerRangeField;
        assert_eq!(int_range.pg_column_type(), "INT4RANGE");

        let bigint_range = FieldType::BigIntegerRangeField;
        assert_eq!(bigint_range.pg_column_type(), "INT8RANGE");

        let float_range = FieldType::FloatRangeField;
        assert_eq!(float_range.pg_column_type(), "NUMRANGE");

        let date_range = FieldType::DateRangeField;
        assert_eq!(date_range.pg_column_type(), "DATERANGE");

        let datetime_range = FieldType::DateTimeRangeField;
        assert_eq!(datetime_range.pg_column_type(), "TSTZRANGE");
    }

    #[test]
    fn test_generated_field() {
        let f = FieldDef::new(
            "full_name",
            FieldType::GeneratedField {
                expression: "first_name || ' ' || last_name".to_string(),
                output_field: Box::new(FieldType::CharField),
                db_persist: true,
            },
        );
        assert!(f.is_generated());
        if let FieldType::GeneratedField {
            expression,
            output_field,
            db_persist,
        } = &f.field_type
        {
            assert_eq!(expression, "first_name || ' ' || last_name");
            assert!(matches!(**output_field, FieldType::CharField));
            assert!(*db_persist);
        } else {
            panic!("Expected GeneratedField");
        }
    }

    #[test]
    fn test_generated_field_not_editable() {
        // Generated fields should typically not be editable.
        let f = FieldDef::new(
            "total",
            FieldType::GeneratedField {
                expression: "price * quantity".to_string(),
                output_field: Box::new(FieldType::FloatField),
                db_persist: true,
            },
        );
        // By default editable is true; user should set it to false.
        assert!(f.is_generated());
    }

    #[test]
    fn test_pg_column_type_array() {
        let ft = FieldType::ArrayField {
            base_field: Box::new(FieldType::IntegerField),
            size: None,
        };
        assert_eq!(ft.pg_column_type(), "INTEGER[]");
    }

    #[test]
    fn test_pg_column_type_nested_array() {
        let ft = FieldType::ArrayField {
            base_field: Box::new(FieldType::ArrayField {
                base_field: Box::new(FieldType::IntegerField),
                size: None,
            }),
            size: None,
        };
        assert_eq!(ft.pg_column_type(), "INTEGER[][]");
    }

    #[test]
    fn test_pg_column_type_hstore() {
        assert_eq!(FieldType::HStoreField.pg_column_type(), "HSTORE");
    }

    #[test]
    fn test_pg_column_type_generated() {
        let ft = FieldType::GeneratedField {
            expression: "a + b".to_string(),
            output_field: Box::new(FieldType::IntegerField),
            db_persist: true,
        };
        // The column type of a generated field is its output_field type.
        assert_eq!(ft.pg_column_type(), "INTEGER");
    }

    #[test]
    fn test_is_generated_false_for_regular() {
        let f = FieldDef::new("name", FieldType::CharField);
        assert!(!f.is_generated());
    }

    #[test]
    fn test_pg_column_types_basic() {
        assert_eq!(FieldType::AutoField.pg_column_type(), "SERIAL");
        assert_eq!(FieldType::BigAutoField.pg_column_type(), "BIGSERIAL");
        assert_eq!(FieldType::TextField.pg_column_type(), "TEXT");
        assert_eq!(FieldType::BooleanField.pg_column_type(), "BOOLEAN");
        assert_eq!(FieldType::UuidField.pg_column_type(), "UUID");
        assert_eq!(FieldType::JsonField.pg_column_type(), "JSONB");
        assert_eq!(FieldType::BinaryField.pg_column_type(), "BYTEA");
        assert_eq!(FieldType::IpAddressField.pg_column_type(), "INET");
    }
}
