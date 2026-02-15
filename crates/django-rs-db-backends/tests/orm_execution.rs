//! Integration tests for the ORM execution pipeline.
//!
//! These tests verify the complete round-trip from QuerySet/Model CRUD through
//! SQL compilation, execution on a real SQLite database, and result mapping
//! back to model instances.

use django_rs_core::{DjangoError, DjangoResult};
use django_rs_db::executor::{
    create_model, delete_model, refresh_model, save_model, DbExecutor, ModelLifecycleHooks,
};
use django_rs_db::fields::{FieldDef, FieldType};
use django_rs_db::model::{Model, ModelMeta};
use django_rs_db::query::compiler::{OrderBy, Row};
use django_rs_db::query::lookups::{Lookup, Q};
use django_rs_db::value::Value;
use django_rs_db_backends::SqliteBackend;

use django_rs_db_backends::DatabaseBackend;

// ── Test model definitions ────────────────────────────────────────────

#[derive(Debug, Clone)]
struct User {
    id: i64,
    name: String,
    age: i64,
    email: String,
}

impl Model for User {
    fn meta() -> &'static ModelMeta {
        use std::sync::LazyLock;
        static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
            app_label: "auth",
            model_name: "user",
            db_table: "auth_user".to_string(),
            verbose_name: "user".to_string(),
            verbose_name_plural: "users".to_string(),
            ordering: vec![],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("name", FieldType::CharField).max_length(100),
                FieldDef::new("age", FieldType::IntegerField),
                FieldDef::new("email", FieldType::CharField).max_length(200),
            ],
        });
        &META
    }

    fn table_name() -> &'static str { "auth_user" }
    fn app_label() -> &'static str { "auth" }

    fn pk(&self) -> Option<&Value> {
        if self.id == 0 { None } else { None }
    }

    fn set_pk(&mut self, value: Value) {
        if let Value::Int(id) = value { self.id = id; }
    }

    fn pk_field_name() -> &'static str { "id" }

    fn field_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::Int(self.id)),
            ("name", Value::String(self.name.clone())),
            ("age", Value::Int(self.age)),
            ("email", Value::String(self.email.clone())),
        ]
    }

    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        Ok(User {
            id: row.get("id")?,
            name: row.get("name")?,
            age: row.get("age")?,
            email: row.get("email")?,
        })
    }
}

// A model with proper pk() that returns Some when id is set
#[derive(Debug, Clone)]
struct Product {
    pk_value: Value,
    id: i64,
    name: String,
    price: f64,
}

impl Product {
    fn new(name: &str, price: f64) -> Self {
        Self { pk_value: Value::Null, id: 0, name: name.to_string(), price }
    }
}

impl Model for Product {
    fn meta() -> &'static ModelMeta {
        use std::sync::LazyLock;
        static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
            app_label: "shop", model_name: "product",
            db_table: "shop_product".to_string(),
            verbose_name: "product".to_string(),
            verbose_name_plural: "products".to_string(),
            ordering: vec![], unique_together: vec![],
            indexes: vec![], abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("name", FieldType::CharField).max_length(200),
                FieldDef::new("price", FieldType::FloatField),
            ],
        });
        &META
    }
    fn table_name() -> &'static str { "shop_product" }
    fn app_label() -> &'static str { "shop" }
    fn pk(&self) -> Option<&Value> {
        if self.id == 0 { None } else { Some(&self.pk_value) }
    }
    fn set_pk(&mut self, value: Value) {
        if let Value::Int(id) = &value { self.id = *id; }
        self.pk_value = value;
    }
    fn pk_field_name() -> &'static str { "id" }
    fn field_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::Int(self.id)),
            ("name", Value::String(self.name.clone())),
            ("price", Value::Float(self.price)),
        ]
    }
    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        let id: i64 = row.get("id")?;
        let price_val: Value = row.get("price")?;
        let price = match price_val {
            Value::Float(f) => f,
            Value::Int(i) => i as f64,
            _ => 0.0,
        };
        Ok(Product { pk_value: Value::Int(id), id, name: row.get("name")?, price })
    }
}

// Model with lifecycle hooks
#[derive(Debug, Clone)]
struct HookedProduct {
    pk_value: Value,
    id: i64,
    name: String,
    count: i64,
}

impl Model for HookedProduct {
    fn meta() -> &'static ModelMeta {
        use std::sync::LazyLock;
        static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
            app_label: "test", model_name: "hookedproduct",
            db_table: "test_hooked".to_string(),
            verbose_name: "hooked".to_string(),
            verbose_name_plural: "hookeds".to_string(),
            ordering: vec![], unique_together: vec![],
            indexes: vec![], abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("name", FieldType::CharField).max_length(100),
                FieldDef::new("count", FieldType::IntegerField),
            ],
        });
        &META
    }
    fn table_name() -> &'static str { "test_hooked" }
    fn app_label() -> &'static str { "test" }
    fn pk(&self) -> Option<&Value> {
        if self.id == 0 { None } else { Some(&self.pk_value) }
    }
    fn set_pk(&mut self, value: Value) {
        if let Value::Int(id) = &value { self.id = *id; }
        self.pk_value = value;
    }
    fn pk_field_name() -> &'static str { "id" }
    fn field_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::Int(self.id)),
            ("name", Value::String(self.name.clone())),
            ("count", Value::Int(self.count)),
        ]
    }
    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        let id: i64 = row.get("id")?;
        Ok(HookedProduct { pk_value: Value::Int(id), id, name: row.get("name")?, count: row.get("count")? })
    }
}

impl ModelLifecycleHooks for HookedProduct {
    fn on_pre_save(&self) -> DjangoResult<()> {
        if self.name.is_empty() {
            return Err(DjangoError::DatabaseError("Name cannot be empty".to_string()));
        }
        Ok(())
    }
    fn on_pre_delete(&self) -> DjangoResult<()> { Ok(()) }
}

// ── Helper functions ──────────────────────────────────────────────────

async fn setup_user_db() -> SqliteBackend {
    let db = SqliteBackend::memory().unwrap();
    db.execute("CREATE TABLE auth_user (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, age INTEGER NOT NULL, email TEXT NOT NULL)", &[]).await.unwrap();
    db
}

async fn setup_product_db() -> SqliteBackend {
    let db = SqliteBackend::memory().unwrap();
    db.execute("CREATE TABLE shop_product (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, price REAL NOT NULL)", &[]).await.unwrap();
    db
}

async fn setup_hooked_db() -> SqliteBackend {
    let db = SqliteBackend::memory().unwrap();
    db.execute("CREATE TABLE test_hooked (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, count INTEGER NOT NULL DEFAULT 0)", &[]).await.unwrap();
    db
}

async fn seed_users(db: &SqliteBackend) {
    for (name, age, email) in [
        ("Alice", 30, "alice@example.com"),
        ("Bob", 25, "bob@example.com"),
        ("Charlie", 35, "charlie@example.com"),
        ("Diana", 28, "diana@example.com"),
        ("Eve", 22, "eve@example.com"),
    ] {
        db.execute("INSERT INTO auth_user (name, age, email) VALUES (?, ?, ?)",
            &[Value::from(name), Value::from(age as i64), Value::from(email)]).await.unwrap();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// QUERYSET EXECUTION TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_qs_execute_all() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let users = mgr.all().execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 5);
}

#[tokio::test]
async fn test_qs_execute_filter_exact() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let users = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Alice")))).execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[0].age, 30);
}

#[tokio::test]
async fn test_qs_execute_filter_gt() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Gt(Value::from(28))))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_qs_execute_filter_gte() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Gte(Value::from(28))))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 3);
}

#[tokio::test]
async fn test_qs_execute_filter_lt() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Lt(Value::from(26))))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_qs_execute_filter_lte() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Lte(Value::from(25))))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_qs_execute_filter_contains() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Contains("li".to_string())))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 2); // Alice, Charlie
}

#[tokio::test]
async fn test_qs_execute_filter_startswith() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::StartsWith("Ch".to_string())))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Charlie");
}

#[tokio::test]
async fn test_qs_execute_filter_endswith() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::EndsWith("e".to_string())))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 3); // Alice, Charlie, Eve
}

#[tokio::test]
async fn test_qs_execute_filter_in() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::In(vec![Value::from("Alice"), Value::from("Bob")])))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_qs_execute_filter_range() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Range(Value::from(25), Value::from(30))))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 3);
}

#[tokio::test]
async fn test_qs_execute_filter_and() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Gte(Value::from(25))) & Q::filter("age", Lookup::Lte(Value::from(30))))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 3);
}

#[tokio::test]
async fn test_qs_execute_filter_or() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))) | Q::filter("name", Lookup::Exact(Value::from("Eve"))))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_qs_execute_exclude() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .exclude(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 4);
    assert!(users.iter().all(|u| u.name != "Alice"));
}

#[tokio::test]
async fn test_qs_execute_order_asc() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new().all()
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db).await.unwrap();
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[4].name, "Eve");
}

#[tokio::test]
async fn test_qs_execute_order_desc() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new().all()
        .order_by(vec![OrderBy::desc("age")])
        .execute_query(&db).await.unwrap();
    assert_eq!(users[0].name, "Charlie");
    assert_eq!(users[4].name, "Eve");
}

#[tokio::test]
async fn test_qs_execute_limit() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new().all()
        .order_by(vec![OrderBy::asc("name")])
        .limit(2)
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[1].name, "Bob");
}

#[tokio::test]
async fn test_qs_execute_offset() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new().all()
        .order_by(vec![OrderBy::asc("name")])
        .limit(2).offset(2)
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Charlie");
    assert_eq!(users[1].name, "Diana");
}

#[tokio::test]
async fn test_qs_execute_none() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new().none().execute_query(&db).await.unwrap();
    assert!(users.is_empty());
}

#[tokio::test]
async fn test_qs_execute_empty_table() {
    let db = setup_user_db().await;
    let users = django_rs_db::Manager::<User>::new().all().execute_query(&db).await.unwrap();
    assert!(users.is_empty());
}

#[tokio::test]
async fn test_qs_execute_chained_filters() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new().all()
        .filter(Q::filter("age", Lookup::Gte(Value::from(25))))
        .filter(Q::filter("age", Lookup::Lte(Value::from(30))))
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 3);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[1].name, "Bob");
    assert_eq!(users[2].name, "Diana");
}

#[tokio::test]
async fn test_qs_execute_reverse() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new().all()
        .order_by(vec![OrderBy::asc("name")])
        .reverse()
        .limit(2)
        .execute_query(&db).await.unwrap();
    assert_eq!(users[0].name, "Eve");
    assert_eq!(users[1].name, "Diana");
}

// ── count_exec ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_count_all() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    assert_eq!(django_rs_db::Manager::<User>::new().all().count_exec(&db).await.unwrap(), 5);
}

#[tokio::test]
async fn test_qs_count_filtered() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let c = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Gt(Value::from(28))))
        .count_exec(&db).await.unwrap();
    assert_eq!(c, 2);
}

#[tokio::test]
async fn test_qs_count_none() {
    let db = setup_user_db().await;
    assert_eq!(django_rs_db::Manager::<User>::new().none().count_exec(&db).await.unwrap(), 0);
}

#[tokio::test]
async fn test_qs_count_empty() {
    let db = setup_user_db().await;
    assert_eq!(django_rs_db::Manager::<User>::new().all().count_exec(&db).await.unwrap(), 0);
}

// ── exists_exec ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_exists_true() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    assert!(django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .exists_exec(&db).await.unwrap());
}

#[tokio::test]
async fn test_qs_exists_false() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    assert!(!django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Nobody"))))
        .exists_exec(&db).await.unwrap());
}

#[tokio::test]
async fn test_qs_exists_none() {
    let db = setup_user_db().await;
    assert!(!django_rs_db::Manager::<User>::new().none().exists_exec(&db).await.unwrap());
}

#[tokio::test]
async fn test_qs_exists_empty() {
    let db = setup_user_db().await;
    assert!(!django_rs_db::Manager::<User>::new().all().exists_exec(&db).await.unwrap());
}

// ── first_exec ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_first_some() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let u = django_rs_db::Manager::<User>::new().all()
        .order_by(vec![OrderBy::asc("name")])
        .first_exec(&db).await.unwrap();
    assert_eq!(u.unwrap().name, "Alice");
}

#[tokio::test]
async fn test_qs_first_none() {
    let db = setup_user_db().await;
    assert!(django_rs_db::Manager::<User>::new().all().first_exec(&db).await.unwrap().is_none());
}

#[tokio::test]
async fn test_qs_first_with_filter() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let u = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Gt(Value::from(30))))
        .first_exec(&db).await.unwrap();
    assert_eq!(u.unwrap().name, "Charlie");
}

#[tokio::test]
async fn test_qs_first_none_qs() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    assert!(django_rs_db::Manager::<User>::new().none().first_exec(&db).await.unwrap().is_none());
}

// ── get_exec ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_get_found() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let u = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .get_exec(&db).await.unwrap();
    assert_eq!(u.name, "Alice");
    assert_eq!(u.age, 30);
}

#[tokio::test]
async fn test_qs_get_not_found() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let r = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Nobody"))))
        .get_exec(&db).await;
    assert!(matches!(r, Err(DjangoError::DoesNotExist(_))));
}

#[tokio::test]
async fn test_qs_get_multiple() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let r = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Gt(Value::from(24))))
        .get_exec(&db).await;
    assert!(matches!(r, Err(DjangoError::MultipleObjectsReturned(_))));
}

#[tokio::test]
async fn test_qs_get_none_qs() {
    let db = setup_user_db().await;
    let r = django_rs_db::Manager::<User>::new().none().get_exec(&db).await;
    assert!(matches!(r, Err(DjangoError::DoesNotExist(_))));
}

// ── update_exec ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_update_single() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let a = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .update(vec![("age", Value::from(31))])
        .update_exec(&db).await.unwrap();
    assert_eq!(a, 1);
    let u = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Alice")))).get_exec(&db).await.unwrap();
    assert_eq!(u.age, 31);
}

#[tokio::test]
async fn test_qs_update_multiple() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let a = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Lt(Value::from(30))))
        .update(vec![("email", Value::from("updated@test.com"))])
        .update_exec(&db).await.unwrap();
    assert_eq!(a, 3);
}

#[tokio::test]
async fn test_qs_update_no_match() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let a = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Nobody"))))
        .update(vec![("age", Value::from(99))])
        .update_exec(&db).await.unwrap();
    assert_eq!(a, 0);
}

#[tokio::test]
async fn test_qs_update_none_qs() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let a = django_rs_db::Manager::<User>::new().none()
        .update(vec![("age", Value::from(99))])
        .update_exec(&db).await.unwrap();
    assert_eq!(a, 0);
}

#[tokio::test]
async fn test_qs_update_no_pending() {
    let db = setup_user_db().await;
    assert!(django_rs_db::Manager::<User>::new().all().update_exec(&db).await.is_err());
}

// ── delete_exec ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_delete_single() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let a = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .delete().delete_exec(&db).await.unwrap();
    assert_eq!(a, 1);
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 4);
}

#[tokio::test]
async fn test_qs_delete_multiple() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let a = mgr.filter(Q::filter("age", Lookup::Lt(Value::from(30))))
        .delete().delete_exec(&db).await.unwrap();
    assert_eq!(a, 3);
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 2);
}

#[tokio::test]
async fn test_qs_delete_all() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let a = mgr.all().delete().delete_exec(&db).await.unwrap();
    assert_eq!(a, 5);
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 0);
}

#[tokio::test]
async fn test_qs_delete_none_qs() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    assert_eq!(django_rs_db::Manager::<User>::new().none().delete().delete_exec(&db).await.unwrap(), 0);
}

#[tokio::test]
async fn test_qs_delete_no_pending() {
    let db = setup_user_db().await;
    assert!(django_rs_db::Manager::<User>::new().all().delete_exec(&db).await.is_err());
}

// ── create_exec ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_create_exec() {
    let db = setup_user_db().await;
    let mgr = django_rs_db::Manager::<User>::new();
    let pk = mgr.create(vec![
        ("name", Value::from("TestUser")),
        ("age", Value::from(42)),
        ("email", Value::from("test@test.com")),
    ]).create_exec(&db).await.unwrap();
    assert_eq!(pk, Value::Int(1));
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 1);
}

#[tokio::test]
async fn test_qs_create_exec_no_pending() {
    let db = setup_user_db().await;
    assert!(django_rs_db::Manager::<User>::new().all().create_exec(&db).await.is_err());
}

// ═══════════════════════════════════════════════════════════════════════
// MODEL CRUD TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_create_model_basic() {
    let db = setup_product_db().await;
    let mut p = Product::new("Widget", 9.99);
    create_model(&mut p, &db).await.unwrap();
    assert_eq!(p.id, 1);
}

#[tokio::test]
async fn test_create_model_multiple() {
    let db = setup_product_db().await;
    let mut p1 = Product::new("A", 1.0);
    let mut p2 = Product::new("B", 2.0);
    create_model(&mut p1, &db).await.unwrap();
    create_model(&mut p2, &db).await.unwrap();
    assert_eq!(p1.id, 1);
    assert_eq!(p2.id, 2);
}

#[tokio::test]
async fn test_create_and_query_back() {
    let db = setup_product_db().await;
    let mut p = Product::new("Gadget", 29.99);
    create_model(&mut p, &db).await.unwrap();
    let fetched = django_rs_db::Manager::<Product>::new()
        .filter(Q::filter("id", Lookup::Exact(Value::from(p.id))))
        .get_exec(&db).await.unwrap();
    assert_eq!(fetched.name, "Gadget");
    assert!((fetched.price - 29.99).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_save_model_insert() {
    let db = setup_product_db().await;
    let mut p = Product::new("SaveInsert", 5.0);
    save_model(&mut p, &db).await.unwrap();
    assert!(p.id > 0);
}

#[tokio::test]
async fn test_save_model_update() {
    let db = setup_product_db().await;
    let mut p = Product::new("Widget", 9.99);
    create_model(&mut p, &db).await.unwrap();
    p.price = 12.99;
    save_model(&mut p, &db).await.unwrap();
    let fetched = django_rs_db::Manager::<Product>::new()
        .filter(Q::filter("id", Lookup::Exact(Value::from(p.id))))
        .get_exec(&db).await.unwrap();
    assert!((fetched.price - 12.99).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_delete_model_basic() {
    let db = setup_product_db().await;
    let mut p = Product::new("ToDelete", 1.0);
    create_model(&mut p, &db).await.unwrap();
    let a = delete_model(&p, &db).await.unwrap();
    assert_eq!(a, 1);
    assert_eq!(django_rs_db::Manager::<Product>::new().all().count_exec(&db).await.unwrap(), 0);
}

#[tokio::test]
async fn test_delete_unsaved_fails() {
    let db = setup_product_db().await;
    let p = Product::new("Unsaved", 1.0);
    assert!(delete_model(&p, &db).await.is_err());
}

#[tokio::test]
async fn test_refresh_model() {
    let db = setup_product_db().await;
    let mut p = Product::new("Gizmo", 5.99);
    create_model(&mut p, &db).await.unwrap();
    db.execute("UPDATE shop_product SET price = 7.99 WHERE id = ?", &[Value::from(p.id)]).await.unwrap();
    assert!((p.price - 5.99).abs() < f64::EPSILON);
    refresh_model(&mut p, &db).await.unwrap();
    assert!((p.price - 7.99).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_refresh_unsaved_fails() {
    let db = setup_product_db().await;
    let mut p = Product::new("Unsaved", 1.0);
    assert!(refresh_model(&mut p, &db).await.is_err());
}

#[tokio::test]
async fn test_full_crud_lifecycle() {
    let db = setup_product_db().await;
    let mgr = django_rs_db::Manager::<Product>::new();

    let mut p1 = Product::new("Laptop", 999.99);
    let mut p2 = Product::new("Mouse", 29.99);
    let mut p3 = Product::new("Keyboard", 79.99);
    create_model(&mut p1, &db).await.unwrap();
    create_model(&mut p2, &db).await.unwrap();
    create_model(&mut p3, &db).await.unwrap();
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 3);

    let all = mgr.all().order_by(vec![OrderBy::asc("name")]).execute_query(&db).await.unwrap();
    assert_eq!(all[0].name, "Keyboard");
    assert_eq!(all[1].name, "Laptop");
    assert_eq!(all[2].name, "Mouse");

    p2.price = 19.99;
    save_model(&mut p2, &db).await.unwrap();
    let updated = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Mouse")))).get_exec(&db).await.unwrap();
    assert!((updated.price - 19.99).abs() < f64::EPSILON);

    delete_model(&p1, &db).await.unwrap();
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 2);

    let remaining = mgr.all().order_by(vec![OrderBy::asc("name")]).execute_query(&db).await.unwrap();
    assert_eq!(remaining.len(), 2);
    assert_eq!(remaining[0].name, "Keyboard");
    assert_eq!(remaining[1].name, "Mouse");
}

// ═══════════════════════════════════════════════════════════════════════
// LIFECYCLE HOOKS TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_hooks_pre_save_rejects_empty() {
    let db = setup_hooked_db().await;
    let mut m = HookedProduct { pk_value: Value::Null, id: 0, name: String::new(), count: 0 };
    assert!(django_rs_db::executor::create_model_with_hooks(&mut m, &db).await.is_err());
}

#[tokio::test]
async fn test_hooks_pre_save_allows_valid() {
    let db = setup_hooked_db().await;
    let mut m = HookedProduct { pk_value: Value::Null, id: 0, name: "Valid".to_string(), count: 0 };
    django_rs_db::executor::create_model_with_hooks(&mut m, &db).await.unwrap();
    assert!(m.id > 0);
}

#[tokio::test]
async fn test_hooks_save_with_hooks() {
    let db = setup_hooked_db().await;
    let mut m = HookedProduct { pk_value: Value::Null, id: 0, name: "Test".to_string(), count: 0 };
    django_rs_db::executor::save_model_with_hooks(&mut m, &db).await.unwrap();
    assert!(m.id > 0);
}

#[tokio::test]
async fn test_hooks_delete_with_hooks() {
    let db = setup_hooked_db().await;
    let mut m = HookedProduct { pk_value: Value::Null, id: 0, name: "ToDelete".to_string(), count: 0 };
    create_model(&mut m, &db).await.unwrap();
    let a = django_rs_db::executor::delete_model_with_hooks(&m, &db).await.unwrap();
    assert_eq!(a, 1);
}

// ═══════════════════════════════════════════════════════════════════════
// DB EXECUTOR TRAIT TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_executor_backend_type() {
    let db = SqliteBackend::memory().unwrap();
    let ex: &dyn DbExecutor = &db;
    assert_eq!(ex.backend_type(), django_rs_db::DatabaseBackendType::SQLite);
}

#[tokio::test]
async fn test_executor_insert_returning_id() {
    let db = SqliteBackend::memory().unwrap();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT)", &[]).await.unwrap();
    let ex: &dyn DbExecutor = &db;
    assert_eq!(ex.insert_returning_id("INSERT INTO t (val) VALUES (?)", &[Value::from("a")]).await.unwrap(), Value::Int(1));
    assert_eq!(ex.insert_returning_id("INSERT INTO t (val) VALUES (?)", &[Value::from("b")]).await.unwrap(), Value::Int(2));
}

#[tokio::test]
async fn test_executor_execute_sql() {
    let db = SqliteBackend::memory().unwrap();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT)", &[]).await.unwrap();
    let ex: &dyn DbExecutor = &db;
    assert_eq!(ex.execute_sql("INSERT INTO t (val) VALUES (?)", &[Value::from("x")]).await.unwrap(), 1);
}

#[tokio::test]
async fn test_executor_query() {
    let db = SqliteBackend::memory().unwrap();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT)", &[]).await.unwrap();
    db.execute("INSERT INTO t (val) VALUES (?)", &[Value::from("a")]).await.unwrap();
    db.execute("INSERT INTO t (val) VALUES (?)", &[Value::from("b")]).await.unwrap();
    let ex: &dyn DbExecutor = &db;
    let rows = ex.query("SELECT * FROM t ORDER BY id", &[]).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<String>("val").unwrap(), "a");
}

#[tokio::test]
async fn test_executor_query_one_found() {
    let db = SqliteBackend::memory().unwrap();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT)", &[]).await.unwrap();
    db.execute("INSERT INTO t (val) VALUES (?)", &[Value::from("only")]).await.unwrap();
    let ex: &dyn DbExecutor = &db;
    let row = ex.query_one("SELECT val FROM t WHERE id = ?", &[Value::from(1)]).await.unwrap();
    assert_eq!(row.get::<String>("val").unwrap(), "only");
}

#[tokio::test]
async fn test_executor_query_one_not_found() {
    let db = SqliteBackend::memory().unwrap();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT)", &[]).await.unwrap();
    let ex: &dyn DbExecutor = &db;
    assert!(matches!(ex.query_one("SELECT id FROM t WHERE id = ?", &[Value::from(999)]).await, Err(DjangoError::DoesNotExist(_))));
}

// ═══════════════════════════════════════════════════════════════════════
// COMPLEX QUERY TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_complex_filter_chain() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new().all()
        .filter(Q::filter("name", Lookup::Contains("li".to_string())) | Q::filter("name", Lookup::Contains("ob".to_string())))
        .filter(Q::filter("age", Lookup::Gte(Value::from(25))))
        .exclude(Q::filter("age", Lookup::Gt(Value::from(32))))
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db).await.unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[1].name, "Bob");
}

#[tokio::test]
async fn test_pagination() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let p1 = mgr.all().order_by(vec![OrderBy::asc("name")]).limit(2).offset(0).execute_query(&db).await.unwrap();
    let p2 = mgr.all().order_by(vec![OrderBy::asc("name")]).limit(2).offset(2).execute_query(&db).await.unwrap();
    let p3 = mgr.all().order_by(vec![OrderBy::asc("name")]).limit(2).offset(4).execute_query(&db).await.unwrap();
    assert_eq!(p1.len(), 2);
    assert_eq!(p2.len(), 2);
    assert_eq!(p3.len(), 1);
    assert_eq!(p1[0].name, "Alice");
    assert_eq!(p2[0].name, "Charlie");
    assert_eq!(p3[0].name, "Eve");
}

#[tokio::test]
async fn test_isnull_filter() {
    let db = SqliteBackend::memory().unwrap();
    db.execute("CREATE TABLE nt (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, bio TEXT)", &[]).await.unwrap();
    db.execute("INSERT INTO nt (name, bio) VALUES (?, ?)", &[Value::from("A"), Value::from("Has bio")]).await.unwrap();
    db.execute("INSERT INTO nt (name, bio) VALUES (?, ?)", &[Value::from("B"), Value::Null]).await.unwrap();

    #[derive(Debug)]
    struct NullUser { id: i64, name: String, bio: Option<String> }
    impl Model for NullUser {
        fn meta() -> &'static ModelMeta {
            use std::sync::LazyLock;
            static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
                app_label: "test", model_name: "nulluser", db_table: "nt".to_string(),
                verbose_name: "nu".to_string(), verbose_name_plural: "nus".to_string(),
                ordering: vec![], unique_together: vec![], indexes: vec![], abstract_model: false, fields: vec![],
            });
            &META
        }
        fn table_name() -> &'static str { "nt" }
        fn app_label() -> &'static str { "test" }
        fn pk(&self) -> Option<&Value> { None }
        fn set_pk(&mut self, value: Value) { if let Value::Int(id) = value { self.id = id; } }
        fn field_values(&self) -> Vec<(&'static str, Value)> {
            vec![("id", Value::Int(self.id)), ("name", Value::String(self.name.clone())), ("bio", Value::from(self.bio.clone()))]
        }
        fn from_row(row: &Row) -> Result<Self, DjangoError> {
            Ok(NullUser { id: row.get("id")?, name: row.get("name")?, bio: row.get("bio")? })
        }
    }

    let mgr = django_rs_db::Manager::<NullUser>::new();
    let with = mgr.filter(Q::filter("bio", Lookup::IsNull(false))).execute_query(&db).await.unwrap();
    assert_eq!(with.len(), 1);
    assert_eq!(with[0].name, "A");
    let without = mgr.filter(Q::filter("bio", Lookup::IsNull(true))).execute_query(&db).await.unwrap();
    assert_eq!(without.len(), 1);
    assert_eq!(without[0].name, "B");
}

#[tokio::test]
async fn test_many_creates() {
    let db = setup_product_db().await;
    for i in 0..20 {
        let mut p = Product::new(&format!("Product {i}"), i as f64 * 1.5);
        create_model(&mut p, &db).await.unwrap();
        assert_eq!(p.id, (i + 1) as i64);
    }
    assert_eq!(django_rs_db::Manager::<Product>::new().all().count_exec(&db).await.unwrap(), 20);
}

#[tokio::test]
async fn test_update_nonexistent() {
    let db = setup_user_db().await;
    let a = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("id", Lookup::Exact(Value::from(999))))
        .update(vec![("name", Value::from("Ghost"))])
        .update_exec(&db).await.unwrap();
    assert_eq!(a, 0);
}

#[tokio::test]
async fn test_delete_nonexistent() {
    let db = setup_user_db().await;
    let a = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("id", Lookup::Exact(Value::from(999))))
        .delete().delete_exec(&db).await.unwrap();
    assert_eq!(a, 0);
}

#[tokio::test]
async fn test_create_and_immediate_get() {
    let db = setup_product_db().await;
    let mut p = Product::new("Immediate", 42.0);
    create_model(&mut p, &db).await.unwrap();
    let f = django_rs_db::Manager::<Product>::new()
        .filter(Q::filter("id", Lookup::Exact(Value::from(p.id))))
        .get_exec(&db).await.unwrap();
    assert_eq!(f.name, "Immediate");
}

#[tokio::test]
async fn test_create_update_delete_cycle() {
    let db = setup_product_db().await;
    let mgr = django_rs_db::Manager::<Product>::new();
    let mut p = Product::new("Cycle", 10.0);
    create_model(&mut p, &db).await.unwrap();
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 1);
    p.name = "Updated".to_string();
    save_model(&mut p, &db).await.unwrap();
    let f = mgr.filter(Q::filter("id", Lookup::Exact(Value::from(p.id)))).get_exec(&db).await.unwrap();
    assert_eq!(f.name, "Updated");
    delete_model(&p, &db).await.unwrap();
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 0);
}

// ── Model trait tests ─────────────────────────────────────────────────

#[test]
fn test_model_table_name() {
    assert_eq!(User::table_name(), "auth_user");
    assert_eq!(Product::table_name(), "shop_product");
}

#[test]
fn test_model_app_label() {
    assert_eq!(User::app_label(), "auth");
    assert_eq!(Product::app_label(), "shop");
}

#[test]
fn test_model_pk_field_name() {
    assert_eq!(User::pk_field_name(), "id");
    assert_eq!(Product::pk_field_name(), "id");
}

#[test]
fn test_model_field_values() {
    let u = User { id: 1, name: "T".to_string(), age: 25, email: "t@t.com".to_string() };
    assert_eq!(u.field_values().len(), 4);
}

#[test]
fn test_model_non_pk_field_values() {
    let u = User { id: 1, name: "T".to_string(), age: 25, email: "t@t.com".to_string() };
    let npk = u.non_pk_field_values();
    assert_eq!(npk.len(), 3);
    assert!(npk.iter().all(|(n, _)| *n != "id"));
}

#[test]
fn test_model_from_row() {
    let row = Row::new(
        vec!["id".into(), "name".into(), "age".into(), "email".into()],
        vec![Value::Int(1), Value::String("A".into()), Value::Int(30), Value::String("a@a.com".into())],
    );
    let u = User::from_row(&row).unwrap();
    assert_eq!(u.id, 1);
    assert_eq!(u.name, "A");
}

#[test]
fn test_model_set_pk() {
    let mut u = User { id: 0, name: "T".to_string(), age: 0, email: String::new() };
    u.set_pk(Value::Int(42));
    assert_eq!(u.id, 42);
}

#[test]
fn test_product_pk_some_after_set() {
    let mut p = Product::new("T", 1.0);
    assert!(p.pk().is_none());
    p.set_pk(Value::Int(5));
    assert!(p.pk().is_some());
    assert_eq!(*p.pk().unwrap(), Value::Int(5));
}
