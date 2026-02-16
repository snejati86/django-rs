//! # django-rs Blog Example
//!
//! A working blog application with a fully functional admin dashboard.
//!
//! ## Running
//!
//! ```bash
//! cargo run --package blog-example
//! ```
//!
//! Then open <http://localhost:8000/admin/> in your browser.
//! Login with **admin** / **admin**.
//!
//! ## Endpoints
//!
//! - `/admin/` - React admin dashboard (SPA)
//! - `/api/admin/` - Admin REST API
//! - `/api/admin/login/` - Login (POST)
//! - `/api/admin/blog/post/` - Blog posts API
//! - `/api/admin/blog/comment/` - Comments API
//! - `/api/admin/auth/user/` - Users API

mod models;
mod settings;
mod urls;
mod views;

use std::collections::HashMap;
use std::sync::Arc;

use django_rs_admin::db::{AdminDbExecutor, InMemoryAdminDb};
use django_rs_admin::log_entry::InMemoryLogEntryStore;
use django_rs_admin::model_admin::{FieldSchema, ModelAdmin};
use django_rs_admin::site::AdminSite;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
        .init();

    // ── 1. Create the shared database and log store ────────────────

    let db = Arc::new(InMemoryAdminDb::new());
    let log_store = Arc::new(InMemoryLogEntryStore::new());

    // ── 2. Register models with the admin site ─────────────────────

    let post_admin = ModelAdmin::new("blog", "post")
        .verbose_name("Post")
        .verbose_name_plural("Posts")
        .list_display(vec!["id", "title", "author", "published", "created_at"])
        .list_display_links(vec!["title"])
        .search_fields(vec!["title", "content", "author"])
        .list_filter_fields(vec!["published", "author"])
        .ordering(vec!["-id"])
        .list_per_page(25)
        .fields_schema(vec![
            FieldSchema::new("id", "BigAutoField").primary_key(),
            FieldSchema::new("title", "CharField")
                .max_length(200)
                .label("Title"),
            FieldSchema::new("content", "TextField").label("Content"),
            FieldSchema::new("author", "CharField")
                .max_length(100)
                .label("Author"),
            FieldSchema::new("published", "BooleanField").label("Published"),
            FieldSchema::new("created_at", "DateTimeField")
                .label("Created at")
                .read_only(),
        ]);

    let comment_admin = ModelAdmin::new("blog", "comment")
        .verbose_name("Comment")
        .verbose_name_plural("Comments")
        .list_display(vec!["id", "post_id", "author", "content", "created_at"])
        .list_display_links(vec!["content"])
        .search_fields(vec!["author", "content"])
        .list_filter_fields(vec!["author"])
        .ordering(vec!["-id"])
        .list_per_page(25)
        .fields_schema(vec![
            FieldSchema::new("id", "BigAutoField").primary_key(),
            FieldSchema::new("post_id", "BigIntegerField").label("Post ID"),
            FieldSchema::new("author", "CharField")
                .max_length(100)
                .label("Author"),
            FieldSchema::new("content", "TextField").label("Content"),
            FieldSchema::new("created_at", "DateTimeField")
                .label("Created at")
                .read_only(),
        ]);

    let user_admin = ModelAdmin::new("auth", "user")
        .verbose_name("User")
        .verbose_name_plural("Users")
        .list_display(vec!["id", "username", "email", "is_staff", "is_active"])
        .list_display_links(vec!["username"])
        .search_fields(vec!["username", "email"])
        .list_filter_fields(vec!["is_staff", "is_active"])
        .ordering(vec!["id"])
        .list_per_page(25)
        .fields_schema(vec![
            FieldSchema::new("id", "BigAutoField").primary_key(),
            FieldSchema::new("username", "CharField")
                .max_length(150)
                .label("Username"),
            FieldSchema::new("email", "EmailField")
                .max_length(254)
                .label("Email address"),
            FieldSchema::new("is_staff", "BooleanField").label("Staff status"),
            FieldSchema::new("is_active", "BooleanField").label("Active"),
        ]);

    let mut site = AdminSite::new("django-rs Blog Admin")
        .db(db.clone() as Arc<dyn AdminDbExecutor>)
        .log_store(log_store.clone() as Arc<dyn django_rs_admin::log_entry::LogEntryStore>);

    site.register("blog.post", post_admin);
    site.register("blog.comment", comment_admin);
    site.register("auth.user", user_admin);

    // ── 3. Seed sample data ────────────────────────────────────────

    seed_data(&db).await;
    tracing::info!("Seeded sample data into InMemoryAdminDb");

    // ── 4. Build the combined Axum router ──────────────────────────

    let admin_router = site.into_axum_router();

    // Resolve path to admin-frontend/dist/ relative to the workspace root
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let spa_dir = std::path::Path::new(manifest_dir)
        .join("../../admin-frontend/dist")
        .canonicalize()
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(manifest_dir).join("../../admin-frontend/dist")
        });

    tracing::info!("Serving SPA from: {}", spa_dir.display());

    // Serve the React SPA with fallback to index.html for client-side routing.
    // With `base: '/admin/'` in vite.config.ts, all asset paths are prefixed
    // with /admin/ so everything is served from the single nest_service.
    let spa_service = ServeDir::new(&spa_dir).fallback(tower_http::services::ServeFile::new(
        spa_dir.join("index.html"),
    ));

    // Axum's `nest()` doesn't forward `/api/admin/` (with trailing slash)
    // to the nested router's `/` route. Add an explicit redirect so the
    // frontend's `GET /api/admin/` works correctly.
    let app = axum::Router::new()
        .nest("/api/admin", admin_router)
        .route(
            "/api/admin/",
            axum::routing::get(|| async { axum::response::Redirect::temporary("/api/admin") }),
        )
        .nest_service("/admin", spa_service);

    // ── 5. Start the server ────────────────────────────────────────

    let addr = "127.0.0.1:8000";
    tracing::info!("Starting django-rs blog server on http://{addr}");
    tracing::info!("Admin dashboard: http://{addr}/admin/");
    tracing::info!("Login with: admin / admin");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Seeds sample blog data into the in-memory database.
async fn seed_data(db: &InMemoryAdminDb) {
    let post_admin =
        ModelAdmin::new("blog", "post")
            .fields_schema(vec![FieldSchema::new("id", "BigAutoField").primary_key()]);

    let comment_admin = ModelAdmin::new("blog", "comment").fields_schema(vec![FieldSchema::new(
        "id",
        "BigAutoField",
    )
    .primary_key()]);

    let user_admin =
        ModelAdmin::new("auth", "user")
            .fields_schema(vec![FieldSchema::new("id", "BigAutoField").primary_key()]);

    // ── Users ──
    for (username, email, is_staff) in [
        ("admin", "admin@example.com", true),
        ("alice", "alice@example.com", true),
        ("bob", "bob@example.com", false),
    ] {
        let mut data = HashMap::new();
        data.insert("username".to_string(), serde_json::json!(username));
        data.insert("email".to_string(), serde_json::json!(email));
        data.insert("is_staff".to_string(), serde_json::json!(is_staff));
        data.insert("is_active".to_string(), serde_json::json!(true));
        db.create_object(&user_admin, &data).await.unwrap();
    }

    // ── Posts ──
    let posts = [
        ("Getting Started with Rust", "Rust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety. In this post, we'll explore the basics of Rust and why it's becoming increasingly popular.", "admin", true),
        ("Building Web Apps with Axum", "Axum is an ergonomic and modular web framework built with Tokio, Tower, and Hyper. It makes it easy to build reliable, high-performance web services in Rust.", "alice", true),
        ("Django-rs: Django in Rust", "What if we could bring Django's developer experience to Rust? That's exactly what django-rs aims to do - a full-featured web framework inspired by Django, written in Rust.", "admin", true),
        ("Understanding Async Rust", "Async programming in Rust can be challenging at first, but once you understand the model, it becomes a powerful tool for building concurrent applications.", "alice", true),
        ("The Future of Web Frameworks", "As web development evolves, new frameworks continue to push the boundaries of performance and developer experience. Let's look at what's coming next.", "bob", false),
    ];

    for (title, content, author, published) in posts {
        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::json!(title));
        data.insert("content".to_string(), serde_json::json!(content));
        data.insert("author".to_string(), serde_json::json!(author));
        data.insert("published".to_string(), serde_json::json!(published));
        data.insert(
            "created_at".to_string(),
            serde_json::json!(chrono::Utc::now().to_rfc3339()),
        );
        db.create_object(&post_admin, &data).await.unwrap();
    }

    // ── Comments ──
    let comments = [
        (
            1,
            "bob",
            "Great introduction to Rust! Very helpful for beginners.",
        ),
        (
            1,
            "alice",
            "I'd love to see a follow-up post about ownership and borrowing.",
        ),
        (
            2,
            "admin",
            "Axum is quickly becoming my favorite web framework.",
        ),
        (
            2,
            "bob",
            "The Tower middleware ecosystem is really powerful.",
        ),
        (3, "alice", "This is exactly what the Rust ecosystem needs!"),
        (3, "bob", "Can't wait to try django-rs for my next project."),
        (4, "bob", "Finally, an async explanation that makes sense."),
    ];

    for (post_id, author, content) in comments {
        let mut data = HashMap::new();
        data.insert("post_id".to_string(), serde_json::json!(post_id));
        data.insert("author".to_string(), serde_json::json!(author));
        data.insert("content".to_string(), serde_json::json!(content));
        data.insert(
            "created_at".to_string(),
            serde_json::json!(chrono::Utc::now().to_rfc3339()),
        );
        db.create_object(&comment_admin, &data).await.unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_seed_data() {
        let db = InMemoryAdminDb::new();
        seed_data(&db).await;

        assert_eq!(db.count("auth.user"), 3);
        assert_eq!(db.count("blog.post"), 5);
        assert_eq!(db.count("blog.comment"), 7);
    }

    #[test]
    fn test_blog_settings() {
        let settings = settings::blog_settings();
        assert!(settings.debug);
        assert!(settings.installed_apps.contains(&"blog".to_string()));
    }

    #[test]
    fn test_blog_store_with_sample_data() {
        let store = views::BlogStore::with_sample_data();
        assert!(store.published_posts().len() >= 3);
    }
}
