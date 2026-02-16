# Migrations

This reference covers the django-rs migration system. Migrations describe schema changes as a sequence of operations that can be applied forward or rolled back.

---

## Overview

The migration system lives in the `django-rs-db-migrations` crate and consists of:

- **Operations** -- Individual schema changes (create table, add column, add index, etc.)
- **SchemaEditor** -- Backend-specific SQL generation (PostgreSQL, SQLite, MySQL)
- **Autodetect** -- Compares model definitions to the current database state and generates operations
- **ProjectState** -- An in-memory representation of all models and their fields

Each operation implements the `Operation` trait:

```rust
pub trait Operation: Send + Sync {
    fn describe(&self) -> String;
    fn state_forwards(&self, app_label: &str, state: &mut ProjectState);
    fn database_forwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        from_state: &ProjectState,
        to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError>;
    fn database_backwards(
        &self,
        app_label: &str,
        schema_editor: &dyn SchemaEditor,
        from_state: &ProjectState,
        to_state: &ProjectState,
    ) -> Result<Vec<String>, DjangoError>;
    fn reversible(&self) -> bool;
}
```

Every operation knows how to:
1. **Update state** (`state_forwards`) -- modify the in-memory model representation
2. **Generate forward SQL** (`database_forwards`) -- produce DDL to apply the change
3. **Generate backward SQL** (`database_backwards`) -- produce DDL to reverse the change

---

## Schema operations

### CreateModel

Creates a new database table with all specified fields and constraints:

```rust
use django_rs_db_migrations::operations::CreateModel;

let op = CreateModel {
    name: "Post".to_string(),
    fields: vec![
        MigrationFieldDef { name: "id".into(), column: "id".into(), field_type: "BigAutoField".into(), .. },
        MigrationFieldDef { name: "title".into(), column: "title".into(), field_type: "CharField".into(), .. },
    ],
    options: ModelOptions::default(),
};
// Generates: CREATE TABLE "blog_post" ("id" BIGSERIAL PRIMARY KEY, "title" VARCHAR(255) NOT NULL)
```

### DeleteModel

Drops a database table:

```rust
use django_rs_db_migrations::operations::DeleteModel;

let op = DeleteModel { name: "Post".to_string() };
// Generates: DROP TABLE "blog_post"
```

### AddField

Adds a column to an existing table:

```rust
use django_rs_db_migrations::operations::AddField;

let op = AddField {
    model_name: "Post".to_string(),
    field: MigrationFieldDef {
        name: "published".into(),
        column: "published".into(),
        field_type: "BooleanField".into(),
        // ...
    },
};
// Generates: ALTER TABLE "blog_post" ADD COLUMN "published" BOOLEAN NOT NULL DEFAULT false
```

### RemoveField

Drops a column from an existing table:

```rust
use django_rs_db_migrations::operations::RemoveField;

let op = RemoveField {
    model_name: "Post".to_string(),
    field_name: "published".to_string(),
};
// Generates: ALTER TABLE "blog_post" DROP COLUMN "published"
```

### AlterField

Modifies a column's type, nullability, or default:

```rust
use django_rs_db_migrations::operations::AlterField;

let op = AlterField {
    model_name: "Post".to_string(),
    field_name: "title".to_string(),
    field: MigrationFieldDef { /* new definition */ },
};
// Generates: ALTER TABLE "blog_post" ALTER COLUMN "title" TYPE TEXT
```

On SQLite, which does not support `ALTER COLUMN`, this triggers a table recreation (create new table, copy data, drop old, rename).

### RenameField

Renames a column:

```rust
use django_rs_db_migrations::operations::RenameField;

let op = RenameField {
    model_name: "Post".to_string(),
    old_name: "title".to_string(),
    new_name: "headline".to_string(),
};
// Generates: ALTER TABLE "blog_post" RENAME COLUMN "title" TO "headline"
```

### RenameModel

Renames a table:

```rust
use django_rs_db_migrations::operations::RenameModel;

let op = RenameModel {
    old_name: "Post".to_string(),
    new_name: "Article".to_string(),
};
// Generates: ALTER TABLE "blog_post" RENAME TO "blog_article"
```

---

## Index operations

### AddIndex

Creates a database index:

```rust
use django_rs_db_migrations::operations::AddIndex;
use django_rs_db::model::{Index, IndexType};

let op = AddIndex {
    model_name: "Post".to_string(),
    index: Index {
        name: Some("idx_post_slug".to_string()),
        fields: vec!["slug".to_string()],
        unique: true,
        index_type: IndexType::BTree,
        concurrently: false,
        expressions: vec![],
        include: vec![],
        condition: None,
    },
};
// Generates: CREATE UNIQUE INDEX "idx_post_slug" ON "blog_post" USING btree ("slug")
```

For concurrent index creation on PostgreSQL:

```rust
let op = AddIndex {
    model_name: "Post".to_string(),
    index: Index {
        name: Some("idx_post_email_lower".to_string()),
        fields: vec![],
        unique: true,
        index_type: IndexType::BTree,
        concurrently: true,
        expressions: vec!["LOWER(email)".to_string()],
        include: vec![],
        condition: Some("is_active = true".to_string()),
    },
};
// Generates: CREATE UNIQUE INDEX CONCURRENTLY "idx_post_email_lower"
//            ON "blog_post" USING btree (LOWER(email)) WHERE is_active = true
```

### RemoveIndex

Drops a database index:

```rust
use django_rs_db_migrations::operations::RemoveIndex;

let op = RemoveIndex {
    model_name: "Post".to_string(),
    index_name: "idx_post_slug".to_string(),
};
// Generates: DROP INDEX "idx_post_slug"
```

---

## Constraint operations

### AddConstraint

Adds a named constraint (CHECK, UNIQUE, or EXCLUDE) to an existing table:

```rust
use django_rs_db_migrations::operations::AddConstraint;

// Add a check constraint
let op = AddConstraint {
    model_name: "Product".to_string(),
    constraint_name: "price_positive".to_string(),
    constraint_sql: r#""price_positive" CHECK ("price" > 0)"#.to_string(),
};
// Generates: ALTER TABLE "shop_product" ADD CONSTRAINT "price_positive" CHECK ("price" > 0)
```

You can generate the constraint SQL from constraint objects:

```rust
use django_rs_db::constraints::{CheckConstraint, Constraint};
use django_rs_db::query::lookups::{Q, Lookup};
use django_rs_db::value::Value;

let constraint = CheckConstraint::new(
    "price_positive",
    Q::filter("price", Lookup::Gt(Value::from(0))),
);
let sql = constraint.to_sql("products"); // Generate the SQL fragment

let op = AddConstraint {
    model_name: "Product".to_string(),
    constraint_name: "price_positive".to_string(),
    constraint_sql: sql,
};
```

### RemoveConstraint

Drops a named constraint from a table. The `constraint_sql` field stores the original SQL so the operation can be reversed:

```rust
use django_rs_db_migrations::operations::RemoveConstraint;

let op = RemoveConstraint {
    model_name: "Product".to_string(),
    constraint_name: "price_positive".to_string(),
    constraint_sql: r#""price_positive" CHECK ("price" > 0)"#.to_string(),
};
// Generates: ALTER TABLE "shop_product" DROP CONSTRAINT "price_positive"
```

Both `AddConstraint` and `RemoveConstraint` are fully reversible -- `AddConstraint` reversed drops the constraint, and `RemoveConstraint` reversed re-adds it.

### AlterUniqueTogether

Sets the `unique_together` constraint groups on a model:

```rust
use django_rs_db_migrations::operations::AlterUniqueTogether;

let op = AlterUniqueTogether {
    model_name: "Enrollment".to_string(),
    unique_together: vec![
        vec!["student_id".to_string(), "course_id".to_string()],
    ],
};
```

---

## Custom migrations

### RunSQL

Executes raw SQL in a migration. Provide both forward and backward SQL for reversibility:

```rust
use django_rs_db_migrations::operations::RunSQL;

// Reversible: both directions provided
let op = RunSQL {
    sql_forwards: "CREATE EXTENSION IF NOT EXISTS btree_gist".to_string(),
    sql_backwards: "DROP EXTENSION IF EXISTS btree_gist".to_string(),
};

// Irreversible: empty backwards SQL
let op = RunSQL {
    sql_forwards: "UPDATE users SET role = 'member' WHERE role IS NULL".to_string(),
    sql_backwards: String::new(), // Cannot be reversed
};
```

`RunSQL` is useful for:
- Creating PostgreSQL extensions (`btree_gist`, `pg_trgm`, `uuid-ossp`)
- Data migrations (updating existing rows)
- Creating views, triggers, or stored procedures
- Any DDL not covered by the built-in operations

### RunRust

Executes arbitrary Rust code during a migration:

```rust
use django_rs_db_migrations::operations::RunRust;

let op = RunRust {
    description: "Backfill slug field from title".to_string(),
    forwards: Box::new(|| {
        // Query the database and update rows
        println!("Running data migration...");
        Ok(())
    }),
    backwards: Some(Box::new(|| {
        // Reverse the data migration
        Ok(())
    })),
};
```

`RunRust` is useful when the migration logic is too complex for raw SQL -- for example, reading data from one table, transforming it in Rust, and writing it to another.

---

## Database backends

The `SchemaEditor` trait abstracts DDL generation across database backends. django-rs ships with three implementations:

| Backend | Struct | Notes |
|---------|--------|-------|
| PostgreSQL | `PostgresSchemaEditor` | Full support for all index types, concurrent creation, exclusion constraints |
| SQLite | `SqliteSchemaEditor` | Uses table recreation for ALTER COLUMN; partial index support via WHERE |
| MySQL | `MySqlSchemaEditor` | Uses backtick quoting; constraint operations via ALTER TABLE |

The schema editor is selected based on your database configuration. All migration operations delegate SQL generation to the schema editor, so the same operation produces correct SQL for each backend.

---

## Operations reference

| Operation | Description | Reversible |
|-----------|-------------|------------|
| `CreateModel` | Creates a new table | Yes (drops table) |
| `DeleteModel` | Drops a table | Yes (recreates table) |
| `AddField` | Adds a column | Yes (drops column) |
| `RemoveField` | Drops a column | Yes (adds column back) |
| `AlterField` | Changes column type/constraints | Yes (reverts to old type) |
| `RenameField` | Renames a column | Yes (renames back) |
| `RenameModel` | Renames a table | Yes (renames back) |
| `AddIndex` | Creates an index | Yes (drops index) |
| `RemoveIndex` | Drops an index | Yes (recreates index) |
| `AlterUniqueTogether` | Sets unique_together groups | Yes (reverts to old groups) |
| `AddConstraint` | Adds a named constraint | Yes (drops constraint) |
| `RemoveConstraint` | Drops a named constraint | Yes (re-adds constraint) |
| `RunSQL` | Executes raw SQL | Only if backward SQL provided |
| `RunRust` | Executes Rust code | Only if backward closure provided |
