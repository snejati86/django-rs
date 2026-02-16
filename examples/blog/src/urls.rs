//! URL configuration for the blog application.
//!
//! Demonstrates how to define URL patterns using the django-rs HTTP layer.
//! Patterns are defined using `path()` with named routes and converters.

use std::sync::Arc;

use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{root, URLEntry, URLResolver};
use django_rs_http::{HttpRequest, HttpResponse};
use django_rs_template::engine::Engine;

use crate::views::{post_detail_view, post_list_view, BlogStore};

/// Creates the blog URL configuration.
///
/// Returns a `URLResolver` with the following patterns:
/// - `posts/` -> Post list view
/// - `posts/<int:id>/` -> Post detail view
/// - `posts/create/` -> Post create view
pub fn blog_urls(store: Arc<BlogStore>, engine: Arc<Engine>) -> URLResolver {
    let list_store = Arc::clone(&store);
    let list_engine = Arc::clone(&engine);
    let list_handler: Arc<dyn Fn(HttpRequest) -> django_rs_http::BoxFuture + Send + Sync> =
        Arc::new(move |request: HttpRequest| {
            let s = Arc::clone(&list_store);
            let e = Arc::clone(&list_engine);
            Box::pin(async move { post_list_view(&request, &s, &e) })
        });

    let detail_store = Arc::clone(&store);
    let detail_engine = Arc::clone(&engine);
    let detail_handler: Arc<dyn Fn(HttpRequest) -> django_rs_http::BoxFuture + Send + Sync> =
        Arc::new(move |request: HttpRequest| {
            let s = Arc::clone(&detail_store);
            let e = Arc::clone(&detail_engine);
            Box::pin(async move {
                // Extract the post ID from the resolver match kwargs
                let id = request
                    .resolver_match()
                    .and_then(|m| m.kwargs.get("id"))
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                post_detail_view(&request, id, &s, &e)
            })
        });

    let create_handler: Arc<dyn Fn(HttpRequest) -> django_rs_http::BoxFuture + Send + Sync> =
        Arc::new(move |_request: HttpRequest| {
            Box::pin(async move {
                // In a full implementation, this would parse the request body.
                // For demonstration, we return a placeholder response.
                HttpResponse::ok("Post creation form would be rendered here")
            })
        });

    let patterns = vec![
        URLEntry::Pattern(path("posts/", list_handler, Some("post_list")).unwrap()),
        URLEntry::Pattern(path("posts/<int:id>/", detail_handler, Some("post_detail")).unwrap()),
        URLEntry::Pattern(path("posts/create/", create_handler, Some("post_create")).unwrap()),
    ];

    root(patterns).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::BlogStore;

    fn setup() -> (Arc<BlogStore>, Arc<Engine>) {
        let store = Arc::new(BlogStore::with_sample_data());
        let engine = Engine::new();
        engine.add_string_template(
            "post_list.html",
            "<h1>{{ title }}</h1>{% for post in posts %}<p>{{ post.title }}</p>{% endfor %}",
        );
        engine.add_string_template(
            "post_detail.html",
            "<h1>{{ post.title }}</h1><p>{{ post.content }}</p>",
        );
        (store, Arc::new(engine))
    }

    #[test]
    fn test_blog_urls_resolves_post_list() {
        let (store, engine) = setup();
        let resolver = blog_urls(store, engine);

        let result = resolver.resolve("posts/");
        assert!(result.is_ok());
        let matched = result.unwrap();
        assert_eq!(matched.url_name.as_deref(), Some("post_list"));
    }

    #[test]
    fn test_blog_urls_resolves_post_detail() {
        let (store, engine) = setup();
        let resolver = blog_urls(store, engine);

        let result = resolver.resolve("posts/42/");
        assert!(result.is_ok());
        let matched = result.unwrap();
        assert_eq!(matched.url_name.as_deref(), Some("post_detail"));
        assert_eq!(matched.kwargs.get("id").map(String::as_str), Some("42"));
    }

    #[test]
    fn test_blog_urls_resolves_post_create() {
        let (store, engine) = setup();
        let resolver = blog_urls(store, engine);

        let result = resolver.resolve("posts/create/");
        assert!(result.is_ok());
        let matched = result.unwrap();
        assert_eq!(matched.url_name.as_deref(), Some("post_create"));
    }

    #[test]
    fn test_blog_urls_no_match() {
        let (store, engine) = setup();
        let resolver = blog_urls(store, engine);

        let result = resolver.resolve("unknown/");
        assert!(result.is_err());
    }

    #[test]
    fn test_blog_urls_reverse_post_list() {
        let (store, engine) = setup();
        let resolver = blog_urls(store, engine);

        let empty_kwargs = std::collections::HashMap::new();
        let url =
            django_rs_http::urls::reverse::reverse("post_list", &[], &empty_kwargs, &resolver);
        assert!(url.is_ok());
        assert_eq!(url.unwrap(), "/posts/");
    }
}
