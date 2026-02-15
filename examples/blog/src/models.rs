//! Blog models: Post and Comment.
//!
//! Demonstrates how to define django-rs models with the ORM's `Model` trait.
//! In a full application, these would be derived via a proc macro.

use std::sync::LazyLock;

use django_rs_core::DjangoError;
use django_rs_db::fields::{FieldDef, FieldType};
use django_rs_db::model::{Model, ModelMeta, Row};
use django_rs_db::query::compiler::{InheritanceType, OrderBy};
use django_rs_db::value::Value;

/// A blog post.
#[derive(Debug, Clone)]
pub struct Post {
    /// Primary key.
    pub id: i64,
    /// The post title.
    pub title: String,
    /// The post body content (Markdown or plain text).
    pub content: String,
    /// The author name.
    pub author: String,
    /// Creation timestamp as an ISO 8601 string.
    pub created_at: String,
    /// Whether the post is published.
    pub published: bool,
}

impl Post {
    /// Creates a new unpublished post with the current timestamp.
    pub fn new(title: impl Into<String>, content: impl Into<String>, author: impl Into<String>) -> Self {
        Self {
            id: 0,
            title: title.into(),
            content: content.into(),
            author: author.into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            published: false,
        }
    }

    /// Returns a summary of the post content (first 200 characters).
    pub fn summary(&self) -> &str {
        if self.content.len() <= 200 {
            &self.content
        } else {
            &self.content[..200]
        }
    }
}

impl Model for Post {
    fn meta() -> &'static ModelMeta {
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
                FieldDef::new("content", FieldType::TextField),
                FieldDef::new("author", FieldType::CharField).max_length(100),
                FieldDef::new("created_at", FieldType::DateTimeField),
                FieldDef::new("published", FieldType::BooleanField),
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
            // In a real implementation, the PK value would be stored
            // alongside the model. This simplified version returns None
            // to indicate "check self.id directly".
            None
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
            ("title", Value::String(self.title.clone())),
            ("content", Value::String(self.content.clone())),
            ("author", Value::String(self.author.clone())),
            ("created_at", Value::String(self.created_at.clone())),
            ("published", Value::Bool(self.published)),
        ]
    }

    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        Ok(Self {
            id: row.get::<i64>("id")?,
            title: row.get::<String>("title")?,
            content: row.get::<String>("content")?,
            author: row.get::<String>("author")?,
            created_at: row.get::<String>("created_at")?,
            published: row.get::<bool>("published")?,
        })
    }
}

/// A comment on a blog post.
#[derive(Debug, Clone)]
pub struct Comment {
    /// Primary key.
    pub id: i64,
    /// Foreign key to the parent post.
    pub post_id: i64,
    /// The comment author name.
    pub author: String,
    /// The comment text.
    pub content: String,
    /// Creation timestamp as an ISO 8601 string.
    pub created_at: String,
}

impl Comment {
    /// Creates a new comment on the given post.
    pub fn new(
        post_id: i64,
        author: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: 0,
            post_id,
            author: author.into(),
            content: content.into(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

impl Model for Comment {
    fn meta() -> &'static ModelMeta {
        static META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
            app_label: "blog",
            model_name: "comment",
            db_table: "blog_comment".to_string(),
            verbose_name: "comment".to_string(),
            verbose_name_plural: "comments".to_string(),
            ordering: vec![OrderBy::asc("created_at")],
            unique_together: vec![],
            indexes: vec![],
            abstract_model: false,
            fields: vec![
                FieldDef::new("id", FieldType::BigAutoField).primary_key(),
                FieldDef::new("post_id", FieldType::BigIntegerField),
                FieldDef::new("author", FieldType::CharField).max_length(100),
                FieldDef::new("content", FieldType::TextField),
                FieldDef::new("created_at", FieldType::DateTimeField),
            ],
            constraints: vec![],
            inheritance_type: InheritanceType::None,
        });
        &META
    }

    fn table_name() -> &'static str {
        "blog_comment"
    }

    fn app_label() -> &'static str {
        "blog"
    }

    fn pk(&self) -> Option<&Value> {
        // Simplified: always returns None. A full implementation would
        // store and return the primary key Value.
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
            ("post_id", Value::Int(self.post_id)),
            ("author", Value::String(self.author.clone())),
            ("content", Value::String(self.content.clone())),
            ("created_at", Value::String(self.created_at.clone())),
        ]
    }

    fn from_row(row: &Row) -> Result<Self, DjangoError> {
        Ok(Self {
            id: row.get::<i64>("id")?,
            post_id: row.get::<i64>("post_id")?,
            author: row.get::<String>("author")?,
            content: row.get::<String>("content")?,
            created_at: row.get::<String>("created_at")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_post_new() {
        let post = Post::new("Hello World", "My first post", "Alice");
        assert_eq!(post.id, 0);
        assert_eq!(post.title, "Hello World");
        assert_eq!(post.content, "My first post");
        assert_eq!(post.author, "Alice");
        assert!(!post.published);
        assert!(!post.created_at.is_empty());
    }

    #[test]
    fn test_post_summary_short() {
        let post = Post::new("Title", "Short content", "Author");
        assert_eq!(post.summary(), "Short content");
    }

    #[test]
    fn test_post_summary_long() {
        let long_content = "x".repeat(300);
        let post = Post::new("Title", long_content.clone(), "Author");
        assert_eq!(post.summary().len(), 200);
    }

    #[test]
    fn test_post_model_meta() {
        let meta = Post::meta();
        assert_eq!(meta.app_label, "blog");
        assert_eq!(meta.model_name, "post");
        assert_eq!(meta.db_table, "blog_post");
        assert_eq!(meta.fields.len(), 6);
    }

    #[test]
    fn test_post_table_name() {
        assert_eq!(Post::table_name(), "blog_post");
    }

    #[test]
    fn test_post_field_values() {
        let post = Post {
            id: 1,
            title: "Test".to_string(),
            content: "Body".to_string(),
            author: "Alice".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            published: true,
        };
        let values = post.field_values();
        assert_eq!(values.len(), 6);
        assert_eq!(values[0], ("id", Value::Int(1)));
        assert_eq!(values[1], ("title", Value::String("Test".to_string())));
    }

    #[test]
    fn test_post_from_row() {
        let row = Row::new(
            vec![
                "id".to_string(),
                "title".to_string(),
                "content".to_string(),
                "author".to_string(),
                "created_at".to_string(),
                "published".to_string(),
            ],
            vec![
                Value::Int(42),
                Value::String("My Post".to_string()),
                Value::String("Content here".to_string()),
                Value::String("Bob".to_string()),
                Value::String("2025-06-15T12:00:00Z".to_string()),
                Value::Bool(true),
            ],
        );

        let post = Post::from_row(&row).unwrap();
        assert_eq!(post.id, 42);
        assert_eq!(post.title, "My Post");
        assert!(post.published);
    }

    #[test]
    fn test_comment_new() {
        let comment = Comment::new(1, "Bob", "Great post!");
        assert_eq!(comment.id, 0);
        assert_eq!(comment.post_id, 1);
        assert_eq!(comment.author, "Bob");
        assert_eq!(comment.content, "Great post!");
    }

    #[test]
    fn test_comment_model_meta() {
        let meta = Comment::meta();
        assert_eq!(meta.app_label, "blog");
        assert_eq!(meta.model_name, "comment");
        assert_eq!(meta.db_table, "blog_comment");
        assert_eq!(meta.fields.len(), 5);
    }

    #[test]
    fn test_comment_from_row() {
        let row = Row::new(
            vec![
                "id".to_string(),
                "post_id".to_string(),
                "author".to_string(),
                "content".to_string(),
                "created_at".to_string(),
            ],
            vec![
                Value::Int(10),
                Value::Int(1),
                Value::String("Carol".to_string()),
                Value::String("Nice!".to_string()),
                Value::String("2025-06-15T13:00:00Z".to_string()),
            ],
        );

        let comment = Comment::from_row(&row).unwrap();
        assert_eq!(comment.id, 10);
        assert_eq!(comment.post_id, 1);
        assert_eq!(comment.author, "Carol");
    }

    #[test]
    fn test_post_set_pk() {
        let mut post = Post::new("Test", "Content", "Author");
        post.set_pk(Value::Int(99));
        assert_eq!(post.id, 99);
    }

    #[test]
    fn test_comment_set_pk() {
        let mut comment = Comment::new(1, "Author", "Content");
        comment.set_pk(Value::Int(55));
        assert_eq!(comment.id, 55);
    }
}
