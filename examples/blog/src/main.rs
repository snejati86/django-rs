//! # django-rs Blog Example
//!
//! A working blog application demonstrating the django-rs framework pipeline:
//!
//! - **Models**: `Post` and `Comment` implementing the `Model` trait
//! - **Views**: Function-based views for listing, viewing, and creating posts
//! - **Templates**: DTL-compatible templates with inheritance
//! - **URLs**: Named URL patterns with path converters
//! - **Settings**: Configurable via TOML or programmatic defaults
//! - **CLI**: Management commands for running, checking, and managing data
//!
//! ## Running
//!
//! ```bash
//! cargo run --package blog-example
//! ```
//!
//! This example demonstrates the framework's API and patterns. It creates
//! sample data, renders templates, and shows how the components connect.

mod models;
mod settings;
mod urls;
mod views;

use std::sync::Arc;

use django_rs_cli::command::CommandRegistry;
use django_rs_cli::commands::register_builtin_commands;
use django_rs_db::model::Model;
use django_rs_template::context::{Context, ContextValue};
use django_rs_template::engine::Engine;

use models::{Comment, Post};
use settings::{blog_settings, load_settings_from_toml};
use views::{BlogStore, post_create_view, make_post_list_handler};

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
        .init();

    // Load settings - try TOML first, fall back to programmatic defaults
    let settings = if std::path::Path::new("blog.toml").exists() {
        tracing::info!("Loading settings from blog.toml");
        load_settings_from_toml("blog.toml")
    } else {
        blog_settings()
    };
    tracing::info!(
        "Blog configured: debug={}, apps={:?}",
        settings.debug,
        settings.installed_apps
    );

    // Create the template engine with inline templates for demonstration
    let engine = create_engine();

    // Create the data store with sample posts
    let store = BlogStore::with_sample_data();

    // Show the management command registry
    demonstrate_cli(&settings);

    // Show the URL resolver
    demonstrate_urls(&store, &engine);

    // Demonstrate template rendering
    demonstrate_templates(&engine);

    // Demonstrate the model API
    demonstrate_models();

    // Demonstrate view functions
    demonstrate_views();

    // Demonstrate email sending
    demonstrate_email();

    tracing::info!("Blog example complete!");
}

/// Creates the template engine with inline templates.
fn create_engine() -> Engine {
    let engine = Engine::new();

    engine.add_string_template(
        "base.html",
        r#"<!DOCTYPE html>
<html>
<head><title>{% block title %}django-rs Blog{% endblock %}</title></head>
<body>
<nav><a href="/posts/">Home</a> | <a href="/posts/create/">New Post</a></nav>
{% block content %}{% endblock %}
<footer>Powered by django-rs</footer>
</body>
</html>"#,
    );

    engine.add_string_template(
        "post_list.html",
        r#"{% extends "base.html" %}
{% block title %}{{ title }}{% endblock %}
{% block content %}
<h1>{{ title }}</h1>
{% for post in posts %}<article><h2>{{ post.title }}</h2><p>{{ post.summary }}</p></article>
{% endfor %}{% endblock %}"#,
    );

    engine.add_string_template(
        "post_detail.html",
        r#"{% extends "base.html" %}
{% block title %}{{ post.title }}{% endblock %}
{% block content %}
<h1>{{ post.title }}</h1>
<p>By {{ post.author }} on {{ post.created_at }}</p>
<div>{{ post.content }}</div>
{% endblock %}"#,
    );

    engine
}

/// Demonstrates the CLI management command registry.
fn demonstrate_cli(settings: &django_rs_core::Settings) {
    tracing::info!("--- Management Commands ---");

    let mut registry = CommandRegistry::new();
    register_builtin_commands(&mut registry);

    tracing::info!("Registered {} commands:", registry.len());
    for name in registry.list_commands() {
        if let Some(cmd) = registry.get(name) {
            tracing::info!("  {} - {}", name, cmd.help());
        }
    }
    tracing::info!("Total: {} commands registered\n", registry.len());

    // Run the check command
    let check_cmd = registry.get("check").unwrap();
    let cli = clap::Command::new("blog")
        .subcommand(check_cmd.add_arguments(clap::Command::new("check")));
    if let Ok(matches) = cli.try_get_matches_from(["blog", "check"]) {
        let (_, sub_matches) = matches.subcommand().unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        match rt.block_on(check_cmd.handle(sub_matches, settings)) {
            Ok(()) => tracing::info!("System check passed!"),
            Err(e) => tracing::warn!("System check: {e}"),
        }
    }
}

/// Demonstrates URL routing.
fn demonstrate_urls(store: &BlogStore, _engine: &Engine) {
    tracing::info!("\n--- URL Routing ---");

    let store_arc = Arc::new(store.clone());
    let engine_arc = Arc::new(Engine::new());

    let resolver = urls::blog_urls(store_arc, engine_arc);

    // Resolve some URLs
    for path in &["posts/", "posts/1/", "posts/42/", "posts/create/"] {
        match resolver.resolve(path) {
            Ok(matched) => {
                tracing::info!(
                    "  {} -> name={:?}, kwargs={:?}",
                    path,
                    matched.url_name,
                    matched.kwargs,
                );
            }
            Err(e) => tracing::warn!("  {} -> no match: {}", path, e),
        }
    }

    // Reverse URL lookup
    let empty_kwargs = std::collections::HashMap::new();
    if let Ok(url) = django_rs_http::urls::reverse::reverse("post_list", &[], &empty_kwargs, &resolver) {
        tracing::info!("  reverse('post_list') -> {url}");
    }
}

/// Demonstrates template rendering.
fn demonstrate_templates(engine: &Engine) {
    tracing::info!("\n--- Template Rendering ---");

    let mut ctx = Context::new();
    ctx.set("title", ContextValue::String("Welcome".to_string()));

    let mut posts = Vec::new();
    let mut post_dict = std::collections::HashMap::new();
    post_dict.insert("title".to_string(), ContextValue::String("First Post".to_string()));
    post_dict.insert("summary".to_string(), ContextValue::String("A short summary...".to_string()));
    post_dict.insert("author".to_string(), ContextValue::String("Alice".to_string()));
    post_dict.insert("created_at".to_string(), ContextValue::String("2025-01-15".to_string()));
    posts.push(ContextValue::Dict(post_dict));

    ctx.set("posts", ContextValue::List(posts));

    match engine.render_to_string("post_list.html", &mut ctx) {
        Ok(html) => {
            tracing::info!("Rendered post_list.html ({} bytes)", html.len());
            // Show a snippet
            let snippet: String = html.chars().take(200).collect();
            tracing::info!("  Preview: {}...", snippet);
        }
        Err(e) => tracing::warn!("Template error: {e}"),
    }
}

/// Demonstrates the Model API.
fn demonstrate_models() {
    tracing::info!("\n--- Model API ---");

    // Create a post
    let post = Post::new(
        "Hello from django-rs",
        "This is a demonstration of the Model API.",
        "admin",
    );
    tracing::info!("Created post: {:?}", post.title);
    tracing::info!("  Table: {}", Post::table_name());
    tracing::info!("  App: {}", Post::app_label());
    tracing::info!("  Fields: {}", Post::meta().fields.len());

    let values = post.field_values();
    tracing::info!("  Field values:");
    for (name, value) in &values {
        tracing::info!("    {} = {:?}", name, value);
    }

    // Create a comment
    let comment = Comment::new(1, "Reader", "Great article!");
    tracing::info!("\nCreated comment: {:?}", comment.content);
    tracing::info!("  Table: {}", Comment::table_name());
    tracing::info!("  Fields: {}", Comment::meta().fields.len());
}

/// Demonstrates view functions for creating posts and building handlers.
fn demonstrate_views() {
    tracing::info!("\n--- View Functions ---");

    let mut store = BlogStore::with_sample_data();

    // Demonstrate post creation via the view function
    let response = post_create_view("Dynamic Post", "Created at runtime", "demo", &mut store);
    tracing::info!("Create post response: status={}", response.status());

    // Show that the store now has one more post
    tracing::info!("Store now has {} published + unpublished posts", store.published_posts().len() + 1);

    // Demonstrate the handler factory
    let store_arc = Arc::new(store);
    let engine_arc = Arc::new(Engine::new());
    let handler = make_post_list_handler(Arc::clone(&store_arc), engine_arc);
    tracing::info!("Created post list handler via factory (handler is Arc<dyn Fn>)");

    // Use the handler
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let request = django_rs_http::HttpRequest::builder().path("/posts/").build();
        let response = (handler)(request).await;
        tracing::info!("Handler response: status={}", response.status());
    });
}

/// Demonstrates the email API.
fn demonstrate_email() {
    use django_rs_cli::email::InMemoryBackend;

    tracing::info!("\n--- Email API ---");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let backend = InMemoryBackend::new();

        // Send a welcome email
        django_rs_cli::email::send_mail(
            "Welcome to the Blog!",
            "Thanks for joining our django-rs blog.",
            "noreply@blog.example.com",
            &["reader@example.com".to_string()],
            &backend,
        )
        .await
        .unwrap();

        tracing::info!(
            "Sent {} email(s) via InMemoryBackend",
            backend.message_count().await
        );

        let messages = backend.get_messages().await;
        for msg in &messages {
            tracing::info!("  Subject: {}", msg.subject);
            tracing::info!("  From: {}", msg.from_email);
            tracing::info!("  To: {}", msg.to.join(", "));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_engine() {
        let engine = create_engine();
        let mut ctx = Context::new();
        ctx.set("title", ContextValue::String("Test".to_string()));
        ctx.set("posts", ContextValue::List(vec![]));

        let result = engine.render_to_string("post_list.html", &mut ctx);
        assert!(result.is_ok());
        let html = result.unwrap();
        assert!(html.contains("Test"));
        assert!(html.contains("django-rs"));
    }

    #[test]
    fn test_blog_settings() {
        let settings = blog_settings();
        assert!(settings.debug);
        assert!(settings.installed_apps.contains(&"blog".to_string()));
    }

    #[test]
    fn test_blog_store_with_sample_data() {
        let store = BlogStore::with_sample_data();
        assert!(store.published_posts().len() >= 3);
    }
}
