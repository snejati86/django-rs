//! Blog views: handlers for listing, viewing, and creating posts.
//!
//! Demonstrates how to write view functions using the django-rs HTTP layer.
//! Each view receives an `HttpRequest` and returns an `HttpResponse`.

use std::collections::HashMap;
use std::sync::Arc;

use django_rs_http::{HttpRequest, HttpResponse, JsonResponse};
use django_rs_template::context::{Context, ContextValue};
use django_rs_template::engine::Engine;

use crate::models::Post;

/// An in-memory data store for demonstration purposes.
///
/// In a real application, views would use the ORM's `QuerySet` to access
/// the database. This simplified store shows the request/response flow.
#[derive(Debug, Clone)]
pub struct BlogStore {
    posts: Vec<Post>,
}

impl BlogStore {
    /// Creates a new store with sample data.
    pub fn with_sample_data() -> Self {
        let mut posts = Vec::new();

        let mut post1 = Post::new(
            "Getting Started with django-rs",
            "django-rs is a full-featured web framework for Rust, inspired by Django.\n\n\
             It provides an ORM, template engine, URL routing, authentication, and more.\n\n\
             This blog example demonstrates the framework's capabilities.",
            "django-rs team",
        );
        post1.id = 1;
        post1.published = true;
        posts.push(post1);

        let mut post2 = Post::new(
            "Understanding the ORM",
            "The django-rs ORM provides a familiar API for database operations.\n\n\
             Define your models as Rust structs implementing the Model trait,\n\
             then use QuerySet for filtering, ordering, and aggregation.",
            "django-rs team",
        );
        post2.id = 2;
        post2.published = true;
        posts.push(post2);

        let mut post3 = Post::new(
            "Template Inheritance in django-rs",
            "django-rs includes a full DTL-compatible template engine.\n\n\
             Use {% extends %} and {% block %} for template inheritance,\n\
             {{ variables }} for output, and {% for %}/{% if %} for logic.",
            "django-rs team",
        );
        post3.id = 3;
        post3.published = true;
        posts.push(post3);

        Self { posts }
    }

    /// Returns all published posts.
    pub fn published_posts(&self) -> Vec<&Post> {
        self.posts.iter().filter(|p| p.published).collect()
    }

    /// Returns a post by its ID.
    pub fn get_post(&self, id: i64) -> Option<&Post> {
        self.posts.iter().find(|p| p.id == id)
    }

    /// Adds a new post and returns its assigned ID.
    pub fn add_post(&mut self, mut post: Post) -> i64 {
        let next_id = self.posts.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        post.id = next_id;
        let id = post.id;
        self.posts.push(post);
        id
    }
}

/// Converts a `Post` into a `ContextValue::Dict` for template rendering.
fn post_to_context_value(post: &Post) -> ContextValue {
    let mut dict = HashMap::new();
    dict.insert("id".to_string(), ContextValue::Integer(post.id));
    dict.insert("title".to_string(), ContextValue::String(post.title.clone()));
    dict.insert("content".to_string(), ContextValue::String(post.content.clone()));
    dict.insert("author".to_string(), ContextValue::String(post.author.clone()));
    dict.insert("created_at".to_string(), ContextValue::String(post.created_at.clone()));
    dict.insert("published".to_string(), ContextValue::Bool(post.published));
    dict.insert("summary".to_string(), ContextValue::String(post.summary().to_string()));
    ContextValue::Dict(dict)
}

/// Handler for `GET /posts/` - list all published posts.
///
/// Renders the post list template with all published posts.
pub fn post_list_view(
    _request: &HttpRequest,
    store: &BlogStore,
    engine: &Engine,
) -> HttpResponse {
    let posts = store.published_posts();

    let post_values: Vec<ContextValue> = posts
        .iter()
        .map(|p| post_to_context_value(p))
        .collect();

    let mut ctx = Context::new();
    ctx.set("posts", ContextValue::List(post_values));
    ctx.set("title", ContextValue::String("All Posts".to_string()));

    match engine.render_to_string("post_list.html", &mut ctx) {
        Ok(html) => HttpResponse::ok(html),
        Err(e) => HttpResponse::server_error(format!("Template error: {e}")),
    }
}

/// Handler for `GET /posts/<id>/` - view a single post.
///
/// Renders the post detail template for the given post ID.
pub fn post_detail_view(
    _request: &HttpRequest,
    post_id: i64,
    store: &BlogStore,
    engine: &Engine,
) -> HttpResponse {
    let Some(post) = store.get_post(post_id) else {
        return HttpResponse::not_found(format!("Post {post_id} not found"));
    };

    let mut ctx = Context::new();
    ctx.set("post", post_to_context_value(post));
    ctx.set("title", ContextValue::String(post.title.clone()));

    match engine.render_to_string("post_detail.html", &mut ctx) {
        Ok(html) => HttpResponse::ok(html),
        Err(e) => HttpResponse::server_error(format!("Template error: {e}")),
    }
}

/// Handler for `POST /posts/create/` - create a new post.
///
/// Accepts JSON input and returns a JSON response with the created post.
pub fn post_create_view(
    title: &str,
    content: &str,
    author: &str,
    store: &mut BlogStore,
) -> HttpResponse {
    if title.is_empty() || content.is_empty() {
        return HttpResponse::bad_request("Title and content are required");
    }

    let post = Post::new(title, content, author);
    let id = store.add_post(post);

    let body = serde_json::json!({
        "id": id,
        "message": "Post created successfully"
    });

    JsonResponse::new(&body)
}

/// Returns a handler function compatible with the django-rs URL router.
///
/// This factory creates an Arc-wrapped async handler that captures the
/// store and engine for use in the route handler.
pub fn make_post_list_handler(
    store: Arc<BlogStore>,
    engine: Arc<Engine>,
) -> Arc<dyn Fn(HttpRequest) -> django_rs_http::BoxFuture + Send + Sync> {
    Arc::new(move |request: HttpRequest| {
        let store = Arc::clone(&store);
        let engine = Arc::clone(&engine);
        Box::pin(async move { post_list_view(&request, &store, &engine) })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_store() -> BlogStore {
        BlogStore::with_sample_data()
    }

    fn sample_engine() -> Engine {
        let engine = Engine::new();

        engine.add_string_template(
            "post_list.html",
            "<h1>{{ title }}</h1>{% for post in posts %}<p>{{ post.title }}</p>{% endfor %}",
        );
        engine.add_string_template(
            "post_detail.html",
            "<h1>{{ post.title }}</h1><p>{{ post.content }}</p><span>By {{ post.author }}</span>",
        );

        engine
    }

    #[test]
    fn test_blog_store_sample_data() {
        let store = sample_store();
        assert_eq!(store.published_posts().len(), 3);
    }

    #[test]
    fn test_blog_store_get_post() {
        let store = sample_store();
        let post = store.get_post(1).unwrap();
        assert_eq!(post.title, "Getting Started with django-rs");
    }

    #[test]
    fn test_blog_store_get_post_not_found() {
        let store = sample_store();
        assert!(store.get_post(999).is_none());
    }

    #[test]
    fn test_blog_store_add_post() {
        let mut store = sample_store();
        let post = Post::new("New Post", "Content", "Author");
        let id = store.add_post(post);
        assert_eq!(id, 4);
        assert!(store.get_post(4).is_some());
    }

    #[test]
    fn test_post_to_context_value() {
        let post = Post {
            id: 1,
            title: "Test".to_string(),
            content: "Content".to_string(),
            author: "Alice".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            published: true,
        };
        let cv = post_to_context_value(&post);
        if let ContextValue::Dict(d) = cv {
            assert_eq!(d.get("title").unwrap().to_display_string(), "Test");
            assert_eq!(d.get("author").unwrap().to_display_string(), "Alice");
        } else {
            panic!("Expected Dict");
        }
    }

    #[test]
    fn test_post_list_view_renders() {
        let store = sample_store();
        let engine = sample_engine();
        let request = HttpRequest::builder().path("/posts/").build();

        let response = post_list_view(&request, &store, &engine);
        assert_eq!(response.status(), http::StatusCode::OK);

        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("All Posts"));
        assert!(body.contains("Getting Started"));
        assert!(body.contains("Understanding the ORM"));
    }

    #[test]
    fn test_post_detail_view_renders() {
        let store = sample_store();
        let engine = sample_engine();
        let request = HttpRequest::builder().path("/posts/1/").build();

        let response = post_detail_view(&request, 1, &store, &engine);
        assert_eq!(response.status(), http::StatusCode::OK);

        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("Getting Started with django-rs"));
        assert!(body.contains("django-rs team"));
    }

    #[test]
    fn test_post_detail_view_not_found() {
        let store = sample_store();
        let engine = sample_engine();
        let request = HttpRequest::builder().path("/posts/999/").build();

        let response = post_detail_view(&request, 999, &store, &engine);
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_post_create_view_success() {
        let mut store = sample_store();
        let response = post_create_view("New Post", "Some content", "Author", &mut store);
        assert_eq!(response.status(), http::StatusCode::OK);
        assert!(store.get_post(4).is_some());
    }

    #[test]
    fn test_post_create_view_empty_title() {
        let mut store = sample_store();
        let response = post_create_view("", "Content", "Author", &mut store);
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_post_create_view_empty_content() {
        let mut store = sample_store();
        let response = post_create_view("Title", "", "Author", &mut store);
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    }
}
