# Database Indexes and Constraints

This reference covers database indexes and constraints in django-rs. Indexes improve query performance, while constraints enforce data integrity rules at the database level.

---

## Indexes

Indexes are defined on a model's `ModelMeta` and are created during migrations. django-rs supports six index types, concurrent creation, expression indexes, covering indexes, and partial indexes.

### Index struct

The `Index` struct represents a database index:

```rust
use django_rs_db::model::{Index, IndexType};

let index = Index {
    name: Some("idx_post_title".to_string()),
    fields: vec!["title".to_string()],
    unique: false,
    index_type: IndexType::BTree,
    concurrently: false,
    expressions: vec![],
    include: vec![],
    condition: None,
};
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | `Option<String>` | The index name (auto-generated if `None`) |
| `fields` | `Vec<String>` | Columns included in the index |
| `unique` | `bool` | Whether this is a unique index |
| `index_type` | `IndexType` | The index algorithm (default: `BTree`) |
| `concurrently` | `bool` | Use `CREATE INDEX CONCURRENTLY` (PostgreSQL only) |
| `expressions` | `Vec<String>` | SQL expressions to index (e.g., `LOWER(email)`) |
| `include` | `Vec<String>` | Columns for a covering index (`INCLUDE` clause) |
| `condition` | `Option<String>` | `WHERE` clause for a partial index |

### Index types

django-rs supports all PostgreSQL index types:

| Type | Enum Variant | SQL | Best For |
|------|-------------|-----|----------|
| B-tree | `IndexType::BTree` | `USING btree` | General-purpose; equality and range queries (default) |
| GIN | `IndexType::Gin` | `USING gin` | Arrays, JSONB, full-text search, hstore |
| GiST | `IndexType::Gist` | `USING gist` | Range types, geometric data, full-text search |
| BRIN | `IndexType::Brin` | `USING brin` | Very large, naturally ordered tables |
| SP-GiST | `IndexType::SpGist` | `USING spgist` | Radix trees, quad-trees, non-balanced structures |
| Bloom | `IndexType::Bloom` | `USING bloom` | Multi-column equality queries (requires `bloom` extension) |

### Specialized index helpers

Each index type has a dedicated helper struct that converts into `Index`:

```rust
use django_rs_db::model::{GinIndex, GistIndex, BrinIndex, SpGistIndex, BloomIndex, Index};

// GIN index for JSONB or full-text search
let gin: Index = GinIndex::new("idx_post_tags", vec!["tags"]).into();

// GiST index for range types
let gist: Index = GistIndex::new("idx_event_range", vec!["time_range"]).into();

// BRIN index for time-series data
let brin: Index = BrinIndex::new("idx_log_created", vec!["created_at"]).into();

// SP-GiST index
let spgist: Index = SpGistIndex::new("idx_ip_range", vec!["ip_addr"]).into();

// Bloom index
let bloom: Index = BloomIndex::new("idx_multi_eq", vec!["col_a", "col_b", "col_c"]).into();
```

### Concurrent index creation

On PostgreSQL, you can create indexes without blocking writes by setting `concurrently: true`. This uses `CREATE INDEX CONCURRENTLY`, which takes longer but does not lock the table for writes:

```rust
let index = Index {
    name: Some("idx_email_concurrent".to_string()),
    fields: vec!["email".to_string()],
    unique: true,
    index_type: IndexType::BTree,
    concurrently: true,
    expressions: vec![],
    include: vec![],
    condition: None,
};

// Generates: CREATE UNIQUE INDEX CONCURRENTLY "idx_email_concurrent"
//            ON "users" USING btree ("email")
```

This is essential for production deployments where you cannot afford downtime during migrations.

### Expression indexes

Expression indexes allow you to index the result of a SQL expression rather than raw column values. This is useful for case-insensitive lookups, computed values, and function-based access patterns:

```rust
let index = Index {
    name: Some("idx_email_lower".to_string()),
    fields: vec![],
    unique: true,
    index_type: IndexType::BTree,
    concurrently: false,
    expressions: vec!["LOWER(email)".to_string()],
    include: vec![],
    condition: None,
};

// Generates: CREATE UNIQUE INDEX "idx_email_lower"
//            ON "users" USING btree (LOWER(email))
```

You can combine regular columns and expressions:

```rust
let index = Index {
    name: Some("idx_mixed".to_string()),
    fields: vec!["status".to_string()],
    unique: false,
    index_type: IndexType::BTree,
    concurrently: false,
    expressions: vec!["LOWER(email)".to_string()],
    include: vec![],
    condition: None,
};

// Generates: CREATE INDEX "idx_mixed"
//            ON "users" USING btree ("status", LOWER(email))
```

### Covering indexes

Covering indexes use the `INCLUDE` clause to store additional columns in the index leaf pages. This enables index-only scans for queries that need those extra columns, avoiding table lookups entirely:

```rust
let index = Index {
    name: Some("idx_post_covering".to_string()),
    fields: vec!["published_at".to_string()],
    unique: false,
    index_type: IndexType::BTree,
    concurrently: false,
    expressions: vec![],
    include: vec!["title".to_string(), "author_id".to_string()],
    condition: None,
};

// Generates: CREATE INDEX "idx_post_covering"
//            ON "blog_post" USING btree ("published_at")
//            INCLUDE ("title", "author_id")
```

### Partial indexes

Partial indexes only index rows that match a `WHERE` condition. This reduces index size and speeds up queries that filter on the same condition:

```rust
let index = Index {
    name: Some("idx_active_users".to_string()),
    fields: vec!["email".to_string()],
    unique: true,
    index_type: IndexType::BTree,
    concurrently: false,
    expressions: vec![],
    include: vec![],
    condition: Some("is_active = true".to_string()),
};

// Generates: CREATE UNIQUE INDEX "idx_active_users"
//            ON "users" USING btree ("email")
//            WHERE is_active = true
```

### Combining features

All index features can be combined. Here is a concurrent, partial, covering expression index:

```rust
let index = Index {
    name: Some("idx_advanced".to_string()),
    fields: vec![],
    unique: true,
    index_type: IndexType::BTree,
    concurrently: true,
    expressions: vec!["LOWER(email)".to_string()],
    include: vec!["name".to_string()],
    condition: Some("is_active = true".to_string()),
};

// Generates: CREATE UNIQUE INDEX CONCURRENTLY "idx_advanced"
//            ON "users" USING btree (LOWER(email))
//            INCLUDE ("name") WHERE is_active = true
```

### Adding indexes to a model

Indexes are declared in the model's `ModelMeta`:

```rust
use django_rs_db::model::{ModelMeta, Index, IndexType, GinIndex};

static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
    app_label: "blog",
    model_name: "post",
    db_table: "blog_post".to_string(),
    // ... other fields ...
    indexes: vec![
        // Simple B-tree index
        Index {
            name: Some("idx_post_slug".to_string()),
            fields: vec!["slug".to_string()],
            unique: true,
            index_type: IndexType::BTree,
            concurrently: false,
            expressions: vec![],
            include: vec![],
            condition: None,
        },
        // GIN index for full-text search
        GinIndex::new("idx_post_search", vec!["search_vector"]).into(),
    ],
    // ...
});
```

---

## Constraints

Constraints enforce data integrity rules at the database level. django-rs supports three constraint types: `CheckConstraint`, `UniqueConstraint`, and `ExclusionConstraint`.

### The Constraint trait

All constraint types implement the `Constraint` trait:

```rust
pub trait Constraint: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &str;
    fn to_sql(&self, table: &str) -> String;
    fn create_sql(&self, table: &str) -> String;
    fn drop_sql(&self, table: &str) -> String;
}
```

| Method | Description |
|--------|-------------|
| `name()` | Returns the constraint name |
| `to_sql(table)` | Generates the constraint definition SQL |
| `create_sql(table)` | Generates `ALTER TABLE ... ADD CONSTRAINT` SQL |
| `drop_sql(table)` | Generates `ALTER TABLE ... DROP CONSTRAINT` SQL |

### CheckConstraint

Enforces that a condition is true for every row. Wraps a `Q` object as the condition:

```rust
use django_rs_db::constraints::{CheckConstraint, Constraint};
use django_rs_db::query::lookups::{Q, Lookup};
use django_rs_db::value::Value;

// Ensure price is always positive
let constraint = CheckConstraint::new(
    "price_positive",
    Q::filter("price", Lookup::Gt(Value::from(0))),
);

let sql = constraint.to_sql("products");
// "price_positive" CHECK ("price" > 0)
```

Check constraints use the standard `Q` query builder, so you can compose complex conditions:

```rust
// Price must be positive AND discount cannot exceed price
let q = Q::filter("price", Lookup::Gt(Value::from(0)))
    & Q::filter("discount", Lookup::Lte(Value::from(100)));

let constraint = CheckConstraint::new("valid_pricing", q);
```

### UniqueConstraint

Enforces uniqueness across one or more columns:

```rust
use django_rs_db::constraints::{UniqueConstraint, Constraint};

// Simple unique constraint
let constraint = UniqueConstraint::new(
    "unique_email",
    vec!["email".to_string()],
);

// Multi-column unique constraint
let constraint = UniqueConstraint::new(
    "unique_user_project",
    vec!["user_id".to_string(), "project_id".to_string()],
);
```

**Conditional unique constraints** restrict uniqueness to rows matching a condition (partial unique index):

```rust
use django_rs_db::query::lookups::{Q, Lookup};
use django_rs_db::value::Value;

let constraint = UniqueConstraint::new(
    "unique_active_email",
    vec!["email".to_string()],
).condition(Q::filter("is_active", Lookup::Exact(Value::Bool(true))));
```

**NULLS NOT DISTINCT** (PostgreSQL 15+) controls whether NULL values count as duplicates:

```rust
let constraint = UniqueConstraint::new(
    "unique_nullable",
    vec!["external_id".to_string()],
).nulls_distinct(false); // At most one NULL allowed
```

### ExclusionConstraint

Exclusion constraints prevent overlapping values using operators. They are a PostgreSQL-only generalization of unique constraints. Common use case: preventing overlapping time ranges for the same resource.

```rust
use django_rs_db::constraints::{ExclusionConstraint, Constraint};

// Prevent overlapping reservations for the same room
let constraint = ExclusionConstraint::new(
    "no_overlapping_reservations",
    vec![
        ("room_id".to_string(), "=".to_string()),
        ("during".to_string(), "&&".to_string()),
    ],
);

let sql = constraint.to_sql("reservations");
// "no_overlapping_reservations" EXCLUDE USING gist ("room_id" WITH =, "during" WITH &&)
```

**Custom index type:**

```rust
// Use SP-GiST instead of the default GiST
let constraint = ExclusionConstraint::new(
    "no_overlap",
    vec![("range_col".to_string(), "&&".to_string())],
).using("spgist");
```

**Conditional exclusion constraint:**

```rust
use django_rs_db::query::lookups::{Q, Lookup};
use django_rs_db::value::Value;

let constraint = ExclusionConstraint::new(
    "no_active_overlap",
    vec![
        ("room_id".to_string(), "=".to_string()),
        ("during".to_string(), "&&".to_string()),
    ],
).condition(Q::filter("cancelled", Lookup::Exact(Value::Bool(false))));
```

Common exclusion operators:

| Operator | Meaning | Use Case |
|----------|---------|----------|
| `=` | Equality | Same resource (room, user, etc.) |
| `&&` | Overlap | Overlapping ranges or arrays |
| `<>` | Not equal | Different values |
| `@>` | Contains | Range containment |
| `<<` | Strictly left of | Non-overlapping ranges |
| `>>` | Strictly right of | Non-overlapping ranges |

### Adding constraints to a model

Constraints are declared in the model's `ModelMeta.constraints` field using boxed trait objects:

```rust
use django_rs_db::constraints::{CheckConstraint, UniqueConstraint, Constraint};
use django_rs_db::query::lookups::{Q, Lookup};
use django_rs_db::value::Value;

static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
    // ... other fields ...
    constraints: vec![
        Box::new(CheckConstraint::new(
            "age_non_negative",
            Q::filter("age", Lookup::Gte(Value::from(0))),
        )),
        Box::new(UniqueConstraint::new(
            "unique_email",
            vec!["email".to_string()],
        )),
    ],
    // ...
});
```

---

## Comparison with Django

| Django (Python) | django-rs (Rust) |
|-----------------|------------------|
| `models.Index(fields=["title"])` | `Index { fields: vec!["title".into()], ..Default::default() }` |
| `models.Index(fields=["title"], name="idx")` | `Index { name: Some("idx".into()), fields: vec!["title".into()], .. }` |
| `GinIndex(fields=["tags"])` | `GinIndex::new("idx_tags", vec!["tags"]).into()` |
| `GistIndex(fields=["range"])` | `GistIndex::new("idx_range", vec!["range"]).into()` |
| `BrinIndex(fields=["created"])` | `BrinIndex::new("idx_created", vec!["created"]).into()` |
| `CheckConstraint(check=Q(age__gte=0), name="...")` | `CheckConstraint::new("...", Q::filter("age", Lookup::Gte(...)))` |
| `UniqueConstraint(fields=["email"], name="...")` | `UniqueConstraint::new("...", vec!["email".into()])` |
| `ExclusionConstraint(expressions=[...])` | `ExclusionConstraint::new("...", vec![...])` |
