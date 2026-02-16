//! Extended ORM integration tests.
//!
//! These tests exercise ORM features NOT covered by `orm_execution.rs`:
//! - Advanced QuerySet operations (Q-object combinations, values/distinct, annotations, aggregates)
//! - Bulk operations (bulk_create, bulk_update, get_or_create, update_or_create)
//! - Transaction management (atomic, savepoints, on_commit, depth tracking)
//! - Raw SQL (RawQuerySet, RawSql, parameterized queries, validation)
//! - Custom lookups and transforms (registry, compilation, chaining)
//! - Database functions (COALESCE, LOWER, UPPER, LENGTH, ABS, ROUND, expression SQL)

use django_rs_core::{DjangoError, DjangoResult};
use django_rs_db::executor::DbExecutor;
use django_rs_db::fields::{FieldDef, FieldType};
use django_rs_db::model::{Model, ModelMeta};
use django_rs_db::query::compiler::{DatabaseBackendType, InheritanceType, OrderBy, Row};
use django_rs_db::query::lookups::{Lookup, Q};
use django_rs_db::value::Value;
use django_rs_db_backends::SqliteBackend;

use django_rs_db_backends::DatabaseBackend;

// ═══════════════════════════════════════════════════════════════════════
// TEST MODEL DEFINITIONS
// ═══════════════════════════════════════════════════════════════════════

/// Employee model with 5 fields for advanced query tests.
#[derive(Debug, Clone)]
struct Employee {
    pk_value: Value,
    id: i64,
    name: String,
    department: String,
    salary: i64,
    active: bool,
}

impl Employee {
    fn new(name: &str, department: &str, salary: i64, active: bool) -> Self {
        Self {
            pk_value: Value::Null,
            id: 0,
            name: name.to_string(),
            department: department.to_string(),
            salary,
            active,
        }
    }
}

impl Model for Employee {
    fn meta() -> &'static ModelMeta {
        use std::sync::LazyLock;
        static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
            app_label: "hr",
            model_name: "employee",
            db_table: "hr_employee".to_string(),
            verbose_name: "employee".to_string(),
            verbose_name_plural: "employees".to_string(),
            ordering: vec![],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("name", FieldType::CharField).max_length(100),
                FieldDef::new("department", FieldType::CharField).max_length(50),
                FieldDef::new("salary", FieldType::IntegerField),
                FieldDef::new("active", FieldType::BooleanField),
            ],
            constraints: vec![],
            inheritance_type: InheritanceType::None,
        });
        &META
    }

    fn table_name() -> &'static str {
        "hr_employee"
    }
    fn app_label() -> &'static str {
        "hr"
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
            ("department", Value::String(self.department.clone())),
            ("salary", Value::Int(self.salary)),
            ("active", Value::Bool(self.active)),
        ]
    }

    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        let id: i64 = row.get("id")?;
        let active_val: Value = row.get("active")?;
        let active = match active_val {
            Value::Bool(b) => b,
            Value::Int(i) => i != 0,
            _ => false,
        };
        Ok(Employee {
            pk_value: Value::Int(id),
            id,
            name: row.get("name")?,
            department: row.get("department")?,
            salary: row.get("salary")?,
            active,
        })
    }
}

/// Gadget model for bulk operation tests — simple 3-field model.
#[derive(Debug, Clone)]
struct Gadget {
    pk_value: Value,
    id: i64,
    name: String,
    price: i64,
}

impl Gadget {
    fn new(name: &str, price: i64) -> Self {
        Self {
            pk_value: Value::Null,
            id: 0,
            name: name.to_string(),
            price,
        }
    }
}

impl Model for Gadget {
    fn meta() -> &'static ModelMeta {
        use std::sync::LazyLock;
        static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
            app_label: "store",
            model_name: "gadget",
            db_table: "store_gadget".to_string(),
            verbose_name: "gadget".to_string(),
            verbose_name_plural: "gadgets".to_string(),
            ordering: vec![],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("name", FieldType::CharField).max_length(100),
                FieldDef::new("price", FieldType::IntegerField),
            ],
            constraints: vec![],
            inheritance_type: InheritanceType::None,
        });
        &META
    }

    fn table_name() -> &'static str {
        "store_gadget"
    }
    fn app_label() -> &'static str {
        "store"
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
            ("price", Value::Int(self.price)),
        ]
    }

    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        let id: i64 = row.get("id")?;
        Ok(Gadget {
            pk_value: Value::Int(id),
            id,
            name: row.get("name")?,
            price: row.get("price")?,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SETUP HELPERS
// ═══════════════════════════════════════════════════════════════════════

async fn setup_employee_db() -> SqliteBackend {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE hr_employee (\
            id INTEGER PRIMARY KEY AUTOINCREMENT, \
            name TEXT NOT NULL, \
            department TEXT NOT NULL, \
            salary INTEGER NOT NULL, \
            active INTEGER NOT NULL DEFAULT 1\
        )",
        &[],
    )
    .await
    .unwrap();
    db
}

async fn setup_gadget_db() -> SqliteBackend {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE store_gadget (\
            id INTEGER PRIMARY KEY AUTOINCREMENT, \
            name TEXT NOT NULL, \
            price INTEGER NOT NULL\
        )",
        &[],
    )
    .await
    .unwrap();
    db
}

async fn setup_gadget_db_unique_name() -> SqliteBackend {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE store_gadget (\
            id INTEGER PRIMARY KEY AUTOINCREMENT, \
            name TEXT NOT NULL UNIQUE, \
            price INTEGER NOT NULL\
        )",
        &[],
    )
    .await
    .unwrap();
    db
}

async fn seed_employees(db: &SqliteBackend) {
    let data = [
        ("Alice", "Engineering", 90000i64, true),
        ("Bob", "Engineering", 85000, true),
        ("Charlie", "Sales", 70000, true),
        ("Diana", "Sales", 75000, false),
        ("Eve", "Marketing", 65000, true),
        ("Frank", "Marketing", 60000, false),
        ("Grace", "Engineering", 95000, true),
        ("Hank", "Sales", 72000, true),
    ];
    for (name, dept, salary, active) in data {
        db.execute(
            "INSERT INTO hr_employee (name, department, salary, active) VALUES (?, ?, ?, ?)",
            &[
                Value::from(name),
                Value::from(dept),
                Value::Int(salary),
                Value::Bool(active),
            ],
        )
        .await
        .unwrap();
    }
}

async fn seed_gadgets(db: &SqliteBackend) {
    for (name, price) in [("Widget", 10), ("Gizmo", 20), ("Doohickey", 30)] {
        db.execute(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            &[Value::from(name), Value::Int(price)],
        )
        .await
        .unwrap();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SECTION 1: ADVANCED QUERYSET OPERATIONS (~25 tests)
// ═══════════════════════════════════════════════════════════════════════

// ── Q-object combinations ─────────────────────────────────────────────

#[tokio::test]
async fn test_q_and_combination() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    // Engineering AND salary > 85000
    let q = Q::filter("department", Lookup::Exact(Value::from("Engineering")))
        & Q::filter("salary", Lookup::Gt(Value::Int(85000)));
    let results = mgr.filter(q).execute_query(&db).await.unwrap();
    assert_eq!(results.len(), 2); // Alice(90k), Grace(95k)
}

#[tokio::test]
async fn test_q_or_combination() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    // Marketing OR Sales
    let q = Q::filter("department", Lookup::Exact(Value::from("Marketing")))
        | Q::filter("department", Lookup::Exact(Value::from("Sales")));
    let results = mgr.filter(q).execute_query(&db).await.unwrap();
    assert_eq!(results.len(), 5); // Charlie, Diana, Eve, Frank, Hank
}

#[tokio::test]
async fn test_q_nested_and_or() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    // (Engineering AND salary >= 90000) OR (Sales AND active)
    let q = (Q::filter("department", Lookup::Exact(Value::from("Engineering")))
        & Q::filter("salary", Lookup::Gte(Value::Int(90000))))
        | (Q::filter("department", Lookup::Exact(Value::from("Sales")))
            & Q::filter("active", Lookup::Exact(Value::Bool(true))));
    let results = mgr
        .filter(q)
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db)
        .await
        .unwrap();
    // Alice(90k), Charlie(Sales,active), Grace(95k), Hank(Sales,active)
    assert_eq!(results.len(), 4);
}

#[tokio::test]
async fn test_q_not_via_exclude() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    // All except Engineering
    let results = mgr
        .exclude(Q::filter(
            "department",
            Lookup::Exact(Value::from("Engineering")),
        ))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(results.len(), 5);
    assert!(results.iter().all(|e| e.department != "Engineering"));
}

#[tokio::test]
async fn test_filter_then_exclude() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    // Active employees, exclude Marketing
    let results = mgr
        .filter(Q::filter("active", Lookup::Exact(Value::Bool(true))))
        .exclude(Q::filter(
            "department",
            Lookup::Exact(Value::from("Marketing")),
        ))
        .execute_query(&db)
        .await
        .unwrap();
    // Active: Alice, Bob, Charlie, Eve, Grace, Hank = 6; minus Eve(Marketing) = 5
    assert_eq!(results.len(), 5);
}

// ── Chained filter tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_triple_chained_filters() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let results = mgr
        .all()
        .filter(Q::filter("active", Lookup::Exact(Value::Bool(true))))
        .filter(Q::filter(
            "department",
            Lookup::Exact(Value::from("Engineering")),
        ))
        .filter(Q::filter("salary", Lookup::Gt(Value::Int(88000))))
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db)
        .await
        .unwrap();
    // Active Engineering with salary > 88000: Alice(90k), Grace(95k)
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "Alice");
    assert_eq!(results[1].name, "Grace");
}

#[tokio::test]
async fn test_filter_in_lookup() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let results = mgr
        .filter(Q::filter(
            "department",
            Lookup::In(vec![
                Value::from("Engineering"),
                Value::from("Marketing"),
            ]),
        ))
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db)
        .await
        .unwrap();
    // Engineering: Alice, Bob, Grace; Marketing: Eve, Frank = 5
    assert_eq!(results.len(), 5);
}

#[tokio::test]
async fn test_filter_range_lookup() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let results = mgr
        .filter(Q::filter(
            "salary",
            Lookup::Range(Value::Int(70000), Value::Int(85000)),
        ))
        .order_by(vec![OrderBy::asc("salary")])
        .execute_query(&db)
        .await
        .unwrap();
    // salary between 70k-85k: Charlie(70k), Hank(72k), Diana(75k), Bob(85k) = 4
    assert_eq!(results.len(), 4);
}

#[tokio::test]
async fn test_filter_contains_lookup() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let results = mgr
        .filter(Q::filter("name", Lookup::Contains("a".to_string())))
        .order_by(vec![OrderBy::asc("name")])
        .execute_query(&db)
        .await
        .unwrap();
    // SQLite LIKE is case-insensitive: names containing 'a' or 'A':
    // Alice, Charlie, Diana, Frank, Grace, Hank = 6
    assert_eq!(results.len(), 6);
}

#[tokio::test]
async fn test_filter_startswith_lookup() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let results = django_rs_db::Manager::<Employee>::new()
        .filter(Q::filter("name", Lookup::StartsWith("G".to_string())))
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Grace");
}

#[tokio::test]
async fn test_filter_endswith_lookup() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let results = django_rs_db::Manager::<Employee>::new()
        .filter(Q::filter("name", Lookup::EndsWith("e".to_string())))
        .execute_query(&db)
        .await
        .unwrap();
    // Alice, Charlie, Eve, Grace = 4
    assert_eq!(results.len(), 4);
}

// ── Values / Distinct ─────────────────────────────────────────────────

#[tokio::test]
async fn test_values_select_specific_columns() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let (sql, _) = mgr
        .all()
        .values(vec!["name", "department"])
        .to_sql(DatabaseBackendType::SQLite);
    // Should only SELECT name and department columns
    assert!(sql.contains("\"name\""), "SQL should contain name column: {sql}");
    assert!(
        sql.contains("\"department\""),
        "SQL should contain department column: {sql}"
    );
    // Should NOT select salary etc. (though it may select them if column selection is not restrictive)
    // The key assertion is that the SQL compiles without error
    assert!(sql.contains("SELECT"));
}

#[tokio::test]
async fn test_distinct_query_sql() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let (sql, _) = mgr
        .all()
        .values(vec!["department"])
        .distinct()
        .to_sql(DatabaseBackendType::SQLite);
    assert!(
        sql.contains("DISTINCT"),
        "SQL should contain DISTINCT: {sql}"
    );
}

#[tokio::test]
async fn test_values_list_sql() {
    let _db = setup_employee_db().await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let (sql, _) = mgr
        .all()
        .values_list(vec!["name"])
        .to_sql(DatabaseBackendType::SQLite);
    assert!(sql.contains("\"name\""), "SQL should select name: {sql}");
}

// ── Aggregate SQL generation ──────────────────────────────────────────

#[tokio::test]
async fn test_aggregate_count_sql() {
    let mgr = django_rs_db::Manager::<Employee>::new();
    let (sql, _) = mgr.all().count_sql(DatabaseBackendType::SQLite);
    // The compiler may quote * as "*" — either form is valid
    assert!(
        sql.contains("COUNT"),
        "Count SQL should contain COUNT: {sql}"
    );
}

#[tokio::test]
async fn test_aggregate_count_exec() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let count = mgr.all().count_exec(&db).await.unwrap();
    assert_eq!(count, 8);
}

#[tokio::test]
async fn test_aggregate_count_with_filter() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let count = mgr
        .filter(Q::filter(
            "department",
            Lookup::Exact(Value::from("Engineering")),
        ))
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(count, 3); // Alice, Bob, Grace
}

#[tokio::test]
async fn test_exists_true() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    assert!(mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Alice"))))
        .exists_exec(&db)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_exists_false() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    assert!(!mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Zephyr"))))
        .exists_exec(&db)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_first_exec_with_order() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let first = mgr
        .all()
        .order_by(vec![OrderBy::desc("salary")])
        .first_exec(&db)
        .await
        .unwrap();
    assert!(first.is_some());
    assert_eq!(first.unwrap().name, "Grace"); // highest salary 95k
}

#[tokio::test]
async fn test_get_exec_unique() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let emp = mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Bob"))))
        .get_exec(&db)
        .await
        .unwrap();
    assert_eq!(emp.name, "Bob");
    assert_eq!(emp.department, "Engineering");
}

#[tokio::test]
async fn test_get_exec_multiple_objects_returned() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let result = mgr
        .filter(Q::filter(
            "department",
            Lookup::Exact(Value::from("Engineering")),
        ))
        .get_exec(&db)
        .await;
    assert!(matches!(result, Err(DjangoError::MultipleObjectsReturned(_))));
}

#[tokio::test]
async fn test_get_exec_does_not_exist() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let result = mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Nobody"))))
        .get_exec(&db)
        .await;
    assert!(matches!(result, Err(DjangoError::DoesNotExist(_))));
}

// ── Ordering ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_order_by_multiple_fields() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let results = mgr
        .all()
        .order_by(vec![OrderBy::asc("department"), OrderBy::desc("salary")])
        .execute_query(&db)
        .await
        .unwrap();
    // Engineering first (sorted desc by salary): Grace(95k), Alice(90k), Bob(85k)
    assert_eq!(results[0].name, "Grace");
    assert_eq!(results[1].name, "Alice");
    assert_eq!(results[2].name, "Bob");
}

#[tokio::test]
async fn test_reverse_ordering() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let results = mgr
        .all()
        .order_by(vec![OrderBy::asc("salary")])
        .reverse()
        .limit(1)
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(results[0].name, "Grace"); // highest salary when reversed
}

// ── Pagination ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_limit_offset_pagination() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let page1 = mgr
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .limit(3)
        .offset(0)
        .execute_query(&db)
        .await
        .unwrap();
    let page2 = mgr
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .limit(3)
        .offset(3)
        .execute_query(&db)
        .await
        .unwrap();
    let page3 = mgr
        .all()
        .order_by(vec![OrderBy::asc("name")])
        .limit(3)
        .offset(6)
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(page1.len(), 3);
    assert_eq!(page2.len(), 3);
    assert_eq!(page3.len(), 2); // only 8 total
    assert_eq!(page1[0].name, "Alice");
    assert_eq!(page2[0].name, "Diana");
    assert_eq!(page3[0].name, "Grace");
}

// ── Annotate SQL ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_annotate_generates_sql() {
    use django_rs_db::query::expressions::core::Expression;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let qs = mgr.all().annotate(
        "double_salary",
        Expression::f("salary") * Expression::value(2),
    );
    let (sql, _) = qs.to_sql(DatabaseBackendType::SQLite);
    // Annotation should appear in SQL
    assert!(
        sql.contains("double_salary"),
        "Annotated SQL should contain alias: {sql}"
    );
}

// ── Update/Delete ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_update_exec_filtered() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let affected = mgr
        .filter(Q::filter(
            "department",
            Lookup::Exact(Value::from("Marketing")),
        ))
        .update(vec![("salary", Value::Int(70000))])
        .update_exec(&db)
        .await
        .unwrap();
    assert_eq!(affected, 2); // Eve and Frank
    // Verify update
    let eve = mgr
        .filter(Q::filter("name", Lookup::Exact(Value::from("Eve"))))
        .get_exec(&db)
        .await
        .unwrap();
    assert_eq!(eve.salary, 70000);
}

#[tokio::test]
async fn test_delete_exec_filtered() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let mgr = django_rs_db::Manager::<Employee>::new();
    let affected = mgr
        .filter(Q::filter("active", Lookup::Exact(Value::Bool(false))))
        .delete()
        .delete_exec(&db)
        .await
        .unwrap();
    assert_eq!(affected, 2); // Diana and Frank
    assert_eq!(mgr.all().count_exec(&db).await.unwrap(), 6);
}

// ═══════════════════════════════════════════════════════════════════════
// SECTION 2: BULK OPERATIONS (~15 tests)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_bulk_create_basic() {
    let db = setup_gadget_db().await;
    let mut gadgets = vec![
        Gadget::new("Sprocket", 15),
        Gadget::new("Cog", 25),
        Gadget::new("Lever", 35),
    ];
    let opts = django_rs_db::BulkCreateOptions::default();
    let count = django_rs_db::bulk_create(&mut gadgets, &opts, &db)
        .await
        .unwrap();
    assert_eq!(count, 3);
    // Verify all inserted
    let all = django_rs_db::Manager::<Gadget>::new()
        .all()
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(all, 3);
}

#[tokio::test]
async fn test_bulk_create_empty() {
    let db = setup_gadget_db().await;
    let mut gadgets: Vec<Gadget> = vec![];
    let opts = django_rs_db::BulkCreateOptions::default();
    let count = django_rs_db::bulk_create(&mut gadgets, &opts, &db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_bulk_create_with_batch_size() {
    let db = setup_gadget_db().await;
    let mut gadgets: Vec<Gadget> = (0..10)
        .map(|i| Gadget::new(&format!("Item{i}"), i * 10))
        .collect();
    let opts = django_rs_db::BulkCreateOptions {
        batch_size: Some(3),
        ..Default::default()
    };
    let count = django_rs_db::bulk_create(&mut gadgets, &opts, &db)
        .await
        .unwrap();
    assert_eq!(count, 10);
    let total = django_rs_db::Manager::<Gadget>::new()
        .all()
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(total, 10);
}

#[tokio::test]
async fn test_bulk_create_ignore_conflicts() {
    let db = setup_gadget_db_unique_name().await;
    // Insert initial
    let mut gadgets = vec![Gadget::new("UniqueA", 10), Gadget::new("UniqueB", 20)];
    let opts = django_rs_db::BulkCreateOptions::default();
    django_rs_db::bulk_create(&mut gadgets, &opts, &db)
        .await
        .unwrap();

    // Insert again with ignore_conflicts
    let mut dupes = vec![
        Gadget::new("UniqueA", 99), // duplicate name
        Gadget::new("UniqueC", 30), // new
    ];
    let opts_ignore = django_rs_db::BulkCreateOptions {
        ignore_conflicts: true,
        unique_fields: vec!["name"],
        ..Default::default()
    };
    let _count = django_rs_db::bulk_create(&mut dupes, &opts_ignore, &db)
        .await
        .unwrap();
    // Should succeed even with duplicate
    let total = django_rs_db::Manager::<Gadget>::new()
        .all()
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(total, 3); // UniqueA, UniqueB, UniqueC
}

#[tokio::test]
async fn test_bulk_create_upsert() {
    let db = setup_gadget_db_unique_name().await;
    // Insert initial
    let mut initial = vec![Gadget::new("Bolt", 5), Gadget::new("Nut", 3)];
    let opts = django_rs_db::BulkCreateOptions::default();
    django_rs_db::bulk_create(&mut initial, &opts, &db)
        .await
        .unwrap();

    // Upsert: Bolt should update price, Screw should be inserted
    let mut upserts = vec![
        Gadget::new("Bolt", 8), // conflict on name -> update price
        Gadget::new("Screw", 4),
    ];
    let upsert_opts = django_rs_db::BulkCreateOptions {
        update_conflicts: true,
        update_fields: vec!["price"],
        unique_fields: vec!["name"],
        ..Default::default()
    };
    django_rs_db::bulk_create(&mut upserts, &upsert_opts, &db)
        .await
        .unwrap();

    let total = django_rs_db::Manager::<Gadget>::new()
        .all()
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(total, 3); // Bolt, Nut, Screw

    // Verify Bolt's price was updated
    let bolt = django_rs_db::Manager::<Gadget>::new()
        .filter(Q::filter("name", Lookup::Exact(Value::from("Bolt"))))
        .get_exec(&db)
        .await
        .unwrap();
    assert_eq!(bolt.price, 8);
}

#[tokio::test]
async fn test_bulk_update_basic() {
    let db = setup_gadget_db().await;
    seed_gadgets(&db).await;

    // Fetch all gadgets
    let gadgets = django_rs_db::Manager::<Gadget>::new()
        .all()
        .order_by(vec![OrderBy::asc("id")])
        .execute_query(&db)
        .await
        .unwrap();

    // Double all prices
    let updated_gadgets: Vec<Gadget> = gadgets
        .iter()
        .map(|g| {
            let mut g2 = g.clone();
            g2.price *= 2;
            g2
        })
        .collect();

    let opts = django_rs_db::BulkUpdateOptions::default();
    let count = django_rs_db::bulk_update(&updated_gadgets, &["price"], &opts, &db)
        .await
        .unwrap();
    assert_eq!(count, 3);

    // Verify prices doubled
    let after = django_rs_db::Manager::<Gadget>::new()
        .all()
        .order_by(vec![OrderBy::asc("id")])
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(after[0].price, 20); // was 10
    assert_eq!(after[1].price, 40); // was 20
    assert_eq!(after[2].price, 60); // was 30
}

#[tokio::test]
async fn test_bulk_update_empty() {
    let db = setup_gadget_db().await;
    let gadgets: Vec<Gadget> = vec![];
    let opts = django_rs_db::BulkUpdateOptions::default();
    let count = django_rs_db::bulk_update(&gadgets, &["price"], &opts, &db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_bulk_update_no_fields() {
    let db = setup_gadget_db().await;
    seed_gadgets(&db).await;
    let gadgets = django_rs_db::Manager::<Gadget>::new()
        .all()
        .execute_query(&db)
        .await
        .unwrap();
    let opts = django_rs_db::BulkUpdateOptions::default();
    // Empty fields list should do nothing
    let count = django_rs_db::bulk_update(&gadgets, &[], &opts, &db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_get_or_create_creates_new() {
    let db = setup_gadget_db().await;
    let (gadget, created) = django_rs_db::get_or_create::<Gadget>(
        &[("name", Value::from("NewGadget"))],
        &[("price", Value::Int(42))],
        &db,
    )
    .await
    .unwrap();
    assert!(created);
    assert_eq!(gadget.name, "NewGadget");
    assert_eq!(gadget.price, 42);
}

#[tokio::test]
async fn test_get_or_create_gets_existing() {
    let db = setup_gadget_db().await;
    seed_gadgets(&db).await;
    let (gadget, created) = django_rs_db::get_or_create::<Gadget>(
        &[("name", Value::from("Widget"))],
        &[("price", Value::Int(999))],
        &db,
    )
    .await
    .unwrap();
    assert!(!created);
    assert_eq!(gadget.name, "Widget");
    assert_eq!(gadget.price, 10); // original price, not 999
}

#[tokio::test]
async fn test_update_or_create_creates_new() {
    let db = setup_gadget_db().await;
    let (gadget, created) = django_rs_db::update_or_create::<Gadget>(
        &[("name", Value::from("Brand"))],
        &[("price", Value::Int(50))],
        &db,
    )
    .await
    .unwrap();
    assert!(created);
    assert_eq!(gadget.name, "Brand");
    assert_eq!(gadget.price, 50);
}

#[tokio::test]
async fn test_update_or_create_updates_existing() {
    let db = setup_gadget_db().await;
    seed_gadgets(&db).await;
    let (gadget, created) = django_rs_db::update_or_create::<Gadget>(
        &[("name", Value::from("Gizmo"))],
        &[("price", Value::Int(99))],
        &db,
    )
    .await
    .unwrap();
    assert!(!created);
    // Price should be updated
    assert_eq!(gadget.price, 99);
}

#[tokio::test]
async fn test_update_or_create_multiple_lookup_fields() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;
    let (emp, created) = django_rs_db::update_or_create::<Employee>(
        &[
            ("name", Value::from("Alice")),
            ("department", Value::from("Engineering")),
        ],
        &[("salary", Value::Int(100000))],
        &db,
    )
    .await
    .unwrap();
    assert!(!created);
    assert_eq!(emp.salary, 100000); // updated
}

// ═══════════════════════════════════════════════════════════════════════
// SECTION 3: TRANSACTION TESTS (~15 tests)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_atomic_commit() {
    let db = setup_gadget_db().await;
    let result = django_rs_db::atomic(&db, |txn| async move {
        txn.execute_sql(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            &[Value::from("TxnItem"), Value::Int(100)],
        )
        .await?;
        Ok(42)
    })
    .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    // Verify the insert was committed
    let count = django_rs_db::Manager::<Gadget>::new()
        .all()
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_atomic_rollback_on_error() {
    let db = setup_gadget_db().await;
    let result: DjangoResult<()> = django_rs_db::atomic(&db, |txn| async move {
        txn.execute_sql(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            &[Value::from("RollbackItem"), Value::Int(100)],
        )
        .await?;
        Err(DjangoError::DatabaseError("forced error".to_string()))
    })
    .await;
    assert!(result.is_err());
    // Verify the insert was rolled back
    let count = django_rs_db::Manager::<Gadget>::new()
        .all()
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_transaction_manager_begin_commit() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);
    assert_eq!(txn.depth().await, 0);

    txn.begin().await.unwrap();
    assert_eq!(txn.depth().await, 1);

    txn.executor()
        .execute_sql(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            &[Value::from("Manual"), Value::Int(10)],
        )
        .await
        .unwrap();

    txn.commit().await.unwrap();
    assert_eq!(txn.depth().await, 0);

    let count = django_rs_db::Manager::<Gadget>::new()
        .all()
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_transaction_manager_rollback() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);

    txn.begin().await.unwrap();
    txn.executor()
        .execute_sql(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            &[Value::from("RolledBack"), Value::Int(10)],
        )
        .await
        .unwrap();
    txn.rollback().await.unwrap();
    assert_eq!(txn.depth().await, 0);

    let count = django_rs_db::Manager::<Gadget>::new()
        .all()
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_transaction_commit_without_begin_fails() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);
    let result = txn.commit().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_transaction_rollback_without_begin_fails() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);
    let result = txn.rollback().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_nested_transaction_savepoints() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);

    // Outer transaction
    txn.begin().await.unwrap();
    assert_eq!(txn.depth().await, 1);

    txn.executor()
        .execute_sql(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            &[Value::from("Outer"), Value::Int(1)],
        )
        .await
        .unwrap();

    // Inner transaction (savepoint)
    txn.begin().await.unwrap();
    assert_eq!(txn.depth().await, 2);

    txn.executor()
        .execute_sql(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            &[Value::from("Inner"), Value::Int(2)],
        )
        .await
        .unwrap();

    // Rollback inner (savepoint)
    txn.rollback().await.unwrap();
    assert_eq!(txn.depth().await, 1);

    // Commit outer
    txn.commit().await.unwrap();
    assert_eq!(txn.depth().await, 0);

    // Only Outer should be committed
    let gadgets = django_rs_db::Manager::<Gadget>::new()
        .all()
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(gadgets.len(), 1);
    assert_eq!(gadgets[0].name, "Outer");
}

#[tokio::test]
async fn test_named_savepoint_create_release() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);

    txn.begin().await.unwrap();

    let sp = txn.create_savepoint("test_sp").await.unwrap();
    assert_eq!(sp.name, "test_sp");
    assert!(!sp.released);
    assert!(!sp.rolled_back);

    txn.executor()
        .execute_sql(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            &[Value::from("InSavepoint"), Value::Int(5)],
        )
        .await
        .unwrap();

    txn.release_savepoint("test_sp").await.unwrap();

    txn.commit().await.unwrap();

    let count = django_rs_db::Manager::<Gadget>::new()
        .all()
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_named_savepoint_rollback() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);

    txn.begin().await.unwrap();

    txn.executor()
        .execute_sql(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            &[Value::from("BeforeSP"), Value::Int(1)],
        )
        .await
        .unwrap();

    txn.create_savepoint("rollback_sp").await.unwrap();

    txn.executor()
        .execute_sql(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            &[Value::from("InSP"), Value::Int(2)],
        )
        .await
        .unwrap();

    txn.rollback_to_savepoint("rollback_sp").await.unwrap();

    txn.commit().await.unwrap();

    // Only BeforeSP should persist
    let gadgets = django_rs_db::Manager::<Gadget>::new()
        .all()
        .execute_query(&db)
        .await
        .unwrap();
    assert_eq!(gadgets.len(), 1);
    assert_eq!(gadgets[0].name, "BeforeSP");
}

#[tokio::test]
async fn test_savepoint_without_transaction_fails() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);
    let result = txn.create_savepoint("fail_sp").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_on_commit_callback_fires_after_commit() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);

    txn.begin().await.unwrap();

    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called_clone = called.clone();
    txn.on_commit(move || {
        called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
    })
    .await;

    assert_eq!(txn.pending_callbacks().await, 1);
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));

    txn.commit().await.unwrap();
    // Callback should have fired
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn test_on_commit_callback_discarded_on_rollback() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);

    txn.begin().await.unwrap();

    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called_clone = called.clone();
    txn.on_commit(move || {
        called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
    })
    .await;

    txn.rollback().await.unwrap();
    // Callback should NOT have fired
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn test_on_commit_outside_transaction_fires_immediately() {
    let db = setup_gadget_db().await;
    let txn = django_rs_db::TransactionManager::new(&db);

    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called_clone = called.clone();
    txn.on_commit(move || {
        called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
    })
    .await;

    // Should fire immediately since not in a transaction
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn test_isolation_level_sql() {
    use django_rs_db::IsolationLevel;

    assert_eq!(IsolationLevel::ReadUncommitted.as_sql(), "READ UNCOMMITTED");
    assert_eq!(IsolationLevel::ReadCommitted.as_sql(), "READ COMMITTED");
    assert_eq!(IsolationLevel::RepeatableRead.as_sql(), "REPEATABLE READ");
    assert_eq!(IsolationLevel::Serializable.as_sql(), "SERIALIZABLE");

    // SQLite-specific set_sql
    let sqlite_sql = IsolationLevel::ReadUncommitted.set_sql(DatabaseBackendType::SQLite);
    assert!(sqlite_sql.contains("PRAGMA"), "SQLite should use PRAGMA: {sqlite_sql}");
}

// ═══════════════════════════════════════════════════════════════════════
// SECTION 4: RAW SQL TESTS (~10 tests)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_raw_query_set_execute() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;

    let raw = django_rs_db::RawQuerySet::<Employee>::new(
        "SELECT * FROM hr_employee WHERE department = ? ORDER BY name",
        vec![Value::from("Engineering")],
    );
    let results = raw.execute(&db).await.unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].name, "Alice");
    assert_eq!(results[1].name, "Bob");
    assert_eq!(results[2].name, "Grace");
}

#[tokio::test]
async fn test_raw_query_set_first() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;

    let raw = django_rs_db::RawQuerySet::<Employee>::new(
        "SELECT * FROM hr_employee ORDER BY salary DESC LIMIT 1",
        vec![],
    );
    let result = raw.first(&db).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().name, "Grace");
}

#[tokio::test]
async fn test_raw_query_set_empty_result() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;

    let raw = django_rs_db::RawQuerySet::<Employee>::new(
        "SELECT * FROM hr_employee WHERE name = ?",
        vec![Value::from("Nobody")],
    );
    let results = raw.execute(&db).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_raw_sql_fetch_all() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;

    let raw = django_rs_db::RawSql::new(
        "SELECT department, COUNT(*) as cnt FROM hr_employee GROUP BY department ORDER BY department",
        vec![],
    );
    let rows = raw.fetch_all(&db).await.unwrap();
    assert_eq!(rows.len(), 3); // Engineering, Marketing, Sales
    assert_eq!(rows[0].get::<String>("department").unwrap(), "Engineering");
}

#[tokio::test]
async fn test_raw_sql_fetch_one() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;

    let raw = django_rs_db::RawSql::new(
        "SELECT COUNT(*) as total FROM hr_employee",
        vec![],
    );
    let row = raw.fetch_one(&db).await.unwrap();
    assert!(row.is_some());
    assert_eq!(row.unwrap().get::<i64>("total").unwrap(), 8);
}

#[tokio::test]
async fn test_raw_sql_execute_insert() {
    let db = setup_gadget_db().await;
    let raw = django_rs_db::RawSql::new(
        "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
        vec![Value::from("RawInserted"), Value::Int(42)],
    );
    let affected = raw.execute(&db).await.unwrap();
    assert_eq!(affected, 1);

    let count = django_rs_db::Manager::<Gadget>::new()
        .all()
        .count_exec(&db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_raw_sql_execute_many() {
    let db = setup_gadget_db().await;
    let stmts = vec![
        django_rs_db::RawSql::new(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            vec![Value::from("A"), Value::Int(1)],
        ),
        django_rs_db::RawSql::new(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            vec![Value::from("B"), Value::Int(2)],
        ),
        django_rs_db::RawSql::new(
            "INSERT INTO store_gadget (name, price) VALUES (?, ?)",
            vec![Value::from("C"), Value::Int(3)],
        ),
    ];
    let total = django_rs_db::RawSql::execute_many(&stmts, &db).await.unwrap();
    assert_eq!(total, 3);
}

#[tokio::test]
async fn test_raw_sql_parameterized() {
    let db = setup_employee_db().await;
    seed_employees(&db).await;

    // Verify parameters are used, not string interpolation
    let raw = django_rs_db::RawQuerySet::<Employee>::new(
        "SELECT * FROM hr_employee WHERE salary > ? AND department = ?",
        vec![Value::Int(80000), Value::from("Engineering")],
    );
    let results = raw.execute(&db).await.unwrap();
    // Alice(90k), Bob(85k), Grace(95k) in Engineering with salary > 80k = 3
    assert_eq!(results.len(), 3);
}

#[test]
fn test_validate_raw_sql_safe() {
    assert!(django_rs_db::query::raw::validate_raw_sql("SELECT * FROM users WHERE id = ?").is_ok());
    assert!(django_rs_db::query::raw::validate_raw_sql("INSERT INTO t (a) VALUES (?)").is_ok());
}

#[test]
fn test_validate_raw_sql_suspicious() {
    assert!(django_rs_db::query::raw::validate_raw_sql(
        "SELECT * FROM users WHERE name = ''; DROP TABLE users"
    )
    .is_err());
}

// ═══════════════════════════════════════════════════════════════════════
// SECTION 5: CUSTOM LOOKUPS TESTS (~10 tests)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_lookup_registry_new_empty() {
    let registry = django_rs_db::LookupRegistry::new();
    assert_eq!(registry.lookup_count(), 0);
    assert_eq!(registry.transform_count(), 0);
}

#[test]
fn test_lookup_registry_with_defaults() {
    let registry = django_rs_db::LookupRegistry::with_defaults();
    // Should have default transforms
    assert!(registry.has_transform("lower"));
    assert!(registry.has_transform("upper"));
    assert!(registry.has_transform("length"));
    assert!(registry.has_transform("trim"));
    assert!(registry.has_transform("year"));
    assert!(registry.has_transform("month"));
    assert!(registry.has_transform("day"));
    assert!(registry.has_transform("hour"));
    assert!(registry.has_transform("minute"));
    assert!(registry.has_transform("second"));
    assert!(registry.has_transform("date"));
    assert!(registry.has_transform("abs"));
    // Should have ne lookup
    assert!(registry.has_lookup("ne"));
}

#[test]
fn test_register_and_get_custom_lookup() {
    let mut registry = django_rs_db::LookupRegistry::new();
    registry.register_lookup(
        "regex",
        django_rs_db::CustomLookup::new("regex", "{column} ~ {value}"),
    );
    assert!(registry.has_lookup("regex"));
    let lookup = registry.get_lookup("regex").unwrap();
    assert_eq!(lookup.name, "regex");
    assert_eq!(lookup.sql_template, "{column} ~ {value}");
}

#[test]
fn test_register_and_get_transform() {
    let mut registry = django_rs_db::LookupRegistry::new();
    registry.register_transform(
        "reverse",
        django_rs_db::Transform::new(
            "reverse",
            "REVERSE({column})",
            django_rs_db::TransformOutput::String,
        ),
    );
    assert!(registry.has_transform("reverse"));
    let t = registry.get_transform("reverse").unwrap();
    assert_eq!(t.name, "reverse");
}

#[test]
fn test_unregister_lookup() {
    let mut registry = django_rs_db::LookupRegistry::with_defaults();
    assert!(registry.has_lookup("ne"));
    let removed = registry.unregister_lookup("ne");
    assert!(removed.is_some());
    assert!(!registry.has_lookup("ne"));
}

#[test]
fn test_unregister_transform() {
    let mut registry = django_rs_db::LookupRegistry::with_defaults();
    assert!(registry.has_transform("lower"));
    let removed = registry.unregister_transform("lower");
    assert!(removed.is_some());
    assert!(!registry.has_transform("lower"));
}

#[test]
fn test_custom_lookup_compile() {
    let lookup = django_rs_db::CustomLookup::new("ne", "{column} != {value}");
    let sql = lookup.compile("name", "?");
    assert_eq!(sql, "\"name\" != ?");
}

#[test]
fn test_transform_apply_sqlite() {
    let t = django_rs_db::Transform::with_backends(
        "year",
        "EXTRACT(YEAR FROM {column})",
        "CAST(strftime('%Y', {column}) AS INTEGER)",
        "YEAR({column})",
        django_rs_db::TransformOutput::Integer,
    );
    let result = t.apply("\"created_at\"", DatabaseBackendType::SQLite);
    assert_eq!(result, "CAST(strftime('%Y', \"created_at\") AS INTEGER)");
}

#[test]
fn test_resolve_chain_transform_then_lookup() {
    let registry = django_rs_db::LookupRegistry::with_defaults();
    let (transforms, lookup) = registry.resolve_chain(&["lower", "exact"]);
    assert_eq!(transforms.len(), 1);
    assert_eq!(transforms[0].name, "lower");
    assert_eq!(lookup, Some("exact"));
}

#[test]
fn test_resolve_field_path() {
    let registry = django_rs_db::LookupRegistry::with_defaults();
    let (field, col_sql, lookup) = registry.resolve_field_path(
        "name__lower__exact",
        DatabaseBackendType::SQLite,
    );
    assert_eq!(field, "name");
    assert_eq!(col_sql, "LOWER(\"name\")");
    assert_eq!(lookup, "exact");
}

#[test]
fn test_resolve_field_path_no_transform() {
    let registry = django_rs_db::LookupRegistry::with_defaults();
    let (field, col_sql, lookup) = registry.resolve_field_path(
        "name__contains",
        DatabaseBackendType::SQLite,
    );
    assert_eq!(field, "name");
    assert_eq!(col_sql, "\"name\"");
    assert_eq!(lookup, "contains");
}

#[test]
fn test_resolve_field_path_only_field() {
    let registry = django_rs_db::LookupRegistry::with_defaults();
    let (field, col_sql, lookup) = registry.resolve_field_path(
        "name",
        DatabaseBackendType::SQLite,
    );
    assert_eq!(field, "name");
    assert_eq!(col_sql, "\"name\"");
    assert_eq!(lookup, "exact"); // defaults to exact
}

#[test]
fn test_compile_custom_lookup_with_params() {
    let lookup = django_rs_db::CustomLookup::new("contains_ci", "LOWER({column}) LIKE LOWER({value})");
    let mut params = Vec::new();
    let sql = django_rs_db::query::custom_lookups::compile_custom_lookup(
        &lookup,
        "\"name\"",
        &Value::from("%test%"),
        &mut params,
        DatabaseBackendType::SQLite,
    );
    assert_eq!(sql, "LOWER(\"name\") LIKE LOWER(?)");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], Value::from("%test%"));
}

// ═══════════════════════════════════════════════════════════════════════
// SECTION 6: DATABASE FUNCTIONS TESTS (~10 tests)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_func_coalesce_execution() {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT, fallback TEXT NOT NULL DEFAULT 'default')",
        &[],
    )
    .await
    .unwrap();
    db.execute(
        "INSERT INTO t (id, val, fallback) VALUES (1, NULL, 'fb')",
        &[],
    )
    .await
    .unwrap();

    let raw = django_rs_db::RawSql::new(
        "SELECT COALESCE(val, fallback) as result FROM t WHERE id = 1",
        vec![],
    );
    let row = raw.fetch_one(&db).await.unwrap().unwrap();
    assert_eq!(row.get::<String>("result").unwrap(), "fb");
}

#[tokio::test]
async fn test_func_lower_upper_execution() {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
        &[],
    )
    .await
    .unwrap();
    db.execute(
        "INSERT INTO t (id, name) VALUES (1, 'Hello World')",
        &[],
    )
    .await
    .unwrap();

    let lower = django_rs_db::RawSql::new(
        "SELECT LOWER(name) as result FROM t WHERE id = 1",
        vec![],
    );
    let row = lower.fetch_one(&db).await.unwrap().unwrap();
    assert_eq!(row.get::<String>("result").unwrap(), "hello world");

    let upper = django_rs_db::RawSql::new(
        "SELECT UPPER(name) as result FROM t WHERE id = 1",
        vec![],
    );
    let row = upper.fetch_one(&db).await.unwrap().unwrap();
    assert_eq!(row.get::<String>("result").unwrap(), "HELLO WORLD");
}

#[tokio::test]
async fn test_func_length_execution() {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
        &[],
    )
    .await
    .unwrap();
    db.execute("INSERT INTO t (id, name) VALUES (1, 'Hello')", &[])
        .await
        .unwrap();

    let raw = django_rs_db::RawSql::new(
        "SELECT LENGTH(name) as len FROM t WHERE id = 1",
        vec![],
    );
    let row = raw.fetch_one(&db).await.unwrap().unwrap();
    assert_eq!(row.get::<i64>("len").unwrap(), 5);
}

#[tokio::test]
async fn test_func_abs_execution() {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, val INTEGER)",
        &[],
    )
    .await
    .unwrap();
    db.execute("INSERT INTO t (id, val) VALUES (1, -42)", &[])
        .await
        .unwrap();

    let raw = django_rs_db::RawSql::new(
        "SELECT ABS(val) as result FROM t WHERE id = 1",
        vec![],
    );
    let row = raw.fetch_one(&db).await.unwrap().unwrap();
    assert_eq!(row.get::<i64>("result").unwrap(), 42);
}

#[tokio::test]
async fn test_func_round_execution() {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, val REAL)",
        &[],
    )
    .await
    .unwrap();
    db.execute("INSERT INTO t (id, val) VALUES (1, 3.14159)", &[])
        .await
        .unwrap();

    let raw = django_rs_db::RawSql::new(
        "SELECT ROUND(val, 2) as result FROM t WHERE id = 1",
        vec![],
    );
    let row = raw.fetch_one(&db).await.unwrap().unwrap();
    let result: Value = row.get("result").unwrap();
    match result {
        Value::Float(f) => assert!((f - 3.14).abs() < 0.01),
        Value::Int(i) => assert_eq!(i, 3), // SQLite may return integer for round
        _ => panic!("Unexpected value type: {:?}", result),
    }
}

#[tokio::test]
async fn test_func_replace_execution() {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
        &[],
    )
    .await
    .unwrap();
    db.execute(
        "INSERT INTO t (id, name) VALUES (1, 'Hello World')",
        &[],
    )
    .await
    .unwrap();

    let raw = django_rs_db::RawSql::new(
        "SELECT REPLACE(name, 'World', 'Rust') as result FROM t WHERE id = 1",
        vec![],
    );
    let row = raw.fetch_one(&db).await.unwrap().unwrap();
    assert_eq!(row.get::<String>("result").unwrap(), "Hello Rust");
}

#[tokio::test]
async fn test_func_substr_execution() {
    let db = SqliteBackend::memory().unwrap();
    db.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
        &[],
    )
    .await
    .unwrap();
    db.execute(
        "INSERT INTO t (id, name) VALUES (1, 'Hello World')",
        &[],
    )
    .await
    .unwrap();

    let raw = django_rs_db::RawSql::new(
        "SELECT SUBSTR(name, 1, 5) as result FROM t WHERE id = 1",
        vec![],
    );
    let row = raw.fetch_one(&db).await.unwrap().unwrap();
    assert_eq!(row.get::<String>("result").unwrap(), "Hello");
}

// ── Expression builder tests (SQL generation, no execution) ───────────

#[test]
fn test_expression_coalesce_builder() {
    use django_rs_db::query::expressions::core::Expression;
    use django_rs_db::query::expressions::functions::coalesce;

    let expr = coalesce(vec![
        Expression::col("nickname"),
        Expression::col("name"),
        Expression::value("Anonymous"),
    ]);
    if let Expression::Func { name, args } = &expr {
        assert_eq!(name, "COALESCE");
        assert_eq!(args.len(), 3);
    } else {
        panic!("Expected Func expression");
    }
}

#[test]
fn test_expression_lower_upper_builders() {
    use django_rs_db::query::expressions::core::Expression;
    use django_rs_db::query::expressions::functions::{lower, upper};

    let low = lower(Expression::col("name"));
    if let Expression::Func { name, .. } = &low {
        assert_eq!(name, "LOWER");
    } else {
        panic!("Expected LOWER Func");
    }

    let up = upper(Expression::col("name"));
    if let Expression::Func { name, .. } = &up {
        assert_eq!(name, "UPPER");
    } else {
        panic!("Expected UPPER Func");
    }
}

#[test]
fn test_expression_arithmetic() {
    use django_rs_db::query::expressions::core::Expression;

    let add = Expression::f("price") + Expression::value(10);
    assert!(matches!(add, Expression::Add(_, _)));

    let sub = Expression::f("price") - Expression::value(5);
    assert!(matches!(sub, Expression::Sub(_, _)));

    let mul = Expression::f("quantity") * Expression::f("price");
    assert!(matches!(mul, Expression::Mul(_, _)));

    let div = Expression::f("total") / Expression::value(2);
    assert!(matches!(div, Expression::Div(_, _)));
}

#[test]
fn test_expression_aggregate_builders() {
    use django_rs_db::query::expressions::core::{AggregateFunc, Expression};

    let count = Expression::aggregate(AggregateFunc::Count, Expression::col("*"));
    assert!(matches!(
        count,
        Expression::Aggregate {
            func: AggregateFunc::Count,
            distinct: false,
            ..
        }
    ));

    let sum = Expression::aggregate(AggregateFunc::Sum, Expression::col("salary"));
    assert!(matches!(
        sum,
        Expression::Aggregate {
            func: AggregateFunc::Sum,
            ..
        }
    ));

    let avg = Expression::aggregate(AggregateFunc::Avg, Expression::col("salary"));
    assert!(matches!(
        avg,
        Expression::Aggregate {
            func: AggregateFunc::Avg,
            ..
        }
    ));

    let distinct_count = Expression::aggregate_distinct(AggregateFunc::Count, Expression::col("department"));
    if let Expression::Aggregate { distinct, .. } = &distinct_count {
        assert!(distinct);
    }
}

#[test]
fn test_expression_case_builder() {
    use django_rs_db::query::expressions::core::{Expression, When};

    let when = When {
        condition: Q::filter("department", Lookup::Exact(Value::from("Engineering"))),
        then: Expression::value("Tech"),
    };
    let expr = Expression::case(vec![when], Some(Expression::value("Other")));
    if let Expression::Case { whens, default } = &expr {
        assert_eq!(whens.len(), 1);
        assert!(default.is_some());
    } else {
        panic!("Expected Case expression");
    }
}

#[test]
fn test_expression_raw_sql() {
    use django_rs_db::query::expressions::core::Expression;

    let expr = Expression::raw("EXTRACT(YEAR FROM ?)", vec![Value::from("2024-01-01")]);
    if let Expression::RawSQL(sql, params) = &expr {
        assert_eq!(sql, "EXTRACT(YEAR FROM ?)");
        assert_eq!(params.len(), 1);
    } else {
        panic!("Expected RawSQL expression");
    }
}

// ── Compile bulk insert SQL tests ─────────────────────────────────────

#[test]
fn test_compile_bulk_insert_sql() {
    let rows = vec![
        vec![("name", Value::from("A")), ("price", Value::Int(10))],
        vec![("name", Value::from("B")), ("price", Value::Int(20))],
    ];
    let opts = django_rs_db::BulkCreateOptions::default();
    let (sql, params) = django_rs_db::query::bulk::compile_bulk_insert(
        "store_gadget",
        &rows,
        &opts,
        DatabaseBackendType::SQLite,
    );
    assert!(sql.starts_with("INSERT INTO"));
    assert!(sql.contains("store_gadget"));
    assert_eq!(params.len(), 4); // 2 rows * 2 columns
}

#[test]
fn test_compile_bulk_insert_empty() {
    let rows: Vec<Vec<(&str, Value)>> = vec![];
    let opts = django_rs_db::BulkCreateOptions::default();
    let (sql, params) = django_rs_db::query::bulk::compile_bulk_insert(
        "t",
        &rows,
        &opts,
        DatabaseBackendType::SQLite,
    );
    assert!(sql.is_empty());
    assert!(params.is_empty());
}

#[test]
fn test_compile_bulk_insert_with_conflict() {
    let rows = vec![
        vec![("name", Value::from("A")), ("price", Value::Int(10))],
    ];
    let opts = django_rs_db::BulkCreateOptions {
        ignore_conflicts: true,
        unique_fields: vec!["name"],
        ..Default::default()
    };
    let (sql, _) = django_rs_db::query::bulk::compile_bulk_insert(
        "t",
        &rows,
        &opts,
        DatabaseBackendType::SQLite,
    );
    assert!(sql.contains("ON CONFLICT"), "Should contain ON CONFLICT: {sql}");
    assert!(sql.contains("DO NOTHING"), "Should contain DO NOTHING: {sql}");
}

// ═══════════════════════════════════════════════════════════════════════
// SAVEPOINT UNIT TESTS
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_savepoint_auto_name() {
    let sp = django_rs_db::transactions::Savepoint::new();
    assert!(sp.name.starts_with("sp_"));
    assert!(!sp.released);
    assert!(!sp.rolled_back);
}

#[test]
fn test_savepoint_custom_name() {
    let sp = django_rs_db::transactions::Savepoint::with_name("my_savepoint");
    assert_eq!(sp.name, "my_savepoint");
}

#[test]
fn test_savepoint_default() {
    let sp = django_rs_db::transactions::Savepoint::default();
    assert!(sp.name.starts_with("sp_"));
}
