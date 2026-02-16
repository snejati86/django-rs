//! Integration tests for `#[derive(Model)]`.
//!
//! These tests verify that the generated `Model` trait implementation
//! produces correct metadata, field definitions, value conversions,
//! and row deserialization.

use django_rs_db::fields::{FieldType, OnDelete};
use django_rs_db::model::Model;
use django_rs_db::query::compiler::Row;
use django_rs_db::value::Value;
use django_rs_macros::Model;

// ── Basic model with all common field types ─────────────────────────────

#[derive(Model)]
#[model(table = "blog_post", app = "blog")]
pub struct Post {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field(max_length = 200)]
    pub title: String,

    #[field(blank, default = "")]
    pub subtitle: Option<String>,

    #[field]
    pub body: String,

    #[field(db_index)]
    pub published: bool,

    #[field(auto_now_add)]
    pub created_at: chrono::NaiveDateTime,

    #[field(auto_now)]
    pub updated_at: chrono::NaiveDateTime,

    #[field(foreign_key = "auth_user", on_delete = "cascade")]
    pub author_id: i64,
}

#[test]
fn test_post_table_name() {
    assert_eq!(Post::table_name(), "blog_post");
}

#[test]
fn test_post_app_label() {
    assert_eq!(Post::app_label(), "blog");
}

#[test]
fn test_post_meta_model_name() {
    let meta = Post::meta();
    assert_eq!(meta.model_name, "post");
}

#[test]
fn test_post_meta_db_table() {
    let meta = Post::meta();
    assert_eq!(meta.db_table, "blog_post");
}

#[test]
fn test_post_meta_verbose_name() {
    let meta = Post::meta();
    assert_eq!(meta.verbose_name, "post");
    assert_eq!(meta.verbose_name_plural, "posts");
}

#[test]
fn test_post_meta_field_count() {
    let meta = Post::meta();
    assert_eq!(meta.fields.len(), 8);
}

#[test]
fn test_post_meta_not_abstract() {
    assert!(!Post::meta().abstract_model);
}

#[test]
fn test_post_meta_has_index_for_published() {
    let meta = Post::meta();
    let idx = meta
        .indexes
        .iter()
        .find(|i| i.fields.contains(&"published".to_string()));
    assert!(idx.is_some(), "Should have an index on published");
    assert!(!idx.unwrap().unique);
}

#[test]
fn test_post_field_values() {
    let dt = chrono::NaiveDate::from_ymd_opt(2024, 6, 15)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let post = Post {
        id: 1,
        title: "Hello World".to_string(),
        subtitle: Some("A subtitle".to_string()),
        body: "Body text".to_string(),
        published: true,
        created_at: dt,
        updated_at: dt,
        author_id: 42,
    };

    let values = post.field_values();
    assert_eq!(values.len(), 8);
    assert_eq!(values[0], ("id", Value::Int(1)));
    assert_eq!(
        values[1],
        ("title", Value::String("Hello World".to_string()))
    );
    assert_eq!(values[3], ("body", Value::String("Body text".to_string())));
    assert_eq!(values[4], ("published", Value::Bool(true)));
    assert_eq!(values[7], ("author_id", Value::Int(42)));
}

#[test]
fn test_post_from_row() {
    let dt = chrono::NaiveDate::from_ymd_opt(2024, 6, 15)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();

    let row = Row::new(
        vec![
            "id".to_string(),
            "title".to_string(),
            "subtitle".to_string(),
            "body".to_string(),
            "published".to_string(),
            "created_at".to_string(),
            "updated_at".to_string(),
            "author_id".to_string(),
        ],
        vec![
            Value::Int(1),
            Value::String("Test".to_string()),
            Value::String("Sub".to_string()),
            Value::String("Body".to_string()),
            Value::Bool(false),
            Value::DateTime(dt),
            Value::DateTime(dt),
            Value::Int(5),
        ],
    );

    let post = Post::from_row(&row).unwrap();
    assert_eq!(post.id, 1);
    assert_eq!(post.title, "Test");
    assert_eq!(post.body, "Body");
    assert!(!post.published);
    assert_eq!(post.author_id, 5);
}

// ── Simple model with defaults ──────────────────────────────────────────

#[derive(Model)]
#[model(app = "test")]
pub struct SimpleModel {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field(max_length = 100)]
    pub name: String,
}

#[test]
fn test_simple_model_default_table() {
    assert_eq!(SimpleModel::table_name(), "test_simplemodel");
}

#[test]
fn test_simple_model_app_label() {
    assert_eq!(SimpleModel::app_label(), "test");
}

#[test]
fn test_simple_model_field_count() {
    let meta = SimpleModel::meta();
    assert_eq!(meta.fields.len(), 2);
}

#[test]
fn test_simple_model_from_row() {
    let row = Row::new(
        vec!["id".to_string(), "name".to_string()],
        vec![Value::Int(42), Value::String("Alice".to_string())],
    );
    let m = SimpleModel::from_row(&row).unwrap();
    assert_eq!(m.id, 42);
    assert_eq!(m.name, "Alice");
}

// ── Model with numeric types ────────────────────────────────────────────

#[derive(Model)]
#[model(table = "numbers", app = "math")]
pub struct NumberModel {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field]
    pub small_num: i32,

    #[field]
    pub big_num: i64,

    #[field]
    pub floating: f64,

    #[field]
    pub flag: bool,
}

#[test]
fn test_number_model_field_values() {
    let m = NumberModel {
        id: 1,
        small_num: 42,
        big_num: 999_999_999,
        floating: 3.14,
        flag: true,
    };
    let values = m.field_values();
    assert_eq!(values.len(), 5);
    assert_eq!(values[1], ("small_num", Value::Int(42)));
    assert_eq!(values[2], ("big_num", Value::Int(999_999_999)));
    assert_eq!(values[3], ("floating", Value::Float(3.14)));
    assert_eq!(values[4], ("flag", Value::Bool(true)));
}

#[test]
fn test_number_model_from_row() {
    let row = Row::new(
        vec![
            "id".to_string(),
            "small_num".to_string(),
            "big_num".to_string(),
            "floating".to_string(),
            "flag".to_string(),
        ],
        vec![
            Value::Int(1),
            Value::Int(42),
            Value::Int(999),
            Value::Float(2.71),
            Value::Bool(false),
        ],
    );
    let m = NumberModel::from_row(&row).unwrap();
    assert_eq!(m.id, 1);
    assert_eq!(m.small_num, 42);
    assert_eq!(m.big_num, 999);
    assert!((m.floating - 2.71).abs() < f64::EPSILON);
    assert!(!m.flag);
}

// ── Model with optional fields ──────────────────────────────────────────

#[derive(Model)]
#[model(table = "profiles", app = "users")]
pub struct Profile {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field(max_length = 100)]
    pub username: String,

    #[field(null)]
    pub bio: Option<String>,

    #[field(null)]
    pub age: Option<i64>,
}

#[test]
fn test_profile_nullable_fields_meta() {
    let meta = Profile::meta();
    let bio_field = meta.fields.iter().find(|f| f.name == "bio").unwrap();
    assert!(bio_field.null, "bio field should be nullable");

    let age_field = meta.fields.iter().find(|f| f.name == "age").unwrap();
    assert!(age_field.null, "age field should be nullable");
}

#[test]
fn test_profile_from_row_with_nulls() {
    let row = Row::new(
        vec![
            "id".to_string(),
            "username".to_string(),
            "bio".to_string(),
            "age".to_string(),
        ],
        vec![
            Value::Int(1),
            Value::String("alice".to_string()),
            Value::Null,
            Value::Null,
        ],
    );
    let p = Profile::from_row(&row).unwrap();
    assert_eq!(p.id, 1);
    assert_eq!(p.username, "alice");
    assert_eq!(p.bio, None);
    assert_eq!(p.age, None);
}

#[test]
fn test_profile_from_row_with_values() {
    let row = Row::new(
        vec![
            "id".to_string(),
            "username".to_string(),
            "bio".to_string(),
            "age".to_string(),
        ],
        vec![
            Value::Int(2),
            Value::String("bob".to_string()),
            Value::String("A developer".to_string()),
            Value::Int(30),
        ],
    );
    let p = Profile::from_row(&row).unwrap();
    assert_eq!(p.bio, Some("A developer".to_string()));
    assert_eq!(p.age, Some(30));
}

// ── Model with unique and indexed fields ────────────────────────────────

#[derive(Model)]
#[model(table = "users", app = "auth")]
pub struct User {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field(max_length = 150, unique)]
    pub username: String,

    #[field(max_length = 254, unique, db_index)]
    pub email: String,

    #[field]
    pub is_active: bool,
}

#[test]
fn test_user_unique_fields() {
    let meta = User::meta();
    let username_field = meta.fields.iter().find(|f| f.name == "username").unwrap();
    assert!(username_field.unique);

    let email_field = meta.fields.iter().find(|f| f.name == "email").unwrap();
    assert!(email_field.unique);
    assert!(email_field.db_index);
}

#[test]
fn test_user_indexes() {
    let meta = User::meta();
    let email_idx = meta
        .indexes
        .iter()
        .find(|i| i.fields.contains(&"email".to_string()) && !i.unique);
    assert!(email_idx.is_some(), "Should have a regular index on email");

    let email_unique_idx = meta
        .indexes
        .iter()
        .find(|i| i.fields.contains(&"email".to_string()) && i.unique);
    assert!(
        email_unique_idx.is_some(),
        "Should have a unique index on email"
    );

    let username_unique_idx = meta
        .indexes
        .iter()
        .find(|i| i.fields.contains(&"username".to_string()) && i.unique);
    assert!(
        username_unique_idx.is_some(),
        "Should have a unique index on username"
    );
}

// ── Model with foreign key ──────────────────────────────────────────────

#[derive(Model)]
#[model(table = "comments", app = "blog")]
pub struct Comment {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field]
    pub text: String,

    #[field(foreign_key = "blog_post", on_delete = "cascade")]
    pub post_id: i64,

    #[field(foreign_key = "auth_user", on_delete = "protect")]
    pub author_id: i64,
}

#[test]
fn test_comment_foreign_key_cascade() {
    let meta = Comment::meta();
    let post_field = meta.fields.iter().find(|f| f.name == "post_id").unwrap();
    if let FieldType::ForeignKey { to, on_delete, .. } = &post_field.field_type {
        assert_eq!(to, "blog_post");
        assert_eq!(*on_delete, OnDelete::Cascade);
    } else {
        panic!("Expected ForeignKey field type");
    }
}

#[test]
fn test_comment_foreign_key_protect() {
    let meta = Comment::meta();
    let author_field = meta.fields.iter().find(|f| f.name == "author_id").unwrap();
    if let FieldType::ForeignKey { to, on_delete, .. } = &author_field.field_type {
        assert_eq!(to, "auth_user");
        assert_eq!(*on_delete, OnDelete::Protect);
    } else {
        panic!("Expected ForeignKey field type");
    }
}

// ── Model with verbose names and help text ──────────────────────────────

#[derive(Model)]
#[model(
    table = "articles",
    app = "blog",
    verbose_name = "blog article",
    verbose_name_plural = "blog articles"
)]
pub struct Article {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field(
        max_length = 200,
        verbose_name = "Article Title",
        help_text = "The main title of the article"
    )]
    pub title: String,
}

#[test]
fn test_article_verbose_names() {
    let meta = Article::meta();
    assert_eq!(meta.verbose_name, "blog article");
    assert_eq!(meta.verbose_name_plural, "blog articles");
}

#[test]
fn test_article_field_verbose_name() {
    let meta = Article::meta();
    let title_field = meta.fields.iter().find(|f| f.name == "title").unwrap();
    assert_eq!(title_field.verbose_name, "Article Title");
    assert_eq!(title_field.help_text, "The main title of the article");
}

// ── Model with ordering ─────────────────────────────────────────────────

#[derive(Model)]
#[model(table = "events", app = "cal", ordering = ["-start_date", "name"])]
pub struct Event {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field(max_length = 200)]
    pub name: String,

    #[field]
    pub start_date: chrono::NaiveDateTime,
}

#[test]
fn test_event_ordering() {
    let meta = Event::meta();
    assert_eq!(meta.ordering.len(), 2);
    assert_eq!(meta.ordering[0].column, "start_date");
    assert!(meta.ordering[0].descending);
    assert_eq!(meta.ordering[1].column, "name");
    assert!(!meta.ordering[1].descending);
}

// ── Model field type inference ──────────────────────────────────────────

#[derive(Model)]
#[model(table = "type_test", app = "test")]
pub struct TypeTestModel {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field(max_length = 50)]
    pub char_field: String,

    #[field]
    pub text_field: String,

    #[field]
    pub int_field: i32,

    #[field]
    pub bigint_field: i64,

    #[field]
    pub float_field: f64,

    #[field]
    pub bool_field: bool,
}

#[test]
fn test_type_inference_char_vs_text() {
    let meta = TypeTestModel::meta();

    let char_f = meta.fields.iter().find(|f| f.name == "char_field").unwrap();
    assert!(matches!(char_f.field_type, FieldType::CharField));
    assert_eq!(char_f.max_length, Some(50));

    let text_f = meta.fields.iter().find(|f| f.name == "text_field").unwrap();
    assert!(matches!(text_f.field_type, FieldType::TextField));
}

#[test]
fn test_type_inference_numeric() {
    let meta = TypeTestModel::meta();

    let int_f = meta.fields.iter().find(|f| f.name == "int_field").unwrap();
    assert!(matches!(int_f.field_type, FieldType::IntegerField));

    let bigint_f = meta
        .fields
        .iter()
        .find(|f| f.name == "bigint_field")
        .unwrap();
    assert!(matches!(bigint_f.field_type, FieldType::BigIntegerField));

    let float_f = meta
        .fields
        .iter()
        .find(|f| f.name == "float_field")
        .unwrap();
    assert!(matches!(float_f.field_type, FieldType::FloatField));
}

#[test]
fn test_type_inference_bool() {
    let meta = TypeTestModel::meta();
    let bool_f = meta.fields.iter().find(|f| f.name == "bool_field").unwrap();
    assert!(matches!(bool_f.field_type, FieldType::BooleanField));
}

#[test]
fn test_type_inference_auto_field() {
    let meta = TypeTestModel::meta();
    let id_f = meta.fields.iter().find(|f| f.name == "id").unwrap();
    assert!(matches!(id_f.field_type, FieldType::BigAutoField));
    assert!(id_f.primary_key);
}

// ── Model pk() returns None ─────────────────────────────────────────────

#[test]
fn test_model_pk_returns_none() {
    let m = SimpleModel {
        id: 1,
        name: "Test".to_string(),
    };
    assert!(m.pk().is_none());
}

// ── Model with on_delete variants ───────────────────────────────────────

#[derive(Model)]
#[model(table = "fk_variants", app = "test")]
pub struct FkVariants {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field(foreign_key = "t1", on_delete = "set_null")]
    pub fk_set_null: i64,

    #[field(foreign_key = "t2", on_delete = "set_default")]
    pub fk_set_default: i64,

    #[field(foreign_key = "t3", on_delete = "do_nothing")]
    pub fk_do_nothing: i64,
}

#[test]
fn test_on_delete_set_null() {
    let meta = FkVariants::meta();
    let f = meta
        .fields
        .iter()
        .find(|f| f.name == "fk_set_null")
        .unwrap();
    if let FieldType::ForeignKey { on_delete, .. } = &f.field_type {
        assert_eq!(*on_delete, OnDelete::SetNull);
    } else {
        panic!("Expected ForeignKey");
    }
}

#[test]
fn test_on_delete_set_default() {
    let meta = FkVariants::meta();
    let f = meta
        .fields
        .iter()
        .find(|f| f.name == "fk_set_default")
        .unwrap();
    if let FieldType::ForeignKey { on_delete, .. } = &f.field_type {
        assert_eq!(*on_delete, OnDelete::SetDefault);
    } else {
        panic!("Expected ForeignKey");
    }
}

#[test]
fn test_on_delete_do_nothing() {
    let meta = FkVariants::meta();
    let f = meta
        .fields
        .iter()
        .find(|f| f.name == "fk_do_nothing")
        .unwrap();
    if let FieldType::ForeignKey { on_delete, .. } = &f.field_type {
        assert_eq!(*on_delete, OnDelete::DoNothing);
    } else {
        panic!("Expected ForeignKey");
    }
}

// ── Model with default value ────────────────────────────────────────────

#[derive(Model)]
#[model(table = "defaults_test", app = "test")]
pub struct DefaultsModel {
    #[field(primary_key, auto)]
    pub id: i64,

    #[field(max_length = 50, default = "draft")]
    pub status: String,
}

#[test]
fn test_default_value() {
    let meta = DefaultsModel::meta();
    let status = meta.fields.iter().find(|f| f.name == "status").unwrap();
    assert!(status.default.is_some());
    assert_eq!(status.default, Some(Value::String("draft".to_string())));
}
