# Tutorial 3: Views and Templates

In [Tutorial 2](./02-models-and-admin.md) you defined models for a blog application and registered them with the admin panel. In this tutorial you will write the views that query data and render it as HTML using the django-rs template engine. By the end, you will have a working set of pages -- a post index, individual post detail pages, and static informational views -- all wired together through URL routing and processed through a middleware pipeline.

This tutorial covers:

- Function-based views and class-based views
- URL routing with path converters and namespaces
- Reverse URL resolution
- The template engine: variables, filters, tags, and inheritance
- Context processors
- The middleware pipeline
- Handling 404 errors

---

## Prerequisites

Make sure you have completed [Tutorial 1: Getting Started](./01-getting-started.md) and [Tutorial 2: Models and the Admin Panel](./02-models-and-admin.md). Your `Cargo.toml` should include at minimum:

```toml
[package]
name = "myblog"
version = "0.1.0"
edition = "2021"

[dependencies]
django-rs-http = { path = "../crates/django-rs-http" }
django-rs-views = { path = "../crates/django-rs-views" }
django-rs-template = { path = "../crates/django-rs-template" }
django-rs-core = { path = "../crates/django-rs-core" }
axum = "0.8"
tokio = { version = "1", features = ["full"] }
http = "1"
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

---

## Part 1: Function-based views

The simplest way to define a view is as a closure or function that takes an `HttpRequest` and returns a future resolving to an `HttpResponse`. The framework defines a `ViewFunction` type alias for this:

```rust
use django_rs_views::views::function::ViewFunction;
use django_rs_http::{HttpRequest, HttpResponse};

let index_view: ViewFunction = Box::new(|_req: HttpRequest| {
    Box::pin(async {
        HttpResponse::ok("<h1>Welcome to my blog</h1>")
    })
});
```

`ViewFunction` is defined as:

```rust
pub type ViewFunction = Box<
    dyn Fn(HttpRequest) -> Pin<Box<dyn Future<Output = HttpResponse> + Send>> + Send + Sync
>;
```

Every view function is `Send + Sync` because django-rs is async from the ground up. The function returns a pinned, boxed future so it can be stored in URL pattern tables and called from the async router.

### Accessing request data

The `HttpRequest` object gives you access to the HTTP method, path, query parameters, headers, and body:

```rust
let search_view: ViewFunction = Box::new(|req: HttpRequest| {
    Box::pin(async move {
        let query = req.get().get("q").cloned().unwrap_or_default();
        let method = req.method().to_string();
        let path = req.path().to_string();

        HttpResponse::ok(format!(
            "Method: {method}, Path: {path}, Query: {query}"
        ))
    })
});
```

A request to `/search/?q=rust` would produce:

```
Method: GET, Path: /search/, Query: rust
```

### Response types

`HttpResponse` provides factory methods for common status codes:

```rust
HttpResponse::ok("200 body")              // 200 OK
HttpResponse::not_found("Page not found") // 404 Not Found
HttpResponse::forbidden("Access denied")  // 403 Forbidden
HttpResponse::bad_request("Invalid data") // 400 Bad Request
HttpResponse::server_error("Oops")        // 500 Internal Server Error
HttpResponse::not_allowed(&["GET","POST"])// 405 Method Not Allowed
```

For redirects, use the dedicated redirect types:

```rust
use django_rs_http::{HttpResponseRedirect, HttpResponsePermanentRedirect};

HttpResponseRedirect::new("/new-location/")       // 302 Found
HttpResponsePermanentRedirect::new("/moved-here/") // 301 Moved Permanently
```

For JSON responses, use `JsonResponse`:

```rust
use django_rs_http::JsonResponse;

let data = serde_json::json!({
    "status": "ok",
    "posts": [{"id": 1, "title": "First Post"}]
});
let response = JsonResponse::new(&data);
```

### Route handlers

URL patterns use `Arc`-wrapped async closures as handlers. This is because Rust needs explicit ownership semantics for sharing handlers across async tasks and threads:

```rust
use django_rs_http::{HttpRequest, HttpResponse, BoxFuture};
use std::sync::Arc;

let index_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
    Arc::new(|request: HttpRequest| {
        Box::pin(async move {
            HttpResponse::ok("<h1>Welcome to my blog</h1>")
        })
    });
```

The `Arc` wrapper allows the handler to be cloned into multiple URL patterns and shared across threads in the async runtime.

### View decorators

django-rs provides decorator functions that wrap a view with additional behavior. These mirror Django's `@require_GET`, `@require_POST`, `@login_required`, and `@permission_required` decorators.

**Restricting HTTP methods:**

```rust
use django_rs_views::views::function::{
    require_get, require_post, require_http_methods, ViewFunction,
};

let my_view: ViewFunction = Box::new(|_req| {
    Box::pin(async { HttpResponse::ok("Hello") })
});

// Only allow GET and HEAD requests (returns 405 for anything else)
let get_only = require_get(my_view);

// Only allow POST requests
let post_only = require_post(another_view);

// Allow specific methods
let limited = require_http_methods(&["GET", "POST"], some_view);
```

**Requiring authentication:**

```rust
use django_rs_views::views::function::{login_required, login_required_redirect};

// Returns 403 Forbidden for unauthenticated users
let protected = login_required(my_view);

// Redirects unauthenticated users to a login page with a ?next= parameter
let protected_with_redirect = login_required_redirect(
    "/accounts/login/",  // login URL
    "next",              // redirect query parameter name
    my_view,
);
```

When an unauthenticated user visits `/dashboard/`, the redirect decorator sends them to `/accounts/login/?next=/dashboard/`. After login, the application can redirect back to the original page.

**Chaining decorators:**

Decorators compose naturally. The outermost decorator runs first:

```rust
// First checks authentication, then checks HTTP method
let view = login_required(require_get(my_view));
```

---

## Part 2: URL routing with parameters

URL routing connects incoming request paths to view functions. django-rs uses `path()` for clean URL syntax with typed placeholders, mirroring Django's `django.urls.path`.

### Defining URL patterns with `path()`

```rust
use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{root, include, URLEntry};
use django_rs_http::{BoxFuture, HttpRequest, HttpResponse};
use std::sync::Arc;

// Define handlers
let index_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
    Arc::new(|_req: HttpRequest| {
        Box::pin(async { HttpResponse::ok("<h1>Blog Index</h1>") })
    });

let detail_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
    Arc::new(|req: HttpRequest| {
        Box::pin(async move {
            let post_id = req
                .resolver_match()
                .and_then(|m| m.kwargs.get("id"))
                .map(String::as_str)
                .unwrap_or("unknown");
            HttpResponse::ok(format!("<h1>Post {post_id}</h1>"))
        })
    });

let slug_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
    Arc::new(|req: HttpRequest| {
        Box::pin(async move {
            let slug = req
                .resolver_match()
                .and_then(|m| m.kwargs.get("slug"))
                .map(String::as_str)
                .unwrap_or("unknown");
            HttpResponse::ok(format!("<h1>Post: {slug}</h1>"))
        })
    });

// Create URL patterns
let patterns = vec![
    URLEntry::Pattern(path("", index_handler, Some("index")).unwrap()),
    URLEntry::Pattern(
        path("posts/<int:id>/", detail_handler, Some("post-detail")).unwrap()
    ),
    URLEntry::Pattern(
        path("posts/<slug:slug>/", slug_handler, Some("post-by-slug")).unwrap()
    ),
];

let resolver = root(patterns).unwrap();
```

Each `path()` call takes:
1. A route string with optional `<type:name>` placeholders
2. A handler function (`Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync>`)
3. An optional name for reverse URL lookup

### Path converters

Path converters define the type and format of URL parameters:

| Converter | Syntax | Matches | Example |
|-----------|--------|---------|---------|
| `int` | `<int:name>` | One or more digits | `42`, `2024` |
| `str` | `<str:name>` | Any non-empty string without `/` | `hello`, `my-page` |
| `slug` | `<slug:name>` | Letters, digits, hyphens, underscores | `my-first-post` |
| `uuid` | `<uuid:name>` | A standard UUID | `550e8400-e29b-...` |
| `path` | `<path:name>` | Any non-empty string, including `/` | `docs/2024/readme.md` |

If you omit the type (e.g., `<name>` instead of `<str:name>`), it defaults to `str`.

### Multiple converters in one route

You can chain multiple converters in a single pattern:

```rust
let archive_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
    Arc::new(|req: HttpRequest| {
        Box::pin(async move {
            let year = req.resolver_match()
                .and_then(|m| m.kwargs.get("year").cloned())
                .unwrap_or_default();
            let month = req.resolver_match()
                .and_then(|m| m.kwargs.get("month").cloned())
                .unwrap_or_default();
            HttpResponse::ok(format!("Archive: {year}/{month}"))
        })
    });

let pattern = path(
    "archive/<int:year>/<int:month>/",
    archive_handler,
    Some("post-archive"),
).unwrap();

let resolver = root(vec![URLEntry::Pattern(pattern)]).unwrap();

let matched = resolver.resolve("archive/2024/12/").unwrap();
assert_eq!(matched.kwargs.get("year").unwrap(), "2024");
assert_eq!(matched.kwargs.get("month").unwrap(), "12");
```

### Regex-based patterns with `re_path()`

For more complex matching, use `re_path()` with a raw regex:

```rust
use django_rs_http::urls::pattern::re_path;

// Match exactly 4-digit years
let pattern = re_path(
    r"^articles/(?P<year>[0-9]{4})/$",
    year_handler,
    Some("article-year"),
).unwrap();
```

Named groups (`(?P<name>...)`) become keyword arguments, just like in Django.

### Resolving URLs

The resolver matches a path and returns a `ResolverMatch` with the handler, captured arguments, and metadata:

```rust
let matched = resolver.resolve("posts/42/").unwrap();
assert_eq!(matched.url_name.as_deref(), Some("post-detail"));
assert_eq!(matched.kwargs.get("id").unwrap(), "42");

// Int converter rejects non-numeric values
assert!(resolver.resolve("posts/abc/").is_err());
```

If no pattern matches, `resolve()` returns a `DjangoError::NotFound`.

---

## Part 3: URL namespaces and includes

As your application grows, you will want to organize URLs into logical groups. django-rs provides `include()` for nesting URL patterns under a prefix, just like Django's `include()`.

### Basic include with namespaces

```rust
use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{root, include, URLEntry};
use django_rs_http::{BoxFuture, HttpRequest, HttpResponse};
use std::sync::Arc;

let handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
    Arc::new(|_req: HttpRequest| {
        Box::pin(async { HttpResponse::ok("ok") })
    });

// Blog app URL patterns
let blog_patterns = vec![
    URLEntry::Pattern(path("", handler.clone(), Some("post-list")).unwrap()),
    URLEntry::Pattern(
        path("<int:id>/", handler.clone(), Some("post-detail")).unwrap()
    ),
    URLEntry::Pattern(
        path("<slug:slug>/", handler.clone(), Some("post-by-slug")).unwrap()
    ),
];

// Include them under the "blog/" prefix with a namespace
let patterns = vec![
    URLEntry::Pattern(path("", handler.clone(), Some("home")).unwrap()),
    URLEntry::Resolver(
        include("blog/", blog_patterns, Some("blog"), Some("blog")).unwrap()
    ),
];

let resolver = root(patterns).unwrap();

// Resolve "blog/" -> blog:post-list
let matched = resolver.resolve("blog/").unwrap();
assert_eq!(matched.url_name.as_deref(), Some("post-list"));
assert_eq!(matched.namespaces, vec!["blog"]);
assert_eq!(matched.view_name(), "blog:post-list");

// Resolve "blog/42/" -> blog:post-detail with id=42
let matched = resolver.resolve("blog/42/").unwrap();
assert_eq!(matched.view_name(), "blog:post-detail");
assert_eq!(matched.kwargs.get("id").unwrap(), "42");
```

The `include()` function takes four arguments:

| Argument | Description |
|----------|-------------|
| `prefix` | The URL prefix to match (e.g., `"blog/"`) |
| `patterns` | A `Vec<URLEntry>` of child patterns |
| `namespace` | An optional instance namespace for reverse resolution |
| `app_name` | An optional application namespace |

### Organizing URLs by app

A typical project organizes URL patterns into separate functions, one per "app." This is the recommended pattern:

```rust
// -- blog/urls.rs --
fn blog_urls() -> Vec<URLEntry> {
    let handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|_req: HttpRequest| {
            Box::pin(async { HttpResponse::ok("blog view") })
        });

    vec![
        URLEntry::Pattern(path("", handler.clone(), Some("post-list")).unwrap()),
        URLEntry::Pattern(
            path("<int:id>/", handler.clone(), Some("post-detail")).unwrap()
        ),
        URLEntry::Pattern(
            path("<int:year>/<slug:slug>/", handler, Some("post-archive")).unwrap()
        ),
    ]
}

// -- accounts/urls.rs --
fn accounts_urls() -> Vec<URLEntry> {
    let handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|_req: HttpRequest| {
            Box::pin(async { HttpResponse::ok("accounts view") })
        });

    vec![
        URLEntry::Pattern(path("login/", handler.clone(), Some("login")).unwrap()),
        URLEntry::Pattern(path("logout/", handler.clone(), Some("logout")).unwrap()),
        URLEntry::Pattern(
            path("profile/<str:username>/", handler, Some("profile")).unwrap()
        ),
    ]
}

// -- mysite/urls.rs (root URL configuration) --
fn url_config() -> URLResolver {
    let index_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|_req: HttpRequest| {
            Box::pin(async { HttpResponse::ok("<h1>Home</h1>") })
        });

    let patterns = vec![
        URLEntry::Pattern(path("", index_handler, Some("home")).unwrap()),
        URLEntry::Resolver(
            include("blog/", blog_urls(), Some("blog"), Some("blog")).unwrap()
        ),
        URLEntry::Resolver(
            include(
                "accounts/",
                accounts_urls(),
                Some("accounts"),
                Some("accounts"),
            ).unwrap(),
        ),
    ];

    root(patterns).unwrap()
}
```

### Deeply nested includes

You can nest includes as deeply as needed. Namespaces are accumulated into a list:

```rust
let info_patterns = vec![
    URLEntry::Pattern(path("info/", handler.clone(), Some("info")).unwrap()),
];

let detail_patterns = vec![
    URLEntry::Resolver(
        include("<int:id>/", info_patterns, Some("detail"), Some("detail")).unwrap(),
    ),
];

let patterns = vec![
    URLEntry::Resolver(
        include("users/", detail_patterns, Some("users"), Some("users")).unwrap()
    ),
];

let resolver = root(patterns).unwrap();

// Resolves: /users/42/info/
let matched = resolver.resolve("users/42/info/").unwrap();
assert_eq!(matched.url_name.as_deref(), Some("info"));
assert_eq!(matched.kwargs.get("id").unwrap(), "42");
assert_eq!(matched.namespaces, vec!["users", "detail"]);
assert_eq!(matched.view_name(), "users:detail:info");
```

---

## Part 4: Reverse URL resolution

Hardcoding URLs in your application is fragile. If you rename a route or change its path, every hardcoded reference breaks. django-rs solves this with `reverse()`, just like Django.

### Basic reverse resolution

```rust
use django_rs_http::urls::reverse::reverse;
use std::collections::HashMap;

// Build the resolver (from the previous section)
let resolver = url_config();

// Reverse a simple URL (no parameters)
let url = reverse("home", &[], &HashMap::new(), &resolver).unwrap();
assert_eq!(url, "/");

// Reverse with keyword arguments
let mut kwargs = HashMap::new();
kwargs.insert("id", "42");
let url = reverse("blog:post-detail", &[], &kwargs, &resolver).unwrap();
assert_eq!(url, "/blog/42/");

// Reverse with multiple kwargs
let mut kwargs = HashMap::new();
kwargs.insert("year", "2024");
kwargs.insert("slug", "hello-world");
let url = reverse("blog:post-archive", &[], &kwargs, &resolver).unwrap();
assert_eq!(url, "/blog/2024/hello-world/");

// Reverse with positional arguments (instead of kwargs)
let url = reverse(
    "blog:post-archive",
    &["2024", "hello-world"],
    &HashMap::new(),
    &resolver,
).unwrap();
assert_eq!(url, "/blog/2024/hello-world/");

// Reversing a non-existent name returns an error
let result = reverse("nonexistent", &[], &HashMap::new(), &resolver);
assert!(result.is_err());
```

### Using reverse() for redirects

A common pattern is using `reverse()` to build redirect URLs:

```rust
use django_rs_http::urls::reverse::reverse;
use django_rs_http::{HttpResponseRedirect, HttpResponse};

fn redirect_to_post(resolver: &URLResolver) -> HttpResponse {
    let mut kwargs = HashMap::new();
    kwargs.insert("id", "1");
    let url = reverse("blog:post-detail", &[], &kwargs, resolver).unwrap();
    HttpResponseRedirect::new(&url)
}
```

### Using reverse() in views

A practical view that redirects after a successful action:

```rust
let create_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> = {
    let resolver = Arc::clone(&resolver);
    Arc::new(move |request: HttpRequest| {
        let resolver = Arc::clone(&resolver);
        Box::pin(async move {
            if request.method() == &http::Method::POST {
                // ... process form data, create the post ...
                let new_post_id = "5";

                let mut kwargs = HashMap::new();
                kwargs.insert("id", new_post_id);
                let url = reverse("blog:post-detail", &[], &kwargs, &resolver).unwrap();
                HttpResponseRedirect::new(&url)
            } else {
                HttpResponse::ok("<form method='post'>...</form>")
            }
        })
    })
};
```

### ResolverMatch

When `resolve()` succeeds, it returns a `ResolverMatch` struct with useful metadata about the matched route:

| Field | Type | Description |
|-------|------|-------------|
| `func` | `RouteHandler` | The matched handler function |
| `args` | `Vec<String>` | Positional arguments (for `re_path` patterns) |
| `kwargs` | `HashMap<String, String>` | Named keyword arguments captured from the URL |
| `url_name` | `Option<String>` | The name of the matched URL pattern |
| `app_names` | `Vec<String>` | Application namespaces in the resolution chain |
| `namespaces` | `Vec<String>` | Instance namespaces in the resolution chain |
| `route` | `String` | The matched route template string |

The `view_name()` method returns the fully-qualified name with namespaces joined by colons:

```rust
// For a match with namespaces=["api", "v1"] and url_name=Some("user-detail")
// view_name() returns "api:v1:user-detail"
```

---

## Part 5: The template engine

django-rs includes a full template engine that implements Django Template Language (DTL) syntax. The engine lives in the `django-rs-template` crate and is used by views to render HTML responses.

### Setting up the engine

```rust
use django_rs_template::engine::Engine;
use django_rs_template::context::{Context, ContextValue};

let mut engine = Engine::new();

// Option 1: Add templates from strings (useful for testing)
engine.add_string_template("hello.html", "Hello, {{ name }}!");

// Option 2: Load templates from the filesystem
use std::path::PathBuf;
engine.set_dirs(vec![PathBuf::from("templates/")]);
```

In a typical project, you configure the engine with a `templates/` directory and the engine loads `.html` files from there.

### Rendering a template

```rust
let mut ctx = Context::new();
ctx.set("name", ContextValue::from("World"));

let output = engine.render_to_string("hello.html", &mut ctx).unwrap();
assert_eq!(output, "Hello, World!");
```

### A practical blog example

Here is a complete example that renders a blog post listing using the template engine:

```rust
use django_rs_template::engine::Engine;
use django_rs_template::context::{Context, ContextValue};
use std::collections::HashMap;

let engine = Engine::new();

engine.add_string_template("post_list.html", r#"
<h1>{{ title }}</h1>
{% for post in posts %}
  <article>
    <h2>{{ post.title }}</h2>
    <p>{{ post.content|truncatechars:100 }}</p>
    <small>By {{ post.author }} on {{ post.created_at|date:"%b %d, %Y" }}</small>
  </article>
{% empty %}
  <p>No posts yet.</p>
{% endfor %}
"#);

// Build the context
let mut ctx = Context::new();
ctx.set("title", ContextValue::from("My Blog"));

let mut post1 = HashMap::new();
post1.insert("title".to_string(), ContextValue::from("Getting Started with Rust"));
post1.insert("content".to_string(), ContextValue::from(
    "Rust is a systems programming language focused on safety, speed, and concurrency."
));
post1.insert("author".to_string(), ContextValue::from("Alice"));
post1.insert("created_at".to_string(), ContextValue::from("2025-06-01"));

let mut post2 = HashMap::new();
post2.insert("title".to_string(), ContextValue::from("Understanding Ownership"));
post2.insert("content".to_string(), ContextValue::from(
    "Ownership is Rust's most distinctive feature. It enables memory safety without garbage collection."
));
post2.insert("author".to_string(), ContextValue::from("Bob"));
post2.insert("created_at".to_string(), ContextValue::from("2025-07-15"));

ctx.set("posts", ContextValue::List(vec![
    ContextValue::Dict(post1),
    ContextValue::Dict(post2),
]));

let html = engine.render_to_string("post_list.html", &mut ctx).unwrap();
```

### Variables

Variables are inserted with double curly braces:

```html
<h1>{{ title }}</h1>
<p>By {{ author }}</p>
```

**Dot notation** resolves nested values. Given this context:

```rust
let mut user = HashMap::new();
user.insert("name".to_string(), ContextValue::from("Alice"));
user.insert("age".to_string(), ContextValue::from(30i32));

let mut address = HashMap::new();
address.insert("city".to_string(), ContextValue::from("Portland"));
user.insert("address".to_string(), ContextValue::Dict(address));

ctx.set("user", ContextValue::Dict(user));
```

You can access nested values in the template:

```html
<p>{{ user.name }}, age {{ user.age }}</p>
<p>Lives in {{ user.address.city }}</p>
```

List items are accessed by index:

```html
{{ items.0 }}  {# first item #}
{{ items.1 }}  {# second item #}
```

Missing variables render as empty strings by default.

### Context values

The `ContextValue` enum represents all types that can appear in a template context:

```rust
ContextValue::String("hello".to_string())            // text
ContextValue::Integer(42)                             // integer
ContextValue::Float(3.14)                             // float
ContextValue::Bool(true)                              // boolean (renders as "True"/"False")
ContextValue::List(vec![...])                         // list
ContextValue::Dict(HashMap::new())                    // dictionary
ContextValue::None                                    // None (renders as empty string)
ContextValue::SafeString("<b>bold</b>".to_string())   // HTML-safe string (no escaping)
```

Convenient `From` implementations let you construct values naturally:

```rust
ctx.set("name", ContextValue::from("Django"));       // from &str
ctx.set("count", ContextValue::from(42i32));          // from integer
ctx.set("active", ContextValue::from(true));          // from bool
ctx.set("score", ContextValue::from(98.5f64));        // from float
ctx.set("missing", ContextValue::from(Option::<i32>::None)); // from Option
```

You can also convert from `serde_json::Value`, which is what generic views use internally:

```rust
let json = serde_json::json!({
    "title": "My Post",
    "tags": ["rust", "web"],
    "published": true,
});
ctx.set("post", ContextValue::from(json));
```

### Auto-escaping

By default, the engine escapes HTML special characters (`<`, `>`, `&`, `"`, `'`) in variable output. This prevents cross-site scripting (XSS) attacks:

```rust
ctx.set("content", ContextValue::from("<script>alert('xss')</script>"));
// Renders as: &lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;
```

To output raw HTML, mark the value as safe or use the `safe` filter:

```rust
// In Rust:
ctx.set("html", ContextValue::SafeString("<b>bold</b>".to_string()));

// Or in the template:
// {{ content|safe }}
```

You can also control auto-escaping per block in templates:

```html
{% autoescape off %}
  {{ raw_html }}
{% endautoescape %}
```

### Context scoping

The `Context` object uses a stack of scopes. The `push()` and `pop()` methods create and destroy scopes, and variable lookups search from the top of the stack downward. This is how `{% for %}` and `{% with %}` tags provide block-local variables without polluting the outer scope:

```rust
let mut ctx = Context::new();
ctx.set("x", ContextValue::from(1i32));

ctx.push();
ctx.set("x", ContextValue::from(2i32)); // shadows outer x
assert_eq!(ctx.get("x").unwrap().to_display_string(), "2");

ctx.pop();
assert_eq!(ctx.get("x").unwrap().to_display_string(), "1");
```

---

## Part 6: Template features

### Filters

Filters transform variable values using the pipe (`|`) syntax. django-rs ships with over 40 built-in filters organized into several categories.

```html
{{ name|upper }}
{{ text|truncatechars:50 }}
{{ items|join:", " }}
```

Filters can be chained. They execute left to right:

```html
{{ name|upper|truncatechars:5 }}
```

Given `name = "hello world"`, this first produces `"HELLO WORLD"`, then truncates to `"HE..."`.

#### Text filters

| Filter | Example | Description |
|--------|---------|-------------|
| `lower` | `{{ name\|lower }}` | Converts to lowercase |
| `upper` | `{{ name\|upper }}` | Converts to uppercase |
| `capitalize` | `{{ name\|capitalize }}` | Capitalizes first letter |
| `title` | `{{ name\|title }}` | Title-cases the string |
| `truncatechars` | `{{ text\|truncatechars:50 }}` | Truncates to N characters with "..." |
| `truncatewords` | `{{ text\|truncatewords:10 }}` | Truncates to N words with "..." |
| `slugify` | `{{ title\|slugify }}` | Converts to URL-friendly slug |
| `wordcount` | `{{ text\|wordcount }}` | Counts the words |
| `ljust` | `{{ val\|ljust:20 }}` | Left-justifies in a field of width N |
| `rjust` | `{{ val\|rjust:20 }}` | Right-justifies in a field of width N |
| `center` | `{{ val\|center:20 }}` | Centers in a field of width N |

#### List and collection filters

| Filter | Example | Description |
|--------|---------|-------------|
| `first` | `{{ items\|first }}` | First item in a list |
| `last` | `{{ items\|last }}` | Last item in a list |
| `join` | `{{ items\|join:", " }}` | Joins list items with separator |
| `length` | `{{ items\|length }}` | Length of list or string |
| `random` | `{{ items\|random }}` | Random item from a list |
| `slice` | `{{ items\|slice:":3" }}` | Slices a list |
| `dictsort` | `{{ items\|dictsort:"name" }}` | Sorts list of dicts by key |

#### HTML filters

| Filter | Example | Description |
|--------|---------|-------------|
| `safe` | `{{ html\|safe }}` | Marks output as HTML-safe (no escaping) |
| `escape` | `{{ text\|escape }}` | Forces HTML escaping |
| `striptags` | `{{ html\|striptags }}` | Removes HTML tags |
| `linebreaks` | `{{ text\|linebreaks }}` | Converts newlines to `<p>` and `<br>` |
| `urlize` | `{{ text\|urlize }}` | Converts URLs to clickable links |

#### Formatting filters

| Filter | Example | Description |
|--------|---------|-------------|
| `date` | `{{ date\|date:"%Y-%m-%d" }}` | Formats a date |
| `time` | `{{ time\|time:"%H:%M" }}` | Formats a time |
| `default` | `{{ val\|default:"N/A" }}` | Provides a fallback value |
| `default_if_none` | `{{ val\|default_if_none:"N/A" }}` | Fallback only for None |
| `floatformat` | `{{ num\|floatformat:2 }}` | Formats a float |
| `filesizeformat` | `{{ bytes\|filesizeformat }}` | Human-readable file size |

#### Math filters

| Filter | Example | Description |
|--------|---------|-------------|
| `add` | `{{ value\|add:5 }}` | Adds a number |
| `subtract` | `{{ value\|subtract:3 }}` | Subtracts a number |
| `multiply` | `{{ value\|multiply:2 }}` | Multiplies by a number |
| `divide` | `{{ value\|divide:4 }}` | Divides by a number |
| `divisibleby` | `{{ value\|divisibleby:3 }}` | Returns True if divisible |

#### Logic filters

| Filter | Example | Description |
|--------|---------|-------------|
| `yesno` | `{{ val\|yesno:"yes,no,maybe" }}` | Maps True/False/None to words |
| `pluralize` | `{{ count\|pluralize }}` | Returns "s" if count != 1 |

### Tags

Tags provide control flow, template composition, and other logic. They use `{% tag %}` syntax.

#### Conditional logic: `if`, `elif`, `else`

```html
{% if user.is_authenticated %}
    <p>Welcome back, {{ user.name }}!</p>
{% elif user.is_guest %}
    <p>Welcome, guest!</p>
{% else %}
    <p>Please log in.</p>
{% endif %}
```

The `if` tag supports comparison operators and boolean logic:

```html
{% if post.comment_count > 0 %}
    {{ post.comment_count }} comment{{ post.comment_count|pluralize }}
{% endif %}

{% if x == 1 %}one{% elif x == 2 %}two{% else %}other{% endif %}
```

#### Loops: `for` and `empty`

```html
{% for post in posts %}
    <article>
        <h2>{{ post.title }}</h2>
        <p>{{ post.body|truncatewords:30 }}</p>
    </article>
{% empty %}
    <p>No posts have been published yet.</p>
{% endfor %}
```

The `{% empty %}` clause renders when the list is empty or undefined.

**Loop variables** -- inside a `{% for %}` block, `forloop` provides iteration metadata:

| Variable | Description |
|----------|-------------|
| `forloop.counter` | 1-based iteration count |
| `forloop.counter0` | 0-based iteration count |
| `forloop.revcounter` | Iterations remaining (1-based) |
| `forloop.first` | `True` on the first iteration |
| `forloop.last` | `True` on the last iteration |
| `forloop.parentloop` | The parent loop's `forloop` (for nested loops) |

Example:

```html
<ol>
{% for item in items %}
    <li class="{% if forloop.first %}first{% endif %}{% if forloop.last %} last{% endif %}">
        {{ forloop.counter }}. {{ item }}
    </li>
{% endfor %}
</ol>
```

#### Variable scoping: `with`

```html
{% with total=items|length %}
    <p>There are {{ total }} items.</p>
{% endwith %}
```

The variable `total` is only available inside the `{% with %}...{% endwith %}` block.

#### Comments

```html
{# This is a single-line comment and will not appear in the output #}

{% comment %}
    This is a multi-line comment.
    None of this content will be rendered.
{% endcomment %}
```

#### Other built-in tags

| Tag | Example | Description |
|-----|---------|-------------|
| `csrf_token` | `{% csrf_token %}` | Outputs a hidden CSRF form field |
| `spaceless` | `{% spaceless %}...{% endspaceless %}` | Removes whitespace between HTML tags |
| `verbatim` | `{% verbatim %}{{ raw }}{% endverbatim %}` | Prevents template parsing inside the block |
| `now` | `{% now %}` | Outputs the current date/time |
| `firstof` | `{% firstof a b c %}` | Outputs the first truthy variable |
| `cycle` | `{% cycle "odd" "even" %}` | Cycles through values in a loop |
| `load` | `{% load static %}` | Loads a template tag library |
| `static` | `{% static "css/style.css" %}` | Generates URL for a static file |
| `include` | `{% include "partial.html" %}` | Includes another template inline |
| `autoescape` | `{% autoescape off %}...{% endautoescape %}` | Controls auto-escaping |

### Template inheritance with `extends` and `block`

Template inheritance is the most powerful feature of Django's template system, and django-rs supports it fully. It lets you build a base "skeleton" template that contains the common structure of your site, then override specific sections in child templates.

#### Defining a base template

```html
<!-- templates/base.html -->
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <title>{% block title %}My Blog{% endblock %}</title>
    {% block extra_head %}{% endblock %}
</head>
<body>
    <nav>
        <a href="/">Home</a>
        <a href="/blog/">Blog</a>
        <a href="/about/">About</a>
    </nav>

    <main>
        {% block content %}
            <p>Default content goes here.</p>
        {% endblock %}
    </main>

    <footer>
        {% block footer %}
            <p>&copy; 2026 My Blog</p>
        {% endblock %}
    </footer>
</body>
</html>
```

Each `{% block name %}...{% endblock %}` defines a section that child templates can override. The content inside the block is the default -- it renders if no child overrides it.

#### Extending the base template

```html
<!-- templates/post_list.html -->
{% extends "base.html" %}

{% block title %}All Posts - My Blog{% endblock %}

{% block content %}
<h1>Blog Posts</h1>
{% for post in object_list %}
    <article>
        <h2><a href="/blog/{{ post.id }}/">{{ post.title }}</a></h2>
        <p>{{ post.body|truncatewords:30 }}</p>
        <time>{{ post.created_at|date:"%B %d, %Y" }}</time>
    </article>
{% empty %}
    <p>No posts yet.</p>
{% endfor %}

{% if is_paginated %}
<nav>
    {% if page_obj.has_previous %}
        <a href="?page={{ page_obj.number|add:-1 }}">Previous</a>
    {% endif %}
    Page {{ page_obj.number }}
    {% if page_obj.has_next %}
        <a href="?page={{ page_obj.number|add:1 }}">Next</a>
    {% endif %}
</nav>
{% endif %}
{% endblock %}
```

The `{% extends "base.html" %}` declaration must be the first tag in the template. Only blocks defined in the parent are overridden -- everything else comes from the parent.

#### Using `block.super`

To include the parent block's content along with your additions, use `{{ block.super }}`:

```html
<!-- templates/post_detail.html -->
{% extends "base.html" %}

{% block title %}{{ object.title }} - {{ block.super }}{% endblock %}

{% block content %}
<article>
    <h1>{{ object.title }}</h1>
    <div class="meta">
        <span>By {{ object.author }}</span>
        <time>{{ object.created_at|date:"%B %d, %Y" }}</time>
    </div>
    <div class="content">{{ object.body|safe }}</div>
</article>
{% endblock %}
```

If the base template's title block contains "My Blog", this renders as "My Post Title - My Blog".

#### Multi-level inheritance

Inheritance can go multiple levels deep:

```html
<!-- templates/base.html -->
<html>{% block content %}base{% endblock %}</html>

<!-- templates/blog_base.html -->
{% extends "base.html" %}
{% block content %}
    <div class="blog-container">
        {% block blog_content %}blog default{% endblock %}
    </div>
{% endblock %}

<!-- templates/post_detail.html -->
{% extends "blog_base.html" %}
{% block blog_content %}
    <h1>{{ post.title }}</h1>
{% endblock %}
```

#### Including partials

The `{% include %}` tag inserts the rendered content of another template inline:

```html
{% include "partials/sidebar.html" %}
```

You can pass variables to the included template:

```html
{% include "partials/post_card.html" with post=featured_post %}
```

The `only` keyword restricts the included template to only the explicitly passed variables:

```html
{% include "partials/post_card.html" with post=featured_post only %}
```

#### A complete inheritance example in Rust

Here is how template inheritance works end-to-end from the Rust side:

```rust
use django_rs_template::engine::Engine;
use django_rs_template::context::{Context, ContextValue};
use std::collections::HashMap;

let engine = Engine::new();

engine.add_string_template(
    "base.html",
    "<!DOCTYPE html><html><body>\
     {% block content %}default{% endblock %}\
     </body></html>"
);

engine.add_string_template(
    "post_detail.html",
    "{% extends \"base.html\" %}\
     {% block content %}\
     <h1>{{ post.title }}</h1>\
     <p>{{ post.body }}</p>\
     {% endblock %}"
);

let mut ctx = Context::new();
let mut post = HashMap::new();
post.insert("title".to_string(), ContextValue::from("Hello World"));
post.insert("body".to_string(), ContextValue::from("This is my first post."));
ctx.set("post", ContextValue::Dict(post));

let html = engine.render_to_string("post_detail.html", &mut ctx).unwrap();
// Produces:
// <!DOCTYPE html><html><body><h1>Hello World</h1><p>This is my first post.</p></body></html>
```

---

## Part 7: Class-based views

For views that follow standard patterns -- rendering a template, listing objects, showing a detail page -- class-based views (CBVs) reduce boilerplate. CBVs are implemented as Rust traits.

### The `View` trait

The base `View` trait provides HTTP method dispatch. Implement the methods you want to handle; everything else returns 405 Method Not Allowed by default:

```rust
use async_trait::async_trait;
use django_rs_views::views::class_based::View;
use django_rs_http::{HttpRequest, HttpResponse};

struct AboutView;

#[async_trait]
impl View for AboutView {
    async fn get(&self, _request: HttpRequest) -> HttpResponse {
        HttpResponse::ok("<h1>About Us</h1><p>We write about Rust.</p>")
    }
}
```

The `dispatch` method routes the request to the correct handler:

```rust
let view = AboutView;
let request = HttpRequest::builder().method(http::Method::GET).build();
let response = view.dispatch(request).await;
// response status: 200 OK

let request = HttpRequest::builder().method(http::Method::DELETE).build();
let response = view.dispatch(request).await;
// response status: 405 Method Not Allowed
```

### Converting CBVs to function views with `as_view()`

URL patterns expect function-based handlers. The `as_view()` method converts a CBV into a `ViewFunction`, just like Django's `MyView.as_view()`:

```rust
let about_handler = AboutView.as_view();
// about_handler is now a ViewFunction that can be used in URL patterns
```

### TemplateView

`TemplateView` renders a template with optional context data. It combines the `View`, `ContextMixin`, and `TemplateResponseMixin` traits:

```rust
use django_rs_views::views::class_based::TemplateView;
use django_rs_template::engine::Engine;
use std::sync::Arc;

let engine = Arc::new(Engine::new());
// Register templates (in production these would be loaded from disk)
engine.add_string_template(
    "about.html",
    "{% extends \"base.html\" %}\
     {% block title %}{{ page_title }}{% endblock %}\
     {% block content %}<h1>{{ page_title }}</h1><p>{{ description }}</p>{% endblock %}"
);

let about_view = TemplateView::new("about.html")
    .with_engine(engine)
    .with_context("page_title", serde_json::json!("About Us"))
    .with_context("description", serde_json::json!("We build web apps with Rust."));

let handler = about_view.as_view();
```

### RedirectView

`RedirectView` handles URL redirects:

```rust
use django_rs_views::views::class_based::RedirectView;

// Temporary redirect (302)
let temp_redirect = RedirectView::new("/new-url/");

// Permanent redirect (301)
let perm_redirect = RedirectView::permanent("/permanent-url/");
```

### ListView

`ListView` displays a list of objects with optional pagination. It is a trait that you implement on your own types:

```rust
use async_trait::async_trait;
use django_rs_views::views::class_based::{View, ContextMixin};
use django_rs_views::views::generic::ListView;
use django_rs_core::DjangoError;
use std::collections::HashMap;

struct PostListView;

impl ContextMixin for PostListView {
    fn get_context_data(
        &self,
        _kwargs: &HashMap<String, String>,
    ) -> HashMap<String, serde_json::Value> {
        let mut context = HashMap::new();
        context.insert("page_title".to_string(), serde_json::json!("All Posts"));
        context
    }
}

#[async_trait]
impl View for PostListView {
    async fn get(&self, request: HttpRequest) -> HttpResponse {
        self.list(request).await
    }
}

#[async_trait]
impl ListView for PostListView {
    fn model_name(&self) -> &str {
        "post"
    }

    fn paginate_by(&self) -> Option<usize> {
        Some(10) // 10 posts per page
    }

    async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError> {
        // In a real app, query the database here
        Ok(vec![
            serde_json::json!({"id": 1, "title": "First Post", "published": true}),
            serde_json::json!({"id": 2, "title": "Second Post", "published": true}),
        ])
    }
}
```

The `ListView` trait automatically:
- Calls `get_queryset()` to retrieve the data
- Paginates the results when `paginate_by()` returns `Some(n)`
- Adds `object_list`, `page_obj`, `paginator`, and `is_paginated` to the template context
- Renders the template named `{model_name}_list.html` (e.g., `post_list.html`)

Pagination is controlled via the `?page=` query parameter. The `page_obj` context variable includes `number`, `has_next`, `has_previous`, `has_other_pages`, `start_index`, and `end_index`.

### DetailView

`DetailView` displays a single object, with automatic 404 handling:

```rust
struct PostDetailView;

#[async_trait]
impl DetailView for PostDetailView {
    fn model_name(&self) -> &str {
        "post"
    }

    async fn get_object(
        &self,
        kwargs: &HashMap<String, String>,
    ) -> Result<serde_json::Value, DjangoError> {
        let id = kwargs.get("pk")
            .ok_or_else(|| DjangoError::NotFound("Missing pk".to_string()))?;
        // Query the database for the post with this ID
        // Return DjangoError::DoesNotExist for a 404 response
        Ok(serde_json::json!({"id": id, "title": "My Post", "body": "Content here"}))
    }
}
```

The `DetailView` renders `{model_name}_detail.html` and adds the retrieved `object` to the template context. If `get_object` returns `DjangoError::NotFound` or `DjangoError::DoesNotExist`, the view automatically returns a 404 response.

### Wiring CBVs to URL patterns

Here is a complete example that connects class-based views to URL patterns:

```rust
use django_rs_views::views::class_based::{TemplateView, RedirectView, View};
use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{root, include, URLEntry};
use django_rs_template::engine::Engine;
use std::sync::Arc;

let engine = Arc::new(Engine::new());
engine.add_string_template("home.html", "<h1>{{ title }}</h1>");

// Convert CBVs to handlers
let home_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> = Arc::new({
    let engine = engine.clone();
    move |req: HttpRequest| -> BoxFuture {
        let engine = engine.clone();
        Box::pin(async move {
            let view = TemplateView::new("home.html")
                .with_engine(engine)
                .with_context("title", serde_json::json!("Welcome"));
            view.dispatch(req).await
        })
    }
});

let old_about_handler = RedirectView::permanent("/about/").as_view();

let urlpatterns = vec![
    URLEntry::Pattern(path("", home_handler, Some("home")).unwrap()),
    URLEntry::Pattern(
        path("old-about/", Arc::new(old_about_handler), Some("old-about")).unwrap()
    ),
];

let resolver = root(urlpatterns).unwrap();
```

### Mixins for CBVs

Class-based views can use mixins for cross-cutting concerns:

```rust
use django_rs_views::views::function::{LoginRequiredMixin, PermissionRequiredMixin};

struct AdminPostView;

impl LoginRequiredMixin for AdminPostView {
    fn login_url(&self) -> &str {
        "/accounts/login/"
    }
    fn redirect_field_name(&self) -> &str {
        "next"
    }
}

impl PermissionRequiredMixin for AdminPostView {
    fn permission_required(&self) -> &str {
        "blog.change_post"
    }
}
```

Call `self.check_login(&request)` or `self.check_permission(&request)` at the start of your view method. If the check fails, it returns `Some(HttpResponse)` with a redirect or 403; if it passes, it returns `None` and you proceed with your view logic.

---

## Part 8: Context processors

Context processors inject variables into every template context automatically, so you do not have to pass them in every view.

### Built-in context processors

| Processor | Variables Added | Description |
|-----------|----------------|-------------|
| `RequestContextProcessor` | `request.path`, `request.method`, `request.is_secure` | Request metadata |
| `CsrfContextProcessor` | `csrf_token` | CSRF protection token |
| `StaticContextProcessor` | `STATIC_URL` | Static file URL prefix |
| `MediaContextProcessor` | `MEDIA_URL` | Media file URL prefix |
| `DebugContextProcessor` | `debug`, `sql_queries` | Debug information |

### Using context processors

```rust
use django_rs_template::context_processors::{
    RequestContextProcessor, CsrfContextProcessor, StaticContextProcessor,
    ContextProcessor,
};
use django_rs_template::context::{Context, ContextValue};
use django_rs_http::HttpRequest;

let request = HttpRequest::builder().path("/blog/").build();

// Apply context processors
let processors: Vec<Box<dyn ContextProcessor>> = vec![
    Box::new(RequestContextProcessor),
    Box::new(CsrfContextProcessor),
    Box::new(StaticContextProcessor::new("/static/")),
];

let mut ctx = Context::new();
for processor in &processors {
    for (key, value) in processor.process(&request) {
        ctx.set(key, value);
    }
}

// Now templates can use {{ request.path }}, {% csrf_token %}, {% static "..." %}
```

In templates, the CSRF token is used in forms:

```html
<form method="post">
    {% csrf_token %}
    <input type="text" name="title">
    <button type="submit">Submit</button>
</form>
```

The `{% csrf_token %}` tag outputs a hidden form field:

```html
<input type="hidden" name="csrfmiddlewaretoken" value="abc123...masked_token...">
```

---

## Part 9: The middleware pipeline

Middleware components process every request before it reaches a view and every response before it is sent to the client. They form an "onion" model: requests pass through middleware in order, and responses pass back through in reverse order.

### How middleware works

Each middleware implements the `Middleware` trait:

```rust
use async_trait::async_trait;
use django_rs_views::middleware::Middleware;
use django_rs_http::{HttpRequest, HttpResponse};
use django_rs_core::DjangoError;

struct TimingMiddleware;

#[async_trait]
impl Middleware for TimingMiddleware {
    async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> {
        // Runs before the view.
        // Return None to continue, or Some(response) to short-circuit.
        None
    }

    async fn process_response(
        &self,
        _request: &HttpRequest,
        response: HttpResponse,
    ) -> HttpResponse {
        // Runs after the view. Can modify the response.
        response
    }

    async fn process_exception(
        &self,
        _request: &HttpRequest,
        _error: &DjangoError,
    ) -> Option<HttpResponse> {
        // Runs if the view raises an error.
        // Return Some(response) for a custom error page.
        None
    }
}
```

The three methods give you hooks at different points in the request lifecycle:

- **`process_request`** -- Called before the view. Returning `Some(HttpResponse)` short-circuits the pipeline and skips the view entirely. This is useful for authentication checks, rate limiting, or maintenance mode.
- **`process_response`** -- Called after the view returns. Use it to add headers, compress content, or log metrics.
- **`process_exception`** -- Called if an error occurs. Return a custom error response or `None` to let default error handling proceed.

### The middleware pipeline

The `MiddlewarePipeline` manages the ordered list of middleware and runs them:

```rust
use django_rs_views::middleware::MiddlewarePipeline;
use django_rs_views::middleware::builtin::{
    SecurityMiddleware, CommonMiddleware, GZipMiddleware,
};

let mut pipeline = MiddlewarePipeline::new();
pipeline.add(SecurityMiddleware::default());
pipeline.add(CommonMiddleware::default());
pipeline.add(GZipMiddleware);
pipeline.add(TimingMiddleware);
```

When processing a request, the pipeline:

1. Calls `process_request` on each middleware **in order** (Security, Common, GZip, Timing)
2. Calls the view handler
3. Calls `process_response` on each middleware **in reverse order** (Timing, GZip, Common, Security)

If any `process_request` returns `Some(response)`, the pipeline short-circuits. Only middleware that already ran has its `process_response` called.

### Running the pipeline

```rust
use django_rs_views::middleware::ViewHandler;

let handler: ViewHandler = Box::new(|_req| {
    Box::pin(async { HttpResponse::ok("Hello from the view!") })
});

let request = HttpRequest::builder()
    .method(http::Method::GET)
    .path("/blog/")
    .build();

let response = pipeline.process(request, &handler).await;
```

### Built-in middleware

django-rs ships with several standard middleware components:

**SecurityMiddleware** -- Sets security headers on every response:
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `X-XSS-Protection: 1; mode=block`
- `Strict-Transport-Security` (configurable HSTS)

```rust
let security = SecurityMiddleware {
    hsts_seconds: 31536000,          // 1 year
    hsts_include_subdomains: true,
    hsts_preload: true,
    x_frame_options: "DENY".to_string(),
};
```

**CommonMiddleware** -- Handles trailing slash redirects and blocks disallowed user agents:

```rust
let common = CommonMiddleware {
    append_slash: true,
    disallowed_user_agents: vec!["BadBot".to_string()],
};
```

**GZipMiddleware** -- Compresses response bodies using gzip for clients that accept it.

**ConditionalGetMiddleware** -- Handles ETag and Last-Modified conditional requests, returning 304 Not Modified when appropriate.

**CorsMiddleware** -- Adds CORS headers for cross-origin requests.

### A typical middleware stack

```rust
let mut pipeline = MiddlewarePipeline::new();
pipeline.add(SecurityMiddleware::default());
pipeline.add(CommonMiddleware::default());
pipeline.add(GZipMiddleware);
// Add your custom middleware here
```

This mirrors Django's recommended `MIDDLEWARE` setting order.

---

## Part 10: Handling 404 errors

When the URL resolver cannot find a matching pattern, it returns a `DjangoError::NotFound`. You handle this by checking the resolve result and returning an appropriate response.

### URL resolution failure

```rust
let resolver = root(urlpatterns).unwrap();

match resolver.resolve("nonexistent-page/") {
    Ok(match_result) => {
        // Call the matched handler
        let response = (match_result.func)(request).await;
    }
    Err(DjangoError::NotFound(msg)) => {
        // Return a 404 response
        let response = HttpResponse::not_found("Page not found");
    }
    Err(e) => {
        // Handle other errors
        let response = HttpResponse::server_error(format!("Error: {e}"));
    }
}
```

### 404 in DetailView

When a `DetailView` implementation's `get_object` returns `DjangoError::NotFound` or `DjangoError::DoesNotExist`, the view automatically returns a 404 response.

### Custom 404 templates

For a polished 404 page, render a template in your error handling:

```rust
match resolver.resolve(request.path()) {
    Ok(match_result) => (match_result.func)(request).await,
    Err(_) => {
        let mut ctx = Context::new();
        ctx.set("path", ContextValue::from(request.path()));
        match engine.render_to_string("404.html", &mut ctx) {
            Ok(html) => {
                let mut response = HttpResponse::not_found(html);
                response.set_content_type("text/html");
                response
            }
            Err(_) => HttpResponse::not_found("Page not found"),
        }
    }
}
```

With a template like:

```html
<!-- templates/404.html -->
{% extends "base.html" %}

{% block title %}Page Not Found{% endblock %}

{% block content %}
<h1>404 - Page Not Found</h1>
<p>The page at <code>{{ path }}</code> could not be found.</p>
<p><a href="/">Return to the home page</a>.</p>
{% endblock %}
```

---

## Putting it all together

Here is a complete, runnable example that ties together views, templates, URL routing, and middleware into a working blog application:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{root, include, URLEntry, URLResolver};
use django_rs_http::urls::reverse::reverse;
use django_rs_http::{BoxFuture, HttpRequest, HttpResponse, JsonResponse};
use django_rs_template::engine::Engine;
use django_rs_template::context::{Context, ContextValue};

// ── Templates ──────────────────────────────────────────────────────

fn setup_templates(engine: &Engine) {
    engine.add_string_template("base.html", r#"<!DOCTYPE html>
<html>
<head><title>{% block title %}My Blog{% endblock %}</title></head>
<body>
    <nav>
        <a href="/">Home</a> | <a href="/blog/">Blog</a> | <a href="/about/">About</a>
    </nav>
    <main>{% block content %}{% endblock %}</main>
    <footer><p>&copy; 2026 My Blog</p></footer>
</body>
</html>"#);

    engine.add_string_template("home.html", r#"{% extends "base.html" %}
{% block title %}Home - My Blog{% endblock %}
{% block content %}
<h1>{{ title }}</h1>
<p>{{ tagline }}</p>
{% endblock %}"#);

    engine.add_string_template("post_list.html", r#"{% extends "base.html" %}
{% block title %}Blog Posts{% endblock %}
{% block content %}
<h1>Blog Posts</h1>
{% for post in posts %}
    <article>
        <h2><a href="/blog/{{ post.id }}/">{{ post.title }}</a></h2>
        <p>{{ post.content|truncatechars:120 }}</p>
        <small>By {{ post.author }} on {{ post.created_at }}</small>
    </article>
{% empty %}
    <p>No posts yet.</p>
{% endfor %}
{% endblock %}"#);

    engine.add_string_template("post_detail.html", r#"{% extends "base.html" %}
{% block title %}{{ post.title }} - My Blog{% endblock %}
{% block content %}
<article>
    <h1>{{ post.title }}</h1>
    <div class="meta">By {{ post.author }} on {{ post.created_at }}</div>
    <div class="content">{{ post.content }}</div>
</article>
{% endblock %}"#);

    engine.add_string_template("about.html", r#"{% extends "base.html" %}
{% block title %}About - My Blog{% endblock %}
{% block content %}
<h1>{{ page_title }}</h1>
<p>{{ description }}</p>
{% endblock %}"#);

    engine.add_string_template("404.html", r#"{% extends "base.html" %}
{% block title %}Page Not Found{% endblock %}
{% block content %}
<h1>404 - Page Not Found</h1>
<p>The page at <code>{{ path }}</code> could not be found.</p>
<p><a href="/">Return to the home page</a>.</p>
{% endblock %}"#);
}

// ── Sample Data ────────────────────────────────────────────────────

fn sample_posts() -> Vec<HashMap<String, ContextValue>> {
    vec![
        {
            let mut p = HashMap::new();
            p.insert("id".into(), ContextValue::Integer(1));
            p.insert("title".into(), ContextValue::from("Getting Started with Rust"));
            p.insert("slug".into(), ContextValue::from("getting-started-with-rust"));
            p.insert("content".into(), ContextValue::from(
                "Rust is a systems programming language focused on safety, \
                 speed, and concurrency. In this post we cover the basics."
            ));
            p.insert("author".into(), ContextValue::from("Alice"));
            p.insert("created_at".into(), ContextValue::from("June 1, 2025"));
            p
        },
        {
            let mut p = HashMap::new();
            p.insert("id".into(), ContextValue::Integer(2));
            p.insert("title".into(), ContextValue::from("Understanding Ownership"));
            p.insert("slug".into(), ContextValue::from("understanding-ownership"));
            p.insert("content".into(), ContextValue::from(
                "Ownership is Rust's most distinctive feature. It enables \
                 memory safety without a garbage collector."
            ));
            p.insert("author".into(), ContextValue::from("Bob"));
            p.insert("created_at".into(), ContextValue::from("July 15, 2025"));
            p
        },
    ]
}

// ── URL Configuration ──────────────────────────────────────────────

fn build_app(engine: Arc<Engine>) -> (URLResolver, Arc<Engine>) {
    // Home view
    let home_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> = {
        let engine = engine.clone();
        Arc::new(move |req: HttpRequest| {
            let engine = engine.clone();
            Box::pin(async move {
                let mut ctx = Context::new();
                ctx.set("title", ContextValue::from("Welcome to My Blog"));
                ctx.set("tagline", ContextValue::from(
                    "Thoughts on Rust and the web"
                ));
                match engine.render_to_string("home.html", &mut ctx) {
                    Ok(html) => HttpResponse::ok(html),
                    Err(_) => HttpResponse::server_error("Template error"),
                }
            })
        })
    };

    // About view
    let about_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> = {
        let engine = engine.clone();
        Arc::new(move |_req: HttpRequest| {
            let engine = engine.clone();
            Box::pin(async move {
                let mut ctx = Context::new();
                ctx.set("page_title", ContextValue::from("About Us"));
                ctx.set("description", ContextValue::from(
                    "We build web applications with Rust and django-rs."
                ));
                match engine.render_to_string("about.html", &mut ctx) {
                    Ok(html) => HttpResponse::ok(html),
                    Err(_) => HttpResponse::server_error("Template error"),
                }
            })
        })
    };

    // Blog list view
    let list_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> = {
        let engine = engine.clone();
        Arc::new(move |_req: HttpRequest| {
            let engine = engine.clone();
            Box::pin(async move {
                let posts = sample_posts();
                let mut ctx = Context::new();
                ctx.set("posts", ContextValue::List(
                    posts.into_iter().map(ContextValue::Dict).collect()
                ));
                match engine.render_to_string("post_list.html", &mut ctx) {
                    Ok(html) => HttpResponse::ok(html),
                    Err(_) => HttpResponse::server_error("Template error"),
                }
            })
        })
    };

    // Blog detail view
    let detail_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> = {
        let engine = engine.clone();
        Arc::new(move |req: HttpRequest| {
            let engine = engine.clone();
            Box::pin(async move {
                let post_id: i64 = req
                    .resolver_match()
                    .and_then(|m| m.kwargs.get("id"))
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);

                let posts = sample_posts();
                let post = posts.into_iter().find(|p| {
                    matches!(p.get("id"), Some(ContextValue::Integer(id)) if *id == post_id)
                });

                match post {
                    Some(p) => {
                        let mut ctx = Context::new();
                        ctx.set("post", ContextValue::Dict(p));
                        match engine.render_to_string("post_detail.html", &mut ctx) {
                            Ok(html) => HttpResponse::ok(html),
                            Err(_) => HttpResponse::server_error("Template error"),
                        }
                    }
                    None => HttpResponse::not_found("Post not found"),
                }
            })
        })
    };

    // Blog API (JSON)
    let api_handler: Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> =
        Arc::new(|_req: HttpRequest| {
            Box::pin(async {
                let data = serde_json::json!({
                    "posts": [
                        {"id": 1, "title": "Getting Started with Rust"},
                        {"id": 2, "title": "Understanding Ownership"},
                    ]
                });
                JsonResponse::new(&data)
            })
        });

    // Blog URL patterns
    let blog_patterns = vec![
        URLEntry::Pattern(path("", list_handler, Some("post-list")).unwrap()),
        URLEntry::Pattern(
            path("<int:id>/", detail_handler, Some("post-detail")).unwrap()
        ),
    ];

    // API URL patterns
    let api_patterns = vec![
        URLEntry::Pattern(path("posts/", api_handler, Some("post-api")).unwrap()),
    ];

    // Root URL configuration
    let urlpatterns = vec![
        URLEntry::Pattern(path("", home_handler, Some("home")).unwrap()),
        URLEntry::Pattern(path("about/", about_handler, Some("about")).unwrap()),
        URLEntry::Resolver(
            include("blog/", blog_patterns, Some("blog"), Some("blog")).unwrap()
        ),
        URLEntry::Resolver(
            include("api/", api_patterns, Some("api"), Some("api")).unwrap()
        ),
    ];

    (root(urlpatterns).unwrap(), engine)
}

// ── Server ─────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let engine = Arc::new(Engine::new());
    setup_templates(&engine);

    let (resolver, engine) = build_app(engine);
    let resolver = Arc::new(resolver);
    let engine = Arc::clone(&engine);

    // Demonstrate reverse URL resolution
    let mut kwargs = HashMap::new();
    kwargs.insert("id", "1");
    let url = reverse("blog:post-detail", &[], &kwargs, &resolver).unwrap();
    println!("Reversed blog:post-detail -> {url}");

    // Build an Axum router
    let resolver_for_handler = Arc::clone(&resolver);
    let engine_for_handler = Arc::clone(&engine);
    let app = axum::Router::new().fallback(move |req: axum::extract::Request| {
        let resolver = Arc::clone(&resolver_for_handler);
        let engine = Arc::clone(&engine_for_handler);
        async move {
            let (parts, body) = req.into_parts();
            let body_bytes = axum::body::to_bytes(body, usize::MAX)
                .await
                .unwrap_or_default()
                .to_vec();

            let mut django_request = HttpRequest::from_axum(parts, body_bytes);
            let path = django_request.path().trim_start_matches('/').to_string();

            match resolver.resolve(&path) {
                Ok(resolver_match) => {
                    let handler = resolver_match.func.clone();
                    django_request.set_resolver_match(resolver_match);
                    let response = handler(django_request).await;
                    axum::response::IntoResponse::into_response(response)
                }
                Err(_) => {
                    let mut ctx = Context::new();
                    ctx.set("path", ContextValue::from(
                        format!("/{path}")
                    ));
                    let html = engine
                        .render_to_string("404.html", &mut ctx)
                        .unwrap_or_else(|_| "Page not found".to_string());
                    let response = HttpResponse::not_found(html);
                    axum::response::IntoResponse::into_response(response)
                }
            }
        }
    });

    let addr = "127.0.0.1:8000";
    println!("Starting development server at http://{addr}/");
    println!("Quit the server with CONTROL-C.");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Run the server:

```bash
cargo run
```

Visit these URLs in your browser:

- [http://127.0.0.1:8000/](http://127.0.0.1:8000/) -- Home page (template-rendered)
- [http://127.0.0.1:8000/blog/](http://127.0.0.1:8000/blog/) -- Blog post listing
- [http://127.0.0.1:8000/blog/1/](http://127.0.0.1:8000/blog/1/) -- Post detail page
- [http://127.0.0.1:8000/about/](http://127.0.0.1:8000/about/) -- About page
- [http://127.0.0.1:8000/api/posts/](http://127.0.0.1:8000/api/posts/) -- JSON API endpoint
- [http://127.0.0.1:8000/nonexistent/](http://127.0.0.1:8000/nonexistent/) -- Custom 404 page

---

## Project structure

Here is the structure of the blog application after this tutorial:

```
myblog/
├── src/
│   ├── main.rs            # Server, views, URL config
│   ├── models.rs          # From Tutorial 2
│   ├── views.rs           # View functions and CBVs
│   └── urls.rs            # URL configuration
├── templates/
│   ├── base.html          # Base template with blocks
│   ├── 404.html           # Custom 404 page
│   ├── home.html          # Home page
│   ├── about.html         # About page
│   ├── post_list.html     # Blog post listing with pagination
│   ├── post_detail.html   # Individual post page
│   └── partials/
│       └── post_card.html # Reusable post card partial
├── Cargo.toml
└── static/
    └── css/
        └── style.css
```

---

## Comparison with Django

| Django (Python) | django-rs (Rust) |
|-----------------|------------------|
| `from django.http import HttpRequest, HttpResponse` | `use django_rs_http::{HttpRequest, HttpResponse};` |
| `from django.http import JsonResponse` | `use django_rs_http::JsonResponse;` |
| `from django.urls import path, include` | `use django_rs_http::urls::pattern::path;`<br>`use django_rs_http::urls::resolver::include;` |
| `from django.urls import reverse` | `use django_rs_http::urls::reverse::reverse;` |
| `from django.views import View` | `use django_rs_views::views::class_based::View;` |
| `from django.views.generic import TemplateView` | `use django_rs_views::views::class_based::TemplateView;` |
| `from django.views.generic import ListView` | `use django_rs_views::views::generic::ListView;` |
| `from django.template import Engine` | `use django_rs_template::engine::Engine;` |
| `path('posts/<int:id>/', view)` | `path("posts/<int:id>/", handler, name)` |
| `HttpResponse("Hello")` | `HttpResponse::ok("Hello")` |
| `reverse('app:view-name', kwargs={...})` | `reverse("app:view-name", &[], &kwargs, &resolver)` |
| `{{ variable\|filter:arg }}` | `{{ variable\|filter:arg }}` (same syntax) |
| `{% extends "base.html" %}` | `{% extends "base.html" %}` (same syntax) |

The main structural differences:
- **Handlers** are `Arc`-wrapped async closures rather than plain functions, because Rust needs explicit ownership semantics for sharing handlers across async tasks.
- **CBVs** use Rust traits rather than class inheritance, providing the same method dispatch pattern.
- **Templates** use identical DTL syntax -- no changes needed when porting templates from Django.

---

## Summary

In this tutorial you learned how to:

1. **Write function-based views** that accept `HttpRequest` and return `HttpResponse`, using `Arc`-wrapped async closures as route handlers.
2. **Define URL patterns** with `path()` using Django-style route syntax and path converters (`int`, `str`, `slug`, `uuid`, `path`).
3. **Organize URLs** with `include()` and namespaces for modular applications.
4. **Generate URLs** with `reverse()` using named patterns and keyword arguments.
5. **Set up the template engine** and render templates with context variables, dot-notation access, and auto-escaping.
6. **Use template features** -- filters (`lower`, `upper`, `truncatechars`, `date`, `safe`, `slugify`), tags (`if/elif/else`, `for/empty`, `with`, `csrf_token`), and template inheritance (`extends`, `block`, `include`).
7. **Build class-based views** with the `View` trait, `TemplateView`, `RedirectView`, `ListView`, and `DetailView`.
8. **Configure middleware** with `MiddlewarePipeline` using built-in middleware (`SecurityMiddleware`, `CommonMiddleware`, `GZipMiddleware`, `CorsMiddleware`).
9. **Use context processors** to inject global variables like `request`, `csrf_token`, and `STATIC_URL` into every template.
10. **Handle 404 errors** with custom templates and automatic error responses from `DetailView`.

---

## What is next

In [Tutorial 4: Forms and Validation](./04-forms-and-validation.md), you will build forms for creating and editing blog posts, handle form submission with validation, protect against CSRF attacks, and use `FormView` to tie the form layer into your views.
