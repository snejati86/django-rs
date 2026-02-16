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
use django_rs_db::query::compiler::{InheritanceType, OrderBy, Row};
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
            constraints: vec![],
            inheritance_type: InheritanceType::None,
        });
        &META
    }

    fn table_name() -> &'static str {
        "auth_user"
    }
    fn app_label() -> &'static str {
        "auth"
    }

    fn pk(&self) -> Option<&Value> {
        if self.id == 0 {
            None
        } else {
            None
        }
    }

    fn set_pk(&mut self, value: Value) {
        if let Value::Int(id) = value {
            self.id = id;
        }
    }

    fn pk_field_name() -> &'static str {
        "id"
    }

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
        Self {
            pk_value: Value::Null,
            id: 0,
            name: name.to_string(),
            price,
        }
    }
}

impl Model for Product {
    fn meta() -> &'static ModelMeta {
        use std::sync::LazyLock;
        static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
            app_label: "shop",
            model_name: "product",
            db_table: "shop_product".to_string(),
            verbose_name: "product".to_string(),
            verbose_name_plural: "products".to_string(),
            ordering: vec![],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("name", FieldType::CharField).max_length(200),
                FieldDef::new("price", FieldType::FloatField),
            ],
            constraints: vec![],
            inheritance_type: InheritanceType::None,
        });
        &META
    }
    fn table_name() -> &'static str {
        "shop_product"
    }
    fn app_label() -> &'static str {
        "shop"
    }
    fn pk(&self) -> Option<&Value> {
        if self.id == 0 {
            None
        } else {
            Some(&self.pk_value)
        }
    }
    fn set_pk(&mut self, value: Value) {
        if let Value::Int(id) = &value {
            self.id = *id;
        }
        self.pk_value = value;
    }
    fn pk_field_name() -> &'static str {
        "id"
    }
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
        Ok(Product {
            pk_value: Value::Int(id),
            id,
            name: row.get("name")?,
            price,
        })
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
            app_label: "test",
            model_name: "hookedproduct",
            db_table: "test_hooked".to_string(),
            verbose_name: "hooked".to_string(),
            verbose_name_plural: "hookeds".to_string(),
            ordering: vec![],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("name", FieldType::CharField).max_length(100),
                FieldDef::new("count", FieldType::IntegerField),
            ],
            constraints: vec![],
            inheritance_type: InheritanceType::None,
        });
        &META
    }
    fn table_name() -> &'static str {
        "test_hooked"
    }
    fn app_label() -> &'static str {
        "test"
    }
    fn pk(&self) -> Option<&Value> {
        if self.id == 0 {
            None
        } else {
            Some(&self.pk_value)
        }
    }
    fn set_pk(&mut self, value: Value) {
        if let Value::Int(id) = &value {
            self.id = *id;
        }
        self.pk_value = value;
    }
    fn pk_field_name() -> &'static str {
        "id"
    }
    fn field_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::Int(self.id)),
            ("name", Value::String(self.name.clone())),
            ("count", Value::Int(self.count)),
        ]
    }
    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        let id: i64 = row.get("id")?;
        Ok(HookedProduct {
            pk_value: Value::Int(id),
            id,
            name: row.get("name")?,
            count: row.get("count")?,
        })
    }
}

impl ModelLifecycleHooks for HookedProduct {
    fn on_pre_save(&self) -> DjangoResult<()> {
        if self.name.is_empty() {
            return Err(DjangoError::DatabaseError(
                "Name cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
    fn on_pre_delete(&self) -> DjangoResult<()> {
        Ok(())
    }
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
        db.execute(
            "INSERT INTO auth_user (name, age, email) VALUES (?, ?, ?)",
            &[
                Value::from(name),
                Value::from(age as i64),
                Value::from(email),
            ],
        )
        .await
        .unwrap();
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
    let users = mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .execute_query(&db)
        .await
        .unwrap();
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
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_qs_execute_filter_gte() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Gte(Value::from(28))))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 3);
}

#[tokio::test]
async fn test_qs_execute_filter_lt() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Lt(Value::from(26))))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_qs_execute_filter_lte() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Lte(Value::from(25))))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_qs_execute_filter_contains() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Contains("li".to_string())))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 2); // Alice, Charlie
}

#[tokio::test]
async fn test_qs_execute_filter_startswith() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::StartsWith("Ch".to_string())))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Charlie");
}

#[tokio::test]
async fn test_qs_execute_filter_endswith() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::EndsWith("e".to_string())))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 3); // Alice, Charlie, Eve
}

#[tokio::test]
async fn test_qs_execute_filter_in() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter(
            "name",
            Lookup::In(vec![Value::from("Alice"), Value::from("Bob")]),
        ))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_qs_execute_filter_range() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(Q::filter(
            "age",
            Lookup::Range(Value::from(25), Value::from(30)),
        ))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 3);
}

#[tokio::test]
async fn test_qs_execute_filter_and() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(
            Q::filter("age", Lookup::Gte(Value::from(25)))
                & Q::filter("age", Lookup::Lte(Value::from(30))),
        )
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 3);
}

#[tokio::test]
async fn test_qs_execute_filter_or() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .filter(
            Q::filter("name", Lookup::Exact(Value::from("Alice")))
                | Q::filter("name", Lookup::Exact(Value::from("Eve"))),
        )
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn test_qs_execute_exclude() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .exclude(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 4);
    assert!(users.iter().all(|u| u.name != "Alice"));
}

#[tokio::test]
async fn test_qs_execute_order_asc() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[4].name, "Eve");
}

#[tokio::test]
async fn test_qs_execute_order_desc() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .all()
        .order_by(vec![OrderBy::desc("age")])
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users[0].name, "Charlie");
    assert_eq!(users[4].name, "Eve");
}

#[tokio::test]
async fn test_qs_execute_limit() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .limit(2)
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[1].name, "Bob");
}

#[tokio::test]
async fn test_qs_execute_offset() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .limit(2)
        .offset(2)
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Charlie");
    assert_eq!(users[1].name, "Diana");
}

#[tokio::test]
async fn test_qs_execute_none() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .none()
        .execute_query(&db)
        .await
        .unwrap();
    assert!(users.is_empty());
}

#[tokio::test]
async fn test_qs_execute_empty_table() {
    let db = setup_user_db().await;
    let users = django_rs_db::Manager::<User>::new()
        .all()
        .execute_query(&db)
        .await
        .unwrap();
    assert!(users.is_empty());
}

#[tokio::test]
async fn test_qs_execute_chained_filters() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .all()
        .filter(Q::filter("age", Lookup::Gte(Value::from(25))))
        .filter(Q::filter("age", Lookup::Lte(Value::from(30))))
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 3);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[1].name, "Bob");
    assert_eq!(users[2].name, "Diana");
}

#[tokio::test]
async fn test_qs_execute_reverse() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .reverse()
        .limit(2)
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users[0].name, "Eve");
    assert_eq!(users[1].name, "Diana");
}

// ── count_exec ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_count_all() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    assert_eq!(
        django_rs_db::Manager::<User>::new()
            .all()
            .count_exec(&db)
            .await
            .unwrap(),
        5
    );
}

#[tokio::test]
async fn test_qs_count_filtered() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let c = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Gt(Value::from(28))))
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(c, 2);
}

#[tokio::test]
async fn test_qs_count_none() {
    let db = setup_user_db().await;
    assert_eq!(
        django_rs_db::Manager::<User>::new()
            .none()
            .count_exec(&db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn test_qs_count_empty() {
    let db = setup_user_db().await;
    assert_eq!(
        django_rs_db::Manager::<User>::new()
            .all()
            .count_exec(&db)
            .await
            .unwrap(),
        0
    );
}

// ── exists_exec ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_exists_true() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    assert!(django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .exists_exec(&db)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_qs_exists_false() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    assert!(!django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Nobody"))))
        .exists_exec(&db)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_qs_exists_none() {
    let db = setup_user_db().await;
    assert!(!django_rs_db::Manager::<User>::new()
        .none()
        .exists_exec(&db)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_qs_exists_empty() {
    let db = setup_user_db().await;
    assert!(!django_rs_db::Manager::<User>::new()
        .all()
        .exists_exec(&db)
        .await
        .unwrap());
}

// ── first_exec ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_first_some() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let u = django_rs_db::Manager::<User>::new()
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .first_exec(&db)
        .await
        .unwrap();
    assert_eq!(u.unwrap().name, "Alice");
}

#[tokio::test]
async fn test_qs_first_none() {
    let db = setup_user_db().await;
    assert!(django_rs_db::Manager::<User>::new()
        .all()
        .first_exec(&db)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_qs_first_with_filter() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let u = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Gt(Value::from(30))))
        .first_exec(&db)
        .await
        .unwrap();
    assert_eq!(u.unwrap().name, "Charlie");
}

#[tokio::test]
async fn test_qs_first_none_qs() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    assert!(django_rs_db::Manager::<User>::new()
        .none()
        .first_exec(&db)
        .await
        .unwrap()
        .is_none());
}

// ── get_exec ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_get_found() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let u = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .get_exec(&db)
        .await
        .unwrap();
    assert_eq!(u.name, "Alice");
    assert_eq!(u.age, 30);
}

#[tokio::test]
async fn test_qs_get_not_found() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let r = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Nobody"))))
        .get_exec(&db)
        .await;
    assert!(matches!(r, Err(DjangoError::DoesNotExist(_))));
}

#[tokio::test]
async fn test_qs_get_multiple() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let r = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Gt(Value::from(24))))
        .get_exec(&db)
        .await;
    assert!(matches!(r, Err(DjangoError::MultipleObjectsReturned(_))));
}

#[tokio::test]
async fn test_qs_get_none_qs() {
    let db = setup_user_db().await;
    let r = django_rs_db::Manager::<User>::new()
        .none()
        .get_exec(&db)
        .await;
    assert!(matches!(r, Err(DjangoError::DoesNotExist(_))));
}

// ── update_exec ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_update_single() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let a = mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .update(vec![("age", Value::from(31))])
        .update_exec(&db)
        .await
        .unwrap();
    assert_eq!(a, 1);
    let u = mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .get_exec(&db)
        .await
        .unwrap();
    assert_eq!(u.age, 31);
}

#[tokio::test]
async fn test_qs_update_multiple() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let a = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("age", Lookup::Lt(Value::from(30))))
        .update(vec![("email", Value::from("updated@test.com"))])
        .update_exec(&db)
        .await
        .unwrap();
    assert_eq!(a, 3);
}

#[tokio::test]
async fn test_qs_update_no_match() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let a = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Nobody"))))
        .update(vec![("age", Value::from(99))])
        .update_exec(&db)
        .await
        .unwrap();
    assert_eq!(a, 0);
}

#[tokio::test]
async fn test_qs_update_none_qs() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let a = django_rs_db::Manager::<User>::new()
        .none()
        .update(vec![("age", Value::from(99))])
        .update_exec(&db)
        .await
        .unwrap();
    assert_eq!(a, 0);
}

#[tokio::test]
async fn test_qs_update_no_pending() {
    let db = setup_user_db().await;
    assert!(django_rs_db::Manager::<User>::new()
        .all()
        .update_exec(&db)
        .await
        .is_err());
}

// ── delete_exec ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_delete_single() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let a = mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .delete()
        .delete_exec(&db)
        .await
        .unwrap();
    assert_eq!(a, 1);
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 4);
}

#[tokio::test]
async fn test_qs_delete_multiple() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let a = mgr
        .filter(Q::filter("age", Lookup::Lt(Value::from(30))))
        .delete()
        .delete_exec(&db)
        .await
        .unwrap();
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
    assert_eq!(
        django_rs_db::Manager::<User>::new()
            .none()
            .delete()
            .delete_exec(&db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn test_qs_delete_no_pending() {
    let db = setup_user_db().await;
    assert!(django_rs_db::Manager::<User>::new()
        .all()
        .delete_exec(&db)
        .await
        .is_err());
}

// ── create_exec ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_qs_create_exec() {
    let db = setup_user_db().await;
    let mgr = django_rs_db::Manager::<User>::new();
    let pk = mgr
        .create(vec![
            ("name", Value::from("TestUser")),
            ("age", Value::from(42)),
            ("email", Value::from("test@test.com")),
        ])
        .create_exec(&db)
        .await
        .unwrap();
    assert_eq!(pk, Value::Int(1));
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 1);
}

#[tokio::test]
async fn test_qs_create_exec_no_pending() {
    let db = setup_user_db().await;
    assert!(django_rs_db::Manager::<User>::new()
        .all()
        .create_exec(&db)
        .await
        .is_err());
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
        .get_exec(&db)
        .await
        .unwrap();
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
        .get_exec(&db)
        .await
        .unwrap();
    assert!((fetched.price - 12.99).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_delete_model_basic() {
    let db = setup_product_db().await;
    let mut p = Product::new("ToDelete", 1.0);
    create_model(&mut p, &db).await.unwrap();
    let a = delete_model(&p, &db).await.unwrap();
    assert_eq!(a, 1);
    assert_eq!(
        django_rs_db::Manager::<Product>::new()
            .all()
            .count_exec(&db)
            .await
            .unwrap(),
        0
    );
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
    db.execute(
        "UPDATE shop_product SET price = 7.99 WHERE id = ?",
        &[Value::from(p.id)],
    )
    .await
    .unwrap();
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

    let all = mgr
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(all[0].name, "Keyboard");
    assert_eq!(all[1].name, "Laptop");
    assert_eq!(all[2].name, "Mouse");

    p2.price = 19.99;
    save_model(&mut p2, &db).await.unwrap();
    let updated = mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Mouse"))))
        .get_exec(&db)
        .await
        .unwrap();
    assert!((updated.price - 19.99).abs() < f64::EPSILON);

    delete_model(&p1, &db).await.unwrap();
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 2);

    let remaining = mgr
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db)
        .await
        .unwrap();
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
    let mut m = HookedProduct {
        pk_value: Value::Null,
        id: 0,
        name: String::new(),
        count: 0,
    };
    assert!(django_rs_db::executor::create_model_with_hooks(&mut m, &db)
        .await
        .is_err());
}

#[tokio::test]
async fn test_hooks_pre_save_allows_valid() {
    let db = setup_hooked_db().await;
    let mut m = HookedProduct {
        pk_value: Value::Null,
        id: 0,
        name: "Valid".to_string(),
        count: 0,
    };
    django_rs_db::executor::create_model_with_hooks(&mut m, &db)
        .await
        .unwrap();
    assert!(m.id > 0);
}

#[tokio::test]
async fn test_hooks_save_with_hooks() {
    let db = setup_hooked_db().await;
    let mut m = HookedProduct {
        pk_value: Value::Null,
        id: 0,
        name: "Test".to_string(),
        count: 0,
    };
    django_rs_db::executor::save_model_with_hooks(&mut m, &db)
        .await
        .unwrap();
    assert!(m.id > 0);
}

#[tokio::test]
async fn test_hooks_delete_with_hooks() {
    let db = setup_hooked_db().await;
    let mut m = HookedProduct {
        pk_value: Value::Null,
        id: 0,
        name: "ToDelete".to_string(),
        count: 0,
    };
    create_model(&mut m, &db).await.unwrap();
    let a = django_rs_db::executor::delete_model_with_hooks(&m, &db)
        .await
        .unwrap();
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
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT)",
        &[],
    )
    .await
    .unwrap();
    let ex: &dyn DbExecutor = &db;
    assert_eq!(
        ex.insert_returning_id("INSERT INTO t (val) VALUES (?)", &[Value::from("a")])
            .await
            .unwrap(),
        Value::Int(1)
    );
    assert_eq!(
        ex.insert_returning_id("INSERT INTO t (val) VALUES (?)", &[Value::from("b")])
            .await
            .unwrap(),
        Value::Int(2)
    );
}

#[tokio::test]
async fn test_executor_execute_sql() {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT)",
        &[],
    )
    .await
    .unwrap();
    let ex: &dyn DbExecutor = &db;
    assert_eq!(
        ex.execute_sql("INSERT INTO t (val) VALUES (?)", &[Value::from("x")])
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn test_executor_query() {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT)",
        &[],
    )
    .await
    .unwrap();
    db.execute("INSERT INTO t (val) VALUES (?)", &[Value::from("a")])
        .await
        .unwrap();
    db.execute("INSERT INTO t (val) VALUES (?)", &[Value::from("b")])
        .await
        .unwrap();
    let ex: &dyn DbExecutor = &db;
    let rows = ex.query("SELECT * FROM t ORDER BY id", &[]).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<String>("val").unwrap(), "a");
}

#[tokio::test]
async fn test_executor_query_one_found() {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT)",
        &[],
    )
    .await
    .unwrap();
    db.execute("INSERT INTO t (val) VALUES (?)", &[Value::from("only")])
        .await
        .unwrap();
    let ex: &dyn DbExecutor = &db;
    let row = ex
        .query_one("SELECT val FROM t WHERE id = ?", &[Value::from(1)])
        .await
        .unwrap();
    assert_eq!(row.get::<String>("val").unwrap(), "only");
}

#[tokio::test]
async fn test_executor_query_one_not_found() {
    let db = SqliteBackend::memory().unwrap();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT)", &[])
        .await
        .unwrap();
    let ex: &dyn DbExecutor = &db;
    assert!(matches!(
        ex.query_one("SELECT id FROM t WHERE id = ?", &[Value::from(999)])
            .await,
        Err(DjangoError::DoesNotExist(_))
    ));
}

// ═══════════════════════════════════════════════════════════════════════
// COMPLEX QUERY TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_complex_filter_chain() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let users = django_rs_db::Manager::<User>::new()
        .all()
        .filter(
            Q::filter("name", Lookup::Contains("li".to_string()))
                | Q::filter("name", Lookup::Contains("ob".to_string())),
        )
        .filter(Q::filter("age", Lookup::Gte(Value::from(25))))
        .exclude(Q::filter("age", Lookup::Gt(Value::from(32))))
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[1].name, "Bob");
}

#[tokio::test]
async fn test_pagination() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();
    let p1 = mgr
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .limit(2)
        .offset(0)
        .execute_query(&db)
        .await
        .unwrap();
    let p2 = mgr
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .limit(2)
        .offset(2)
        .execute_query(&db)
        .await
        .unwrap();
    let p3 = mgr
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .limit(2)
        .offset(4)
        .execute_query(&db)
        .await
        .unwrap();
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
    db.execute(
        "CREATE TABLE nt (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, bio TEXT)",
        &[],
    )
    .await
    .unwrap();
    db.execute(
        "INSERT INTO nt (name, bio) VALUES (?, ?)",
        &[Value::from("A"), Value::from("Has bio")],
    )
    .await
    .unwrap();
    db.execute(
        "INSERT INTO nt (name, bio) VALUES (?, ?)",
        &[Value::from("B"), Value::Null],
    )
    .await
    .unwrap();

    #[derive(Debug)]
    struct NullUser {
        id: i64,
        name: String,
        bio: Option<String>,
    }
    impl Model for NullUser {
        fn meta() -> &'static ModelMeta {
            use std::sync::LazyLock;
            static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
                app_label: "test",
                model_name: "nulluser",
                db_table: "nt".to_string(),
                verbose_name: "nu".to_string(),
                verbose_name_plural: "nus".to_string(),
                ordering: vec![],
                unique_together: vec![],
                indexes: vec![],
                abstract_model: false,
                fields: vec![],
                constraints: vec![],
                inheritance_type: InheritanceType::None,
            });
            &META
        }
        fn table_name() -> &'static str {
            "nt"
        }
        fn app_label() -> &'static str {
            "test"
        }
        fn pk(&self) -> Option<&Value> {
            None
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
                ("bio", Value::from(self.bio.clone())),
            ]
        }
        fn from_row(row: &Row) -> Result<Self, DjangoError> {
            Ok(NullUser {
                id: row.get("id")?,
                name: row.get("name")?,
                bio: row.get("bio")?,
            })
        }
    }

    let mgr = django_rs_db::Manager::<NullUser>::new();
    let with = mgr
        .filter(Q::filter("bio", Lookup::IsNull(false)))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(with.len(), 1);
    assert_eq!(with[0].name, "A");
    let without = mgr
        .filter(Q::filter("bio", Lookup::IsNull(true)))
        .execute_query(&db)
        .await
        .unwrap();
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
    assert_eq!(
        django_rs_db::Manager::<Product>::new()
            .all()
            .count_exec(&db)
            .await
            .unwrap(),
        20
    );
}

#[tokio::test]
async fn test_update_nonexistent() {
    let db = setup_user_db().await;
    let a = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("id", Lookup::Exact(Value::from(999))))
        .update(vec![("name", Value::from("Ghost"))])
        .update_exec(&db)
        .await
        .unwrap();
    assert_eq!(a, 0);
}

#[tokio::test]
async fn test_delete_nonexistent() {
    let db = setup_user_db().await;
    let a = django_rs_db::Manager::<User>::new()
        .filter(Q::filter("id", Lookup::Exact(Value::from(999))))
        .delete()
        .delete_exec(&db)
        .await
        .unwrap();
    assert_eq!(a, 0);
}

#[tokio::test]
async fn test_create_and_immediate_get() {
    let db = setup_product_db().await;
    let mut p = Product::new("Immediate", 42.0);
    create_model(&mut p, &db).await.unwrap();
    let f = django_rs_db::Manager::<Product>::new()
        .filter(Q::filter("id", Lookup::Exact(Value::from(p.id))))
        .get_exec(&db)
        .await
        .unwrap();
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
    let f = mgr
        .filter(Q::filter("id", Lookup::Exact(Value::from(p.id))))
        .get_exec(&db)
        .await
        .unwrap();
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
    let u = User {
        id: 1,
        name: "T".to_string(),
        age: 25,
        email: "t@t.com".to_string(),
    };
    assert_eq!(u.field_values().len(), 4);
}

#[test]
fn test_model_non_pk_field_values() {
    let u = User {
        id: 1,
        name: "T".to_string(),
        age: 25,
        email: "t@t.com".to_string(),
    };
    let npk = u.non_pk_field_values();
    assert_eq!(npk.len(), 3);
    assert!(npk.iter().all(|(n, _)| *n != "id"));
}

#[test]
fn test_model_from_row() {
    let row = Row::new(
        vec!["id".into(), "name".into(), "age".into(), "email".into()],
        vec![
            Value::Int(1),
            Value::String("A".into()),
            Value::Int(30),
            Value::String("a@a.com".into()),
        ],
    );
    let u = User::from_row(&row).unwrap();
    assert_eq!(u.id, 1);
    assert_eq!(u.name, "A");
}

#[test]
fn test_model_set_pk() {
    let mut u = User {
        id: 0,
        name: "T".to_string(),
        age: 0,
        email: String::new(),
    };
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

// ═══════════════════════════════════════════════════════════════════════
// UNION / INTERSECT / EXCEPT INTEGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_union_basic() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();

    // Young users (< 26): Bob(25), Eve(22)
    let young = mgr.filter(Q::filter("age", Lookup::Lt(Value::from(26))));
    // Old users (> 30): Charlie(35)
    let old = mgr.filter(Q::filter("age", Lookup::Gt(Value::from(30))));

    let combined = young.union(old);
    let (sql, _params) = combined.to_sql(django_rs_db::DatabaseBackendType::SQLite);
    assert!(sql.contains("UNION"), "SQL should contain UNION: {sql}");

    let results = combined.execute_query(&db).await.unwrap();
    // Should have Bob, Eve, and Charlie = 3
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_union_all_keeps_duplicates() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();

    // All users
    let all1 = mgr.all();
    // Users with age >= 25 (Alice, Bob, Charlie, Diana) = 4
    let some = mgr.filter(Q::filter("age", Lookup::Gte(Value::from(25))));

    let combined = all1.union_all(some);
    let (sql, _params) = combined.to_sql(django_rs_db::DatabaseBackendType::SQLite);
    assert!(
        sql.contains("UNION ALL"),
        "SQL should contain UNION ALL: {sql}"
    );

    let results = combined.execute_query(&db).await.unwrap();
    // 5 + 4 = 9 (duplicates preserved)
    assert_eq!(results.len(), 9);
}

#[tokio::test]
async fn test_union_deduplicates() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();

    // All users
    let all1 = mgr.all();
    // Same all users
    let all2 = mgr.all();

    let combined = all1.union(all2);
    let results = combined.execute_query(&db).await.unwrap();
    // UNION deduplicates: still 5
    assert_eq!(results.len(), 5);
}

#[tokio::test]
async fn test_intersect_basic() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();

    // age >= 25: Alice(30), Bob(25), Charlie(35), Diana(28) = 4
    let older = mgr.filter(Q::filter("age", Lookup::Gte(Value::from(25))));
    // age <= 30: Alice(30), Bob(25), Diana(28), Eve(22) = 4
    let younger = mgr.filter(Q::filter("age", Lookup::Lte(Value::from(30))));

    let combined = older.intersection(younger);
    let (sql, _params) = combined.to_sql(django_rs_db::DatabaseBackendType::SQLite);
    assert!(
        sql.contains("INTERSECT"),
        "SQL should contain INTERSECT: {sql}"
    );

    let results = combined.execute_query(&db).await.unwrap();
    // Intersection: Alice(30), Bob(25), Diana(28) = 3
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_except_basic() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();

    // All users = 5
    let all = mgr.all();
    // age < 26: Bob(25), Eve(22) = 2
    let young = mgr.filter(Q::filter("age", Lookup::Lt(Value::from(26))));

    let combined = all.difference(young);
    let (sql, _params) = combined.to_sql(django_rs_db::DatabaseBackendType::SQLite);
    assert!(sql.contains("EXCEPT"), "SQL should contain EXCEPT: {sql}");

    let results = combined.execute_query(&db).await.unwrap();
    // All minus young: Alice(30), Charlie(35), Diana(28) = 3
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_union_with_limit() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();

    let young = mgr.filter(Q::filter("age", Lookup::Lt(Value::from(26))));
    let old = mgr.filter(Q::filter("age", Lookup::Gt(Value::from(30))));

    let combined = young
        .union(old)
        .order_by(vec![OrderBy::asc("name")])
        .limit(2);
    let results = combined.execute_query(&db).await.unwrap();
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_union_count() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();

    let young = mgr.filter(Q::filter("age", Lookup::Lt(Value::from(26))));
    let old = mgr.filter(Q::filter("age", Lookup::Gt(Value::from(30))));

    // The union itself should produce 3 results
    let combined = young.union(old);
    let results = combined.execute_query(&db).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_union_chained() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();

    // Chain multiple unions
    let q1 = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))));
    let q2 = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Bob"))));
    let q3 = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Charlie"))));

    let combined = q1.union(q2).union(q3);
    let results = combined.execute_query(&db).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_union_empty_result() {
    let db = setup_user_db().await;
    seed_users(&db).await;
    let mgr = django_rs_db::Manager::<User>::new();

    let nobody1 = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Nobody1"))));
    let nobody2 = mgr.filter(Q::filter("name", Lookup::Exact(Value::from("Nobody2"))));

    let combined = nobody1.union(nobody2);
    let results = combined.execute_query(&db).await.unwrap();
    assert!(results.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════
// SELECT_RELATED INTEGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════

// Post model with a foreign key to auth_user
#[derive(Debug, Clone)]
struct Post {
    id: i64,
    title: String,
    author_id: i64,
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
            ordering: vec![],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("title", FieldType::CharField).max_length(200),
                FieldDef::new("author_id", FieldType::BigIntegerField),
            ],
            constraints: vec![],
            inheritance_type: InheritanceType::None,
        });
        &META
    }
    fn table_name() -> &'static str {
        "blog_post"
    }
    fn app_label() -> &'static str {
        "blog"
    }
    fn pk(&self) -> Option<&Value> {
        if self.id == 0 {
            None
        } else {
            None
        }
    }
    fn set_pk(&mut self, value: Value) {
        if let Value::Int(id) = value {
            self.id = id;
        }
    }
    fn pk_field_name() -> &'static str {
        "id"
    }
    fn field_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::Int(self.id)),
            ("title", Value::String(self.title.clone())),
            ("author_id", Value::Int(self.author_id)),
        ]
    }
    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        Ok(Post {
            id: row.get("id")?,
            title: row.get("title")?,
            author_id: row.get("author_id")?,
        })
    }
}

async fn setup_post_db() -> SqliteBackend {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE auth_user (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, age INTEGER NOT NULL, email TEXT NOT NULL)",
        &[],
    ).await.unwrap();
    db.execute(
        "CREATE TABLE blog_post (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, author_id INTEGER NOT NULL REFERENCES auth_user(id))",
        &[],
    ).await.unwrap();
    db
}

async fn seed_posts(db: &SqliteBackend) {
    // Insert users
    for (name, age, email) in [
        ("Alice", 30, "alice@example.com"),
        ("Bob", 25, "bob@example.com"),
    ] {
        db.execute(
            "INSERT INTO auth_user (name, age, email) VALUES (?, ?, ?)",
            &[
                Value::from(name),
                Value::from(age as i64),
                Value::from(email),
            ],
        )
        .await
        .unwrap();
    }
    // Insert posts
    for (title, author_id) in [("First Post", 1), ("Second Post", 1), ("Third Post", 2)] {
        db.execute(
            "INSERT INTO blog_post (title, author_id) VALUES (?, ?)",
            &[Value::from(title), Value::from(author_id as i64)],
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn test_select_related_join_query() {
    let db = setup_post_db().await;
    seed_posts(&db).await;

    use django_rs_db::query::compiler::SelectRelatedField;

    let mgr = django_rs_db::Manager::<Post>::new();
    let qs = mgr.all().select_related_with(vec![SelectRelatedField {
        field_name: "author".to_string(),
        related_table: "auth_user".to_string(),
        fk_column: "author_id".to_string(),
        related_column: "id".to_string(),
        alias: "author".to_string(),
    }]);

    let (sql, _params) = qs.to_sql(django_rs_db::DatabaseBackendType::SQLite);
    // Should produce a LEFT JOIN (or LEFT OUTER JOIN)
    assert!(
        sql.contains("LEFT JOIN"),
        "SQL should contain LEFT JOIN: {sql}"
    );
    assert!(
        sql.contains("auth_user"),
        "SQL should reference auth_user: {sql}"
    );

    // Execute and verify we get all 3 posts
    let results = qs.execute_query(&db).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_select_related_with_filter() {
    let db = setup_post_db().await;
    seed_posts(&db).await;

    use django_rs_db::query::compiler::SelectRelatedField;

    let mgr = django_rs_db::Manager::<Post>::new();
    let qs = mgr
        .filter(Q::filter("author_id", Lookup::Exact(Value::from(1i64))))
        .select_related_with(vec![SelectRelatedField {
            field_name: "author".to_string(),
            related_table: "auth_user".to_string(),
            fk_column: "author_id".to_string(),
            related_column: "id".to_string(),
            alias: "author".to_string(),
        }]);

    let results = qs.execute_query(&db).await.unwrap();
    // Alice has 2 posts
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|p| p.author_id == 1));
}

// ═══════════════════════════════════════════════════════════════════════
// PREFETCH_RELATED INTEGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_prefetch_related_batch_query() {
    let db = setup_post_db().await;
    seed_posts(&db).await;

    use django_rs_db::query::compiler::PrefetchRelatedField;

    let mgr = django_rs_db::Manager::<User>::new();
    let qs = mgr.all().prefetch_related_with(vec![PrefetchRelatedField {
        field_name: "posts".to_string(),
        related_table: "blog_post".to_string(),
        source_column: "id".to_string(),
        related_column: "author_id".to_string(),
    }]);

    // Compile and verify the prefetch SQL is generated
    let compiler = django_rs_db::SqlCompiler::new(django_rs_db::DatabaseBackendType::SQLite);
    let pk_values = vec![Value::Int(1), Value::Int(2)];
    let prefetch_queries =
        compiler.compile_prefetch_queries(&qs.query().prefetch_related, &pk_values);
    assert_eq!(prefetch_queries.len(), 1);
    let (field_name, sql, params) = &prefetch_queries[0];
    assert_eq!(field_name, "posts");
    assert!(
        sql.contains("blog_post"),
        "Prefetch SQL should reference blog_post: {sql}"
    );
    assert!(sql.contains("IN"), "Prefetch SQL should contain IN: {sql}");
    assert_eq!(params.len(), 2);
}

#[tokio::test]
async fn test_prefetch_related_empty_result() {
    use django_rs_db::query::compiler::PrefetchRelatedField;

    let compiler = django_rs_db::SqlCompiler::new(django_rs_db::DatabaseBackendType::SQLite);
    let prefetch_fields = vec![PrefetchRelatedField {
        field_name: "posts".to_string(),
        related_table: "blog_post".to_string(),
        source_column: "id".to_string(),
        related_column: "author_id".to_string(),
    }];

    // With empty PK values, no queries should be generated
    let queries = compiler.compile_prefetch_queries(&prefetch_fields, &[]);
    assert!(queries.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════
// MODEL INHERITANCE INTEGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════

// Multi-table inheritance: Restaurant extends User (place)
// Parent table: auth_user (id, name, age, email)
// Child table: restaurant (id, user_id FK, cuisine)
#[derive(Debug, Clone)]
struct Restaurant {
    id: i64,
    user_id: i64,
    name: String,
    age: i64,
    email: String,
    cuisine: String,
}

impl Model for Restaurant {
    fn meta() -> &'static ModelMeta {
        use std::sync::LazyLock;
        static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
            app_label: "dining",
            model_name: "restaurant",
            db_table: "dining_restaurant".to_string(),
            verbose_name: "restaurant".to_string(),
            verbose_name_plural: "restaurants".to_string(),
            ordering: vec![],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("user_id", FieldType::BigIntegerField),
                FieldDef::new("cuisine", FieldType::CharField).max_length(100),
            ],
            constraints: vec![],
            inheritance_type: InheritanceType::MultiTable {
                parent_table: "auth_user".to_string(),
                parent_link_column: "user_id".to_string(),
                parent_pk_column: "id".to_string(),
            },
        });
        &META
    }

    fn table_name() -> &'static str {
        "dining_restaurant"
    }
    fn app_label() -> &'static str {
        "dining"
    }

    fn pk(&self) -> Option<&Value> {
        if self.id == 0 {
            None
        } else {
            None
        }
    }
    fn set_pk(&mut self, value: Value) {
        if let Value::Int(id) = value {
            self.id = id;
        }
    }
    fn pk_field_name() -> &'static str {
        "id"
    }

    fn field_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::Int(self.id)),
            ("user_id", Value::Int(self.user_id)),
            ("cuisine", Value::String(self.cuisine.clone())),
        ]
    }

    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        Ok(Restaurant {
            id: row.get("id")?,
            user_id: row.get("user_id")?,
            name: row.get::<String>("name").unwrap_or_default(),
            age: row.get::<i64>("age").unwrap_or(0),
            email: row.get::<String>("email").unwrap_or_default(),
            cuisine: row.get("cuisine")?,
        })
    }

    fn inheritance_type() -> InheritanceType {
        InheritanceType::MultiTable {
            parent_table: "auth_user".to_string(),
            parent_link_column: "user_id".to_string(),
            parent_pk_column: "id".to_string(),
        }
    }

    fn parent_field_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::String(self.name.clone())),
            ("age", Value::Int(self.age)),
            ("email", Value::String(self.email.clone())),
        ]
    }

    fn child_field_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("user_id", Value::Int(self.user_id)),
            ("cuisine", Value::String(self.cuisine.clone())),
        ]
    }
}

// Proxy model: ProxyUser uses auth_user table but different behavior
#[derive(Debug, Clone)]
struct ProxyUser {
    id: i64,
    name: String,
    age: i64,
    email: String,
}

impl Model for ProxyUser {
    fn meta() -> &'static ModelMeta {
        use std::sync::LazyLock;
        static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
            app_label: "auth",
            model_name: "proxyuser",
            db_table: "auth_proxyuser".to_string(),
            verbose_name: "proxy user".to_string(),
            verbose_name_plural: "proxy users".to_string(),
            ordering: vec![OrderBy::desc("age")],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("name", FieldType::CharField).max_length(100),
                FieldDef::new("age", FieldType::IntegerField),
                FieldDef::new("email", FieldType::CharField).max_length(200),
            ],
            constraints: vec![],
            inheritance_type: InheritanceType::Proxy {
                parent_table: "auth_user".to_string(),
            },
        });
        &META
    }

    fn table_name() -> &'static str {
        "auth_proxyuser"
    }
    fn app_label() -> &'static str {
        "auth"
    }

    fn pk(&self) -> Option<&Value> {
        if self.id == 0 {
            None
        } else {
            None
        }
    }
    fn set_pk(&mut self, value: Value) {
        if let Value::Int(id) = value {
            self.id = id;
        }
    }
    fn pk_field_name() -> &'static str {
        "id"
    }

    fn field_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::Int(self.id)),
            ("name", Value::String(self.name.clone())),
            ("age", Value::Int(self.age)),
            ("email", Value::String(self.email.clone())),
        ]
    }

    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        Ok(ProxyUser {
            id: row.get("id")?,
            name: row.get("name")?,
            age: row.get("age")?,
            email: row.get("email")?,
        })
    }

    fn inheritance_type() -> InheritanceType {
        InheritanceType::Proxy {
            parent_table: "auth_user".to_string(),
        }
    }
}

async fn setup_inheritance_db() -> SqliteBackend {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE auth_user (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, age INTEGER NOT NULL, email TEXT NOT NULL)",
        &[],
    ).await.unwrap();
    db.execute(
        "CREATE TABLE dining_restaurant (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL REFERENCES auth_user(id), cuisine TEXT NOT NULL)",
        &[],
    ).await.unwrap();
    db
}

async fn seed_inheritance(db: &SqliteBackend) {
    // Insert parent records
    for (name, age, email) in [
        ("Alice Restaurant", 30, "alice@food.com"),
        ("Bob Cafe", 25, "bob@food.com"),
        ("Charlie Bar", 35, "charlie@food.com"),
    ] {
        db.execute(
            "INSERT INTO auth_user (name, age, email) VALUES (?, ?, ?)",
            &[
                Value::from(name),
                Value::from(age as i64),
                Value::from(email),
            ],
        )
        .await
        .unwrap();
    }
    // Insert child records
    for (user_id, cuisine) in [(1, "Italian"), (2, "Japanese"), (3, "Mexican")] {
        db.execute(
            "INSERT INTO dining_restaurant (user_id, cuisine) VALUES (?, ?)",
            &[Value::from(user_id as i64), Value::from(cuisine)],
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn test_multi_table_inheritance_query() {
    let db = setup_inheritance_db().await;
    seed_inheritance(&db).await;

    let mgr = django_rs_db::Manager::<Restaurant>::new();
    let qs = mgr.all().set_inheritance(InheritanceType::MultiTable {
        parent_table: "auth_user".to_string(),
        parent_link_column: "user_id".to_string(),
        parent_pk_column: "id".to_string(),
    });

    let (sql, _params) = qs.to_sql(django_rs_db::DatabaseBackendType::SQLite);
    // Multi-table inheritance produces an INNER JOIN
    assert!(
        sql.contains("INNER JOIN"),
        "SQL should contain INNER JOIN: {sql}"
    );
    assert!(
        sql.contains("auth_user"),
        "SQL should reference parent table: {sql}"
    );

    let results = qs.execute_query(&db).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_multi_table_inheritance_filtered() {
    let db = setup_inheritance_db().await;
    seed_inheritance(&db).await;

    let mgr = django_rs_db::Manager::<Restaurant>::new();
    let qs = mgr
        .filter(Q::filter("cuisine", Lookup::Exact(Value::from("Italian"))))
        .set_inheritance(InheritanceType::MultiTable {
            parent_table: "auth_user".to_string(),
            parent_link_column: "user_id".to_string(),
            parent_pk_column: "id".to_string(),
        });

    let results = qs.execute_query(&db).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].cuisine, "Italian");
}

#[tokio::test]
async fn test_multi_table_inheritance_count() {
    let db = setup_inheritance_db().await;
    seed_inheritance(&db).await;

    let mgr = django_rs_db::Manager::<Restaurant>::new();
    let qs = mgr.all().set_inheritance(InheritanceType::MultiTable {
        parent_table: "auth_user".to_string(),
        parent_link_column: "user_id".to_string(),
        parent_pk_column: "id".to_string(),
    });

    // Count through count_exec
    let count = qs.count_exec(&db).await.unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn test_proxy_model_reads_parent_table() {
    let db = setup_user_db().await;
    seed_users(&db).await;

    // ProxyUser should query auth_user table directly
    let mgr = django_rs_db::Manager::<ProxyUser>::new();
    let qs = mgr.all().set_inheritance(InheritanceType::Proxy {
        parent_table: "auth_user".to_string(),
    });

    let (sql, _params) = qs.to_sql(django_rs_db::DatabaseBackendType::SQLite);
    // Proxy model rewrites the table name to the parent table
    assert!(
        sql.contains("auth_user"),
        "SQL should reference parent table auth_user: {sql}"
    );

    let results = qs.execute_query(&db).await.unwrap();
    assert_eq!(results.len(), 5);
}

#[tokio::test]
async fn test_proxy_model_with_filter() {
    let db = setup_user_db().await;
    seed_users(&db).await;

    let mgr = django_rs_db::Manager::<ProxyUser>::new();
    let qs = mgr
        .filter(Q::filter("age", Lookup::Gte(Value::from(30))))
        .set_inheritance(InheritanceType::Proxy {
            parent_table: "auth_user".to_string(),
        });

    let results = qs.execute_query(&db).await.unwrap();
    // Alice(30), Charlie(35) = 2
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_proxy_model_count() {
    let db = setup_user_db().await;
    seed_users(&db).await;

    let mgr = django_rs_db::Manager::<ProxyUser>::new();
    let qs = mgr.all().set_inheritance(InheritanceType::Proxy {
        parent_table: "auth_user".to_string(),
    });

    let count = qs.count_exec(&db).await.unwrap();
    assert_eq!(count, 5);
}

#[tokio::test]
async fn test_proxy_model_exists() {
    let db = setup_user_db().await;
    seed_users(&db).await;

    let mgr = django_rs_db::Manager::<ProxyUser>::new();
    let qs = mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .set_inheritance(InheritanceType::Proxy {
            parent_table: "auth_user".to_string(),
        });

    assert!(qs.exists_exec(&db).await.unwrap());
}

// ── Model trait inheritance method tests ─────────────────────────────

#[test]
fn test_default_inheritance_type_is_none() {
    assert_eq!(User::inheritance_type(), InheritanceType::None);
    assert_eq!(Product::inheritance_type(), InheritanceType::None);
}

#[test]
fn test_restaurant_inheritance_type() {
    assert!(matches!(
        Restaurant::inheritance_type(),
        InheritanceType::MultiTable { .. }
    ));
}

#[test]
fn test_proxy_user_inheritance_type() {
    assert!(matches!(
        ProxyUser::inheritance_type(),
        InheritanceType::Proxy { .. }
    ));
}

#[test]
fn test_default_parent_field_values_empty() {
    let u = User {
        id: 1,
        name: "T".to_string(),
        age: 25,
        email: "t@t.com".to_string(),
    };
    assert!(u.parent_field_values().is_empty());
}

#[test]
fn test_restaurant_parent_field_values() {
    let r = Restaurant {
        id: 1,
        user_id: 1,
        name: "Test".to_string(),
        age: 30,
        email: "test@test.com".to_string(),
        cuisine: "Italian".to_string(),
    };
    let parent_fields = r.parent_field_values();
    assert_eq!(parent_fields.len(), 3);
    assert!(parent_fields.iter().any(|(name, _)| *name == "name"));
    assert!(parent_fields.iter().any(|(name, _)| *name == "age"));
    assert!(parent_fields.iter().any(|(name, _)| *name == "email"));
}

#[test]
fn test_restaurant_child_field_values() {
    let r = Restaurant {
        id: 1,
        user_id: 1,
        name: "Test".to_string(),
        age: 30,
        email: "test@test.com".to_string(),
        cuisine: "Italian".to_string(),
    };
    let child_fields = r.child_field_values();
    assert_eq!(child_fields.len(), 2);
    assert!(child_fields.iter().any(|(name, _)| *name == "user_id"));
    assert!(child_fields.iter().any(|(name, _)| *name == "cuisine"));
}

#[test]
fn test_default_child_field_values_same_as_non_pk() {
    let u = User {
        id: 1,
        name: "T".to_string(),
        age: 25,
        email: "t@t.com".to_string(),
    };
    let child_fv = u.child_field_values();
    let non_pk_fv = u.non_pk_field_values();
    assert_eq!(child_fv.len(), non_pk_fv.len());
}

// ── Compiler parent insert/update tests ─────────────────────────────

#[test]
fn test_compile_parent_insert() {
    let compiler = django_rs_db::SqlCompiler::new(django_rs_db::DatabaseBackendType::SQLite);
    let fields: Vec<(&str, Value)> = vec![
        ("name", Value::from("Test Restaurant")),
        ("age", Value::from(5i64)),
        ("email", Value::from("test@food.com")),
    ];
    let (sql, params) = compiler.compile_parent_insert("auth_user", &fields);
    assert!(sql.contains("INSERT INTO"), "Should be an INSERT: {sql}");
    assert!(
        sql.contains("auth_user"),
        "Should reference parent table: {sql}"
    );
    assert_eq!(params.len(), 3);
}

#[test]
fn test_compile_parent_update() {
    let compiler = django_rs_db::SqlCompiler::new(django_rs_db::DatabaseBackendType::SQLite);
    let fields: Vec<(&str, Value)> = vec![("name", Value::from("Updated Restaurant"))];
    let where_clause = django_rs_db::WhereNode::Condition {
        column: "id".to_string(),
        lookup: Lookup::Exact(Value::from(1i64)),
    };
    let (sql, params) = compiler.compile_parent_update("auth_user", &fields, &where_clause);
    assert!(sql.contains("UPDATE"), "Should be an UPDATE: {sql}");
    assert!(
        sql.contains("auth_user"),
        "Should reference parent table: {sql}"
    );
    assert!(params.len() >= 2); // at least the field value and the WHERE param
}
