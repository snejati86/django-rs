# Tutorial 1: Getting Started

In this tutorial you will create your first django-rs project from scratch. You will set up a Rust project, write HTTP handlers, define URL routes with path converters, and run a development server that responds to requests.

By the end of this tutorial you will have a working web server with multiple URL routes, named patterns, path converters that capture typed URL segments, and reverse URL resolution -- all powered by django-rs.

This tutorial mirrors [Django's Tutorial Part 1](https://docs.djangoproject.com/en/stable/intro/tutorial01/), adapted for Rust.

---

## Installing Rust

If you do not already have Rust installed, install it via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

After installation, verify that you have Rust 1.75 or later:

```bash
rustc --version
# rustc 1.75.0 (or later)

cargo --version
# cargo 1.75.0 (or later)
```

If you already have Rust but need to update:

```bash
rustup update stable
```

---

## Creating a new project

Create a new Rust binary project. We will call it `myapp`:

```bash
cargo new myapp
cd myapp
```

Cargo generates the following structure:

```
myapp/
├── Cargo.toml
└── src/
    └── main.rs
```

- **`Cargo.toml`** -- The project manifest, where you declare dependencies and metadata.
- **`src/main.rs`** -- The entry point for your application.

---

## Adding django-rs as a dependency

Open `Cargo.toml` and replace its contents with:

```toml
[package]
name = "myapp"
version = "0.1.0"
edition = "2021"

[dependencies]
django-rs-http = { git = "https://github.com/snejati86/django-rs" }
axum = "0.8"
tokio = { version = "1", features = ["full"] }
http-body-util = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Here is what each dependency does:

| Dependency | Purpose |
|-----------|---------|
| `django-rs-http` | The django-rs HTTP crate -- request/response handling, URL routing, path converters |
| `axum` | The HTTP server framework we use to serve requests |
| `tokio` | The async runtime that powers the server |
| `http-body-util` | Utilities for working with HTTP bodies in the axum integration |
| `serde` / `serde_json` | JSON serialization for API responses |

Run `cargo build` to download and compile the dependencies. The first build will take a minute or two as Cargo compiles the entire dependency tree:

```bash
cargo build
```

---

## Project structure explanation

At this stage your project is minimal, but it is worth understanding how a django-rs application is organized conceptually. Unlike Django, which uses a CLI to scaffold projects and apps, django-rs follows standard Rust project conventions. You structure your code with modules and crates.

A typical django-rs project looks like this:

```
myapp/
├── Cargo.toml              # Dependencies and project metadata
├── src/
│   ├── main.rs             # Entry point: server startup and root URL config
│   ├── urls.rs             # Root URL configuration
│   ├── blog/               # A "blog" app (a Rust module)
│   │   ├── mod.rs
│   │   ├── views.rs        # View/handler functions
│   │   ├── urls.rs         # URL patterns for this app
│   │   ├── models.rs       # Database models (Tutorial 2)
│   │   └── forms.rs        # Forms (Tutorial 4)
│   └── templates/          # HTML templates (Tutorial 3)
│       └── blog/
│           ├── index.html
│           └── detail.html
└── static/                 # Static files (CSS, JS, images)
```

For this first tutorial, everything will live in `src/main.rs`. We will split things into separate modules starting in Tutorial 2.

---

## Writing your first HTTP handler

In Django, a *view* is a function that takes a request and returns a response. django-rs follows the exact same pattern. Open `src/main.rs` and replace its contents with:

```rust
use django_rs_http::{HttpRequest, HttpResponse};

fn index(_request: &HttpRequest) -> HttpResponse {
    HttpResponse::ok("Hello, django-rs!")
}

fn main() {
    // We will wire up routing and a server shortly.
    // For now, let's verify the view works by calling it directly.
    let request = HttpRequest::builder()
        .method(http::Method::GET)
        .path("/")
        .build();

    let response = index(&request);
    println!("Status: {}", response.status_code());
    println!("Body: {}", response.content());
}
```

Run it:

```bash
cargo run
```

You should see:

```
Status: 200
Body: Hello, django-rs!
```

This confirms our view function works. It takes a reference to an `HttpRequest` and returns an `HttpResponse`. The `HttpResponse::ok()` constructor creates a 200 OK response with the given body text.

---

## The request/response cycle

Before wiring up routing, let us understand the two core types you will work with in every django-rs view.

### HttpRequest

`HttpRequest` represents an incoming HTTP request. It provides access to everything you need:

| Method | Returns | Description |
|--------|---------|-------------|
| `request.method()` | `&http::Method` | The HTTP method (`GET`, `POST`, etc.) |
| `request.path()` | `&str` | The URL path (e.g., `"/articles/2024/"`) |
| `request.query_string()` | `&str` | The raw query string (without the leading `?`) |
| `request.headers()` | Headers map | The HTTP headers |
| `request.cookies()` | Cookie map | Parsed cookies from the `Cookie` header |
| `request.get()` | `QueryDict` | GET parameters parsed from the query string |
| `request.post()` | `QueryDict` | POST form parameters parsed from the body |
| `request.body()` | `&[u8]` | The raw request body as bytes |
| `request.meta()` | Metadata map | Server metadata (`SERVER_NAME`, etc.) |
| `request.is_secure()` | `bool` | `true` if the request uses HTTPS |
| `request.is_ajax()` | `bool` | `true` if `X-Requested-With: XMLHttpRequest` |
| `request.get_host()` | `String` | The hostname from the `Host` header |
| `request.get_full_path()` | `String` | Path including query string (e.g., `"/foo/?page=2"`) |
| `request.scheme()` | `&str` | URL scheme (`"http"` or `"https"`) |

You can construct requests manually using the builder pattern, which is particularly useful for testing:

```rust
use django_rs_http::HttpRequest;

let request = HttpRequest::builder()
    .method(http::Method::GET)
    .path("/articles/")
    .query_string("page=2&sort=date")
    .build();

assert_eq!(request.method(), &http::Method::GET);
assert_eq!(request.path(), "/articles/");
assert_eq!(request.get().get("page"), Some("2"));
assert_eq!(request.get().get("sort"), Some("date"));
assert_eq!(request.get_full_path(), "/articles/?page=2&sort=date");
```

### HttpResponse

`HttpResponse` is the return type for every view. It carries a status code, headers, and body content. django-rs provides convenient constructors for common status codes:

```rust
use django_rs_http::HttpResponse;

// 200 OK
let response = HttpResponse::ok("Hello, World!");

// 201 Created
let response = HttpResponse::new(http::StatusCode::CREATED, "Resource created");

// 404 Not Found
let response = HttpResponse::not_found("Page not found");

// 403 Forbidden
let response = HttpResponse::forbidden("Access denied");

// 400 Bad Request
let response = HttpResponse::bad_request("Invalid input");

// 500 Internal Server Error
let response = HttpResponse::server_error("Something went wrong");
```

You can also set headers:

```rust
use django_rs_http::HttpResponse;
use http::header::HeaderValue;

let response = HttpResponse::ok("cached content")
    .set_header(
        http::header::CACHE_CONTROL,
        HeaderValue::from_static("max-age=3600"),
    );
```

### JSON responses

For API endpoints, use `JsonResponse` to serialize data and set the `Content-Type: application/json` header automatically:

```rust
use django_rs_http::JsonResponse;

let data = serde_json::json!({
    "status": "ok",
    "count": 42
});
let response = JsonResponse::new(&data);
// Status: 200, Content-Type: application/json
```

You can also set a custom status code:

```rust
use django_rs_http::JsonResponse;

let data = serde_json::json!({"id": 1, "title": "New Post"});
let response = JsonResponse::with_status(http::StatusCode::CREATED, &data);
```

### Redirects

django-rs provides redirect response types that mirror Django's:

```rust
use django_rs_http::{HttpResponseRedirect, HttpResponsePermanentRedirect};

// 302 Found (temporary redirect)
let response = HttpResponseRedirect::new("/new-location/");

// 301 Moved Permanently
let response = HttpResponsePermanentRedirect::new("/permanent-location/");
```

---

## URL routing with path()

Now for the core of this tutorial: URL routing. django-rs uses a `path()` function that will look very familiar if you know Django. It accepts a route string, a handler function, and an optional name.

### Defining URL patterns

Update `src/main.rs` to define some routes:

```rust
use std::sync::Arc;

use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{root, URLEntry};
use django_rs_http::{BoxFuture, HttpRequest, HttpResponse};

// View functions
fn index(_request: &HttpRequest) -> HttpResponse {
    HttpResponse::ok("<h1>Welcome to django-rs!</h1>")
}

fn about(_request: &HttpRequest) -> HttpResponse {
    HttpResponse::ok("<h1>About</h1><p>Built with django-rs.</p>")
}

fn main() {
    // Wrap view functions as route handlers.
    // Route handlers must be Arc-wrapped async closures.
    let index_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|request: HttpRequest| {
            Box::pin(async move { index(&request) })
        });

    let about_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|request: HttpRequest| {
            Box::pin(async move { about(&request) })
        });

    // Define URL patterns
    let patterns = vec![
        URLEntry::Pattern(path("", index_handler, Some("index")).unwrap()),
        URLEntry::Pattern(path("about/", about_handler, Some("about")).unwrap()),
    ];

    // Create the root URL resolver
    let resolver = root(patterns).unwrap();

    // Test: resolve the index URL
    let matched = resolver.resolve("").unwrap();
    assert_eq!(matched.url_name.as_deref(), Some("index"));

    // Test: resolve the about URL
    let matched = resolver.resolve("about/").unwrap();
    assert_eq!(matched.url_name.as_deref(), Some("about"));

    println!("URL patterns configured successfully!");
}
```

Let us break down the key concepts:

- **`path(route, handler, name)`** creates a `URLPattern`. The route string uses Django-style syntax with trailing slashes. The name is optional but recommended -- it enables reverse URL resolution.
- **`URLEntry::Pattern(...)`** wraps a pattern into a URL entry that can be added to a resolver.
- **`root(patterns)`** creates a root-level `URLResolver` that matches incoming paths against all entries.
- **`resolver.resolve(path)`** returns a `ResolverMatch` containing the matched handler, captured arguments, and the URL name.

### Route handlers

Route handlers in django-rs have the type signature:

```rust
Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync>
```

This is an `Arc`-wrapped function that takes an `HttpRequest` by value and returns a pinned, boxed future that produces an `HttpResponse`. The `Arc` allows the handler to be shared across threads in the async runtime. The general pattern for wrapping a view function is:

```rust
let handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
    Arc::new(|request: HttpRequest| {
        Box::pin(async move {
            // Your view logic here
            HttpResponse::ok("response body")
        })
    });
```

---

## Path converters

One of Django's most useful features is path converters -- typed placeholders in URL patterns that automatically capture and validate URL segments. django-rs supports the same set of converters.

### Built-in converters

| Converter | Syntax | Matches | Example |
|-----------|--------|---------|---------|
| `int` | `<int:name>` | One or more digits | `42`, `2024` |
| `str` | `<str:name>` | Any non-empty string without `/` | `hello`, `my-page` |
| `slug` | `<slug:name>` | Letters, digits, hyphens, and underscores | `my-first-post` |
| `uuid` | `<uuid:name>` | A standard UUID | `550e8400-e29b-41d4-a716-446655440000` |
| `path` | `<path:name>` | Any non-empty string, including `/` | `docs/2024/readme.md` |

If you omit the type (e.g., `<name>` instead of `<str:name>`), it defaults to `str`.

### Using converters in routes

Here is a complete example demonstrating each converter:

```rust
use std::sync::Arc;

use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{root, URLEntry};
use django_rs_http::{BoxFuture, HttpRequest, HttpResponse};

fn main() {
    let handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|_req: HttpRequest| {
            Box::pin(async { HttpResponse::ok("ok") })
        });

    let patterns = vec![
        // <int:id> -- matches digits only
        // Matches: articles/2024/, articles/1/
        // Does NOT match: articles/abc/, articles//
        URLEntry::Pattern(
            path("articles/<int:year>/", handler.clone(), Some("article-year")).unwrap()
        ),

        // <slug:slug> -- matches letters, digits, hyphens, underscores
        // Matches: posts/my-first-post/, posts/hello_world/
        URLEntry::Pattern(
            path("posts/<slug:slug>/", handler.clone(), Some("post-detail")).unwrap()
        ),

        // <str:name> -- matches any non-empty string without /
        // Matches: users/alice/, users/bob/
        URLEntry::Pattern(
            path("users/<str:username>/", handler.clone(), Some("user-profile")).unwrap()
        ),

        // <path:filepath> -- matches any string including /
        // Matches: files/docs/readme.md, files/images/photo.png
        URLEntry::Pattern(
            path("files/<path:filepath>", handler.clone(), Some("file-view")).unwrap()
        ),

        // <uuid:id> -- matches a standard UUID
        // Matches: items/550e8400-e29b-41d4-a716-446655440000/
        URLEntry::Pattern(
            path("items/<uuid:id>/", handler.clone(), Some("item-detail")).unwrap()
        ),
    ];

    let resolver = root(patterns).unwrap();

    // Resolve URLs and inspect captured kwargs
    let matched = resolver.resolve("articles/2024/").unwrap();
    assert_eq!(matched.url_name.as_deref(), Some("article-year"));
    assert_eq!(matched.kwargs.get("year").unwrap(), "2024");

    let matched = resolver.resolve("posts/my-first-post/").unwrap();
    assert_eq!(matched.kwargs.get("slug").unwrap(), "my-first-post");

    let matched = resolver.resolve("users/alice/").unwrap();
    assert_eq!(matched.kwargs.get("username").unwrap(), "alice");

    let matched = resolver.resolve("files/docs/readme.md").unwrap();
    assert_eq!(matched.kwargs.get("filepath").unwrap(), "docs/readme.md");

    // Int converter rejects non-numeric values
    assert!(resolver.resolve("articles/abc/").is_err());

    println!("All path converter tests passed!");
}
```

### Multiple converters in one route

You can chain multiple converters in a single pattern:

```rust
use std::sync::Arc;

use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{root, URLEntry};
use django_rs_http::{BoxFuture, HttpRequest, HttpResponse};

fn main() {
    let handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|_req: HttpRequest| {
            Box::pin(async { HttpResponse::ok("ok") })
        });

    let patterns = vec![
        URLEntry::Pattern(
            path(
                "articles/<int:year>/<slug:title>/",
                handler,
                Some("article-detail"),
            )
            .unwrap(),
        ),
    ];

    let resolver = root(patterns).unwrap();

    let matched = resolver.resolve("articles/2024/hello-world/").unwrap();
    assert_eq!(matched.kwargs.get("year").unwrap(), "2024");
    assert_eq!(matched.kwargs.get("title").unwrap(), "hello-world");

    println!("Multi-converter route resolved!");
}
```

---

## Setting up the Axum server

Now let us put everything together into a fully working web server. We will use Axum as the HTTP server and integrate django-rs's URL resolver to dispatch requests to the correct handlers.

Replace `src/main.rs` with the following complete example:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{include, root, URLEntry, URLResolver};
use django_rs_http::urls::reverse::reverse;
use django_rs_http::{BoxFuture, HttpRequest, HttpResponse, JsonResponse};

// ── Views ──────────────────────────────────────────────────────────

fn index_view(_request: &HttpRequest) -> HttpResponse {
    HttpResponse::ok(
        "<h1>Welcome to django-rs</h1>\
         <p>Visit <a href=\"/blog/\">/blog/</a> to see the blog.</p>",
    )
}

fn blog_index_view(_request: &HttpRequest) -> HttpResponse {
    HttpResponse::ok("<h1>Blog</h1><p>All posts will appear here.</p>")
}

fn blog_detail_view(request: &HttpRequest) -> HttpResponse {
    // Access captured URL parameters via resolver_match
    let post_id = request
        .resolver_match()
        .and_then(|m| m.kwargs.get("id"))
        .map(String::as_str)
        .unwrap_or("unknown");

    HttpResponse::ok(format!(
        "<h1>Blog Post</h1><p>You are reading post {post_id}.</p>"
    ))
}

fn blog_archive_view(request: &HttpRequest) -> HttpResponse {
    let year = request
        .resolver_match()
        .and_then(|m| m.kwargs.get("year"))
        .map(String::as_str)
        .unwrap_or("unknown");
    let slug = request
        .resolver_match()
        .and_then(|m| m.kwargs.get("slug"))
        .map(String::as_str)
        .unwrap_or("unknown");

    HttpResponse::ok(format!(
        "<h1>Blog Archive</h1><p>Post: {slug} ({year})</p>"
    ))
}

fn api_posts_view(_request: &HttpRequest) -> HttpResponse {
    let data = serde_json::json!({
        "posts": [
            {"id": 1, "title": "First Post"},
            {"id": 2, "title": "Second Post"}
        ]
    });
    JsonResponse::new(&data)
}

// ── URL Configuration ──────────────────────────────────────────────

fn blog_urls() -> Vec<URLEntry> {
    let index_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|request: HttpRequest| {
            Box::pin(async move { blog_index_view(&request) })
        });

    let detail_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|request: HttpRequest| {
            Box::pin(async move { blog_detail_view(&request) })
        });

    let archive_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|request: HttpRequest| {
            Box::pin(async move { blog_archive_view(&request) })
        });

    vec![
        URLEntry::Pattern(path("", index_handler, Some("index")).unwrap()),
        URLEntry::Pattern(
            path("<int:id>/", detail_handler, Some("detail")).unwrap(),
        ),
        URLEntry::Pattern(
            path(
                "<int:year>/<slug:slug>/",
                archive_handler,
                Some("archive"),
            )
            .unwrap(),
        ),
    ]
}

fn api_urls() -> Vec<URLEntry> {
    let posts_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|request: HttpRequest| {
            Box::pin(async move { api_posts_view(&request) })
        });

    vec![
        URLEntry::Pattern(path("posts/", posts_handler, Some("posts")).unwrap()),
    ]
}

fn url_config() -> URLResolver {
    let index_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|request: HttpRequest| {
            Box::pin(async move { index_view(&request) })
        });

    let patterns = vec![
        URLEntry::Pattern(path("", index_handler, Some("home")).unwrap()),
        URLEntry::Resolver(
            include("blog/", blog_urls(), Some("blog"), Some("blog")).unwrap(),
        ),
        URLEntry::Resolver(
            include("api/", api_urls(), Some("api"), Some("api")).unwrap(),
        ),
    ];

    root(patterns).unwrap()
}

// ── Server ─────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // Build the URL resolver
    let resolver = Arc::new(url_config());

    // Demonstrate reverse URL resolution
    let mut kwargs = HashMap::new();
    kwargs.insert("id", "5");
    let detail_url = reverse("blog:detail", &[], &kwargs, &resolver).unwrap();
    println!("Reversed blog:detail -> {detail_url}");

    let mut kwargs = HashMap::new();
    kwargs.insert("year", "2024");
    kwargs.insert("slug", "hello-world");
    let archive_url = reverse("blog:archive", &[], &kwargs, &resolver).unwrap();
    println!("Reversed blog:archive -> {archive_url}");

    // Build an Axum router that delegates to the django-rs resolver
    let resolver_for_handler = Arc::clone(&resolver);
    let app = axum::Router::new().fallback(move |req: axum::extract::Request| {
        let resolver = Arc::clone(&resolver_for_handler);
        async move {
            // Convert the axum request into a django-rs HttpRequest
            let (parts, body) = req.into_parts();
            let body_bytes = axum::body::to_bytes(
                axum::body::Body::from(http_body_util::Full::from(
                    axum::body::to_bytes(body, usize::MAX)
                        .await
                        .unwrap_or_default(),
                )),
                usize::MAX,
            )
            .await
            .unwrap_or_default()
            .to_vec();

            let mut django_request = HttpRequest::from_axum(parts, body_bytes);

            // Strip leading slash -- the resolver expects paths without it
            let path = django_request.path().trim_start_matches('/').to_string();

            // Resolve the URL to a handler
            match resolver.resolve(&path) {
                Ok(resolver_match) => {
                    let handler = resolver_match.func.clone();
                    django_request.set_resolver_match(resolver_match);
                    let response = handler(django_request).await;
                    axum::response::IntoResponse::into_response(response)
                }
                Err(_) => {
                    let response = HttpResponse::not_found(
                        "<h1>404 Not Found</h1>\
                         <p>The requested URL was not found on this server.</p>",
                    );
                    axum::response::IntoResponse::into_response(response)
                }
            }
        }
    });

    // Start the server
    let addr = "127.0.0.1:8000";
    println!("\nStarting development server at http://{addr}/");
    println!("Quit the server with CONTROL-C.\n");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

---

## Running the server and testing with curl

Start the server:

```bash
cargo run
```

You should see:

```
Reversed blog:detail -> /blog/5/
Reversed blog:archive -> /blog/2024/hello-world/

Starting development server at http://127.0.0.1:8000/
Quit the server with CONTROL-C.
```

Now test it with `curl` in another terminal:

```bash
# Home page (200 OK, HTML)
curl http://127.0.0.1:8000/
# <h1>Welcome to django-rs</h1>...

# Blog index
curl http://127.0.0.1:8000/blog/
# <h1>Blog</h1><p>All posts will appear here.</p>

# Blog detail with int path converter
curl http://127.0.0.1:8000/blog/42/
# <h1>Blog Post</h1><p>You are reading post 42.</p>

# Blog archive with int + slug path converters
curl http://127.0.0.1:8000/blog/2024/hello-world/
# <h1>Blog Archive</h1><p>Post: hello-world (2024)</p>

# API endpoint (JSON response)
curl http://127.0.0.1:8000/api/posts/
# {"posts":[{"id":1,"title":"First Post"},{"id":2,"title":"Second Post"}]}

# Non-existent URL (404)
curl http://127.0.0.1:8000/nonexistent/
# <h1>404 Not Found</h1>...
```

You can also check headers with the `-i` flag:

```bash
curl -i http://127.0.0.1:8000/api/posts/
# HTTP/1.1 200 OK
# content-type: application/json
# ...
```

---

## Understanding the request/response cycle

Now that you have a running server, let us trace through what happens when a request arrives:

1. **Axum receives the HTTP request** from the client and passes it to our fallback handler.

2. **We convert it to a django-rs `HttpRequest`** using `HttpRequest::from_axum()`, which extracts the method, path, headers, query string, body, and other metadata.

3. **The URL resolver matches the path** by calling `resolver.resolve(&path)`. The resolver walks through the registered URL patterns and tries to match the path against each one, extracting any path converter values along the way.

4. **On a match**, we get a `ResolverMatch` containing:
   - `func` -- the matched handler function
   - `kwargs` -- captured keyword arguments (e.g., `{"id": "42"}`)
   - `url_name` -- the name of the matched pattern (e.g., `"detail"`)
   - `namespaces` -- the namespace chain (e.g., `["blog"]`)

5. **We attach the `ResolverMatch` to the request** with `set_resolver_match()`, so the view function can access captured parameters.

6. **We call the handler** with the request and get back an `HttpResponse`.

7. **The response is sent to the client** through Axum.

This cycle is the same for every request, whether it returns HTML, JSON, or a redirect.

---

## Comparison with Django

If you are coming from Django, here is how the key concepts map:

| Django (Python) | django-rs (Rust) |
|-----------------|------------------|
| `from django.http import HttpRequest` | `use django_rs_http::HttpRequest;` |
| `from django.http import HttpResponse` | `use django_rs_http::HttpResponse;` |
| `from django.http import JsonResponse` | `use django_rs_http::JsonResponse;` |
| `from django.urls import path` | `use django_rs_http::urls::pattern::path;` |
| `from django.urls import include` | `use django_rs_http::urls::resolver::include;` |
| `from django.urls import reverse` | `use django_rs_http::urls::reverse::reverse;` |
| `path('articles/<int:year>/', view)` | `path("articles/<int:year>/", handler, name)` |
| `HttpResponse("Hello")` | `HttpResponse::ok("Hello")` |
| `JsonResponse({"key": "value"})` | `JsonResponse::new(&json!({"key": "value"}))` |
| `HttpResponseRedirect('/url/')` | `HttpResponseRedirect::new("/url/")` |
| `reverse('app:name', kwargs={...})` | `reverse("app:name", &[], &kwargs, &resolver)` |

The main structural difference is that django-rs handlers are `Arc`-wrapped async closures rather than plain functions. This is because Rust needs explicit ownership semantics for sharing handlers across async tasks and threads.

---

## Summary

In this tutorial you learned how to:

1. **Create a new Rust project** and add django-rs dependencies.
2. **Write view functions** that accept `HttpRequest` and return `HttpResponse`.
3. **Use `HttpResponse` constructors** for common status codes (`ok()`, `not_found()`, `server_error()`, `redirect()`, `json()`).
4. **Use `HttpRequest` methods** to inspect the request (`method()`, `path()`, `query_string()`, `headers()`, `cookies()`).
5. **Define URL patterns** with `path()` using Django-style route syntax.
6. **Use path converters** (`<int:id>`, `<str:name>`, `<slug:slug>`, `<uuid:id>`) to capture typed URL segments.
7. **Organize URLs with `include()`** and namespaces for modular applications.
8. **Set up an Axum server** integrated with the django-rs URL resolver.
9. **Run the server** with `cargo run` and test endpoints with `curl`.

---

Next up: **[Tutorial 2: Models and the Admin Panel](./02-models-and-admin.md)** -- where you will define database models, run migrations, and use the built-in admin interface to manage your data.
