# Models and Fields Reference

This reference covers the model system and field types in django-rs. For a tutorial introduction, see [Tutorial 2: Models and the Admin Panel](../tutorial/02-models-and-admin.md).

---

## The Model trait

Every ORM model implements the `Model` trait, which provides access to metadata, field values, and construction from database rows:

```rust
pub trait Model: Send + Sync + 'static {
    fn meta() -> &'static ModelMeta;
    fn table_name() -> &'static str;
    fn app_label() -> &'static str;
    fn pk(&self) -> Option<&Value>;
    fn set_pk(&mut self, value: Value);
    fn pk_field_name() -> &'static str;          // default: "id"
    fn field_values(&self) -> Vec<(&'static str, Value)>;
    fn non_pk_field_values(&self) -> Vec<(&'static str, Value)>;
    fn from_row(row: &Row) -> Result<Self, DjangoError>;
    fn inheritance_type() -> InheritanceType;      // default: None
    fn parent_field_values(&self) -> Vec<(&'static str, Value)>;
    fn child_field_values(&self) -> Vec<(&'static str, Value)>;
}
```

### Implementing Model

```rust
use django_rs_db::model::{Model, ModelMeta};
use django_rs_db::fields::{FieldDef, FieldType};
use django_rs_db::value::Value;
use django_rs_db::query::compiler::{InheritanceType, Row, OrderBy};

struct Post {
    id: i64,
    title: String,
    published: bool,
}

impl Model for Post {
    fn meta() -> &'static ModelMeta {
        use std::sync::LazyLock;
        static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
            app_label: "blog",
            model_name: "post",
            db_table: "blog_post".to_string(),
            verbose_name: "post".to_string(),
            verbose_name_plural: "posts".to_string(),
            ordering: vec![OrderBy::desc("created_at")],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("title", FieldType::CharField).max_length(200),
                FieldDef::new("published", FieldType::BooleanField),
            ],
            constraints: vec![],
            inheritance_type: InheritanceType::None,
        });
        &META
    }

    fn table_name() -> &'static str { "blog_post" }
    fn app_label() -> &'static str { "blog" }

    fn pk(&self) -> Option<&Value> { Some(&Value::Int(self.id)) }
    fn set_pk(&mut self, value: Value) {
        if let Value::Int(id) = value { self.id = id; }
    }

    fn field_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::Int(self.id)),
            ("title", Value::String(self.title.clone())),
            ("published", Value::Bool(self.published)),
        ]
    }

    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        Ok(Post {
            id: row.get::<i64>("id")?,
            title: row.get::<String>("title")?,
            published: row.get::<bool>("published")?,
        })
    }
}
```

---

## ModelMeta

`ModelMeta` is the equivalent of Django's `class Meta`. It describes a model's database mapping:

| Field | Type | Description |
|-------|------|-------------|
| `app_label` | `&'static str` | Application label (e.g., `"blog"`) |
| `model_name` | `&'static str` | Model name in lowercase (e.g., `"post"`) |
| `db_table` | `String` | Database table name (e.g., `"blog_post"`) |
| `verbose_name` | `String` | Human-readable singular name |
| `verbose_name_plural` | `String` | Human-readable plural name |
| `ordering` | `Vec<OrderBy>` | Default query ordering |
| `unique_together` | `Vec<Vec<&'static str>>` | Composite uniqueness constraints |
| `indexes` | `Vec<Index>` | Database indexes (see [Indexes and Constraints](./indexes-and-constraints.md)) |
| `abstract_model` | `bool` | If `true`, no table is created |
| `fields` | `Vec<FieldDef>` | Field definitions |
| `constraints` | `Vec<BoxedConstraint>` | CHECK, UNIQUE, and EXCLUDE constraints |
| `inheritance_type` | `InheritanceType` | `None`, `MultiTable`, or `Proxy` |

---

## Field types

Fields are defined with `FieldDef::new(name, field_type)` and configured with builder methods:

```rust
use django_rs_db::fields::{FieldDef, FieldType};

FieldDef::new("title", FieldType::CharField)
    .max_length(200)
    .null(false)
    .blank(false)
    .help_text("The article title")
```

### FieldDef builder methods

| Method | Default | Description |
|--------|---------|-------------|
| `.primary_key()` | `false` | Marks as primary key |
| `.null(bool)` | `false` | Allows NULL values |
| `.blank(bool)` | `false` | Allows empty values in forms |
| `.default_value(Value)` | `None` | Default value for new rows |
| `.unique()` | `false` | Adds a unique constraint |
| `.db_index()` | `false` | Creates an index on this column |
| `.max_length(usize)` | `None` | Maximum character length |
| `.help_text(&str)` | `""` | Help text for admin/forms |
| `.verbose_name(&str)` | Derived | Human-readable name |
| `.editable(bool)` | `true` | Whether the field appears in forms |
| `.choices(Vec<(String, String)>)` | `[]` | Valid choices for this field |

### Complete field type reference

#### Auto fields

| Type | SQL (PostgreSQL) | Description |
|------|-----------------|-------------|
| `AutoField` | `SERIAL` | Auto-incrementing 32-bit integer |
| `BigAutoField` | `BIGSERIAL` | Auto-incrementing 64-bit integer |
| `SmallAutoField` | `SMALLSERIAL` | Auto-incrementing 16-bit integer |

#### Numeric fields

| Type | SQL | Description |
|------|-----|-------------|
| `IntegerField` | `INTEGER` | 32-bit signed integer |
| `BigIntegerField` | `BIGINT` | 64-bit signed integer |
| `SmallIntegerField` | `SMALLINT` | 16-bit signed integer |
| `PositiveIntegerField` | `INTEGER CHECK >= 0` | Non-negative 32-bit integer |
| `PositiveBigIntegerField` | `BIGINT CHECK >= 0` | Non-negative 64-bit integer |
| `PositiveSmallIntegerField` | `SMALLINT CHECK >= 0` | Non-negative 16-bit integer |
| `FloatField` | `DOUBLE PRECISION` | 64-bit floating point |
| `DecimalField` | `NUMERIC(max_digits, decimal_places)` | Fixed-precision decimal |

#### Text fields

| Type | SQL | Description |
|------|-----|-------------|
| `CharField` | `VARCHAR(max_length)` | Fixed-length string |
| `TextField` | `TEXT` | Unlimited-length string |
| `SlugField` | `VARCHAR(max_length)` | URL-friendly string |
| `EmailField` | `VARCHAR(254)` | Email address |
| `UrlField` | `VARCHAR(200)` | URL |
| `IpAddressField` | `VARCHAR(39)` | IPv4 or IPv6 address |
| `FilePathField` | `VARCHAR(100)` | File system path |

#### Temporal fields

| Type | SQL | Description |
|------|-----|-------------|
| `DateField` | `DATE` | Calendar date |
| `DateTimeField` | `TIMESTAMP WITH TIME ZONE` | Date and time |
| `TimeField` | `TIME` | Time of day |
| `DurationField` | `INTERVAL` | Duration / time span |

#### Boolean fields

| Type | SQL | Description |
|------|-----|-------------|
| `BooleanField` | `BOOLEAN` | True / false |
| `NullBooleanField` | `BOOLEAN` (nullable) | True / false / NULL |

#### Binary and file fields

| Type | SQL | Description |
|------|-----|-------------|
| `BinaryField` | `BYTEA` | Raw binary data |
| `FileField` | `VARCHAR(100)` | File upload path |
| `ImageField` | `VARCHAR(100)` | Image upload path |

#### Other fields

| Type | SQL | Description |
|------|-----|-------------|
| `UuidField` | `UUID` | Universally unique identifier |
| `JsonField` | `JSONB` | JSON data |
| `ForeignKey` | `INTEGER REFERENCES ...` | Many-to-one relationship |
| `OneToOneField` | `INTEGER UNIQUE REFERENCES ...` | One-to-one relationship |
| `ManyToManyField` | (junction table) | Many-to-many relationship |

---

## Model inheritance

django-rs supports three types of model inheritance:

### Abstract models

Abstract models define fields and behavior that are inherited by child models. No database table is created for abstract models:

```rust
// The abstract model's fields are copied into child models
ModelMeta {
    abstract_model: true,
    // ...
}
```

### Multi-table inheritance

Each model in the hierarchy gets its own database table. The child table has a foreign key to the parent:

```rust
ModelMeta {
    inheritance_type: InheritanceType::MultiTable,
    // ...
}
```

Override `parent_field_values()` and `child_field_values()` to split field values between parent and child tables.

### Proxy models

Proxy models share the same database table as their parent but can have different Rust-level behavior (methods, Meta options):

```rust
ModelMeta {
    inheritance_type: InheritanceType::Proxy,
    // db_table is the same as the parent
}
```

---

## Value type

The `Value` enum represents all types that can be stored in the database:

```rust
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Date(String),      // YYYY-MM-DD
    DateTime(String),  // ISO 8601
    Time(String),      // HH:MM:SS
    Uuid(String),
    Json(serde_json::Value),
    List(Vec<Value>),
}
```

`Value` implements `From` for common Rust types:

```rust
Value::from(42i64)         // Value::Int(42)
Value::from("hello")       // Value::String("hello".to_string())
Value::from(true)          // Value::Bool(true)
Value::from(3.14f64)       // Value::Float(3.14)
```
