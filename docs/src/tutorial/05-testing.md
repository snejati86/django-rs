# Tutorial 5: Testing Your App

In the previous four tutorials you built a blog application with models, an admin panel, views, templates, and forms. In this final tutorial, you will write automated tests to verify that everything works correctly and continues to work as you make changes.

This tutorial mirrors [Django's Tutorial Part 5](https://docs.djangoproject.com/en/stable/intro/tutorial05/), adapted for Rust and the django-rs test framework.

---

## Why test?

Tests save time. As your application grows, manually verifying each feature after every change becomes impractical. Automated tests check your code in seconds, catching regressions before they reach production.

Tests also serve as documentation. A well-written test suite describes exactly how your application is supposed to behave, in a format that the compiler can verify.

django-rs itself is backed by over **3,620 tests** across its 15 crates. This extensive test suite is what makes it possible to refactor internal code with confidence, knowing that the public API contracts are enforced by the tests.

In Rust, the compiler already prevents entire categories of bugs -- null pointer dereferences, data races, type mismatches -- at compile time. Tests complement the compiler by catching the errors it cannot: incorrect business logic, wrong HTTP status codes, missing database records, broken template rendering, and performance regressions.

---

## The test framework

django-rs ships a dedicated testing crate, `django-rs-test`, that provides everything you need to test models, views, forms, and middleware. The core imports are:

```rust
use django_rs::test::framework::{TestCase, TestRunner};
use django_rs::test::client::TestClient;
```

Here is an overview of every component:

| Component | Purpose |
|---|---|
| `TestClient` | Simulate HTTP requests against your Axum router |
| `RequestFactory` | Build `HttpRequest` objects without routing or middleware |
| `TestDatabase` | In-memory SQLite database with automatic isolation |
| `TestCase` | Structured test setup with a client and settings overrides |
| Assertion helpers | `assert_contains`, `assert_redirects`, `assert_status`, and more |
| `assert_num_queries` | Verify the exact number of SQL queries executed |
| `OverrideSettings` | Temporarily swap framework settings for a test |
| `MailOutbox` | Capture emails sent during a test |
| `LiveServerTestCase` | Spin up a real HTTP server for integration tests |

Add `django-rs-test` as a dev dependency in your `Cargo.toml`:

```toml
[dev-dependencies]
django-rs-test = { path = "../crates/django-rs-test" }
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

All tests in django-rs are async, so you will use `#[tokio::test]` instead of `#[test]` for any test that calls `.await`.

---

## Writing model tests

Model tests verify that your data layer behaves correctly: schema creation, CRUD operations, query filtering, and ordering. Each test gets its own in-memory database through `TestDatabase::new()`, ensuring complete isolation.

### QuerySet filtering

The `QuerySet` API generates SQL from chainable method calls. Testing it means verifying that the generated SQL contains the correct clauses and that queries return the expected results.

```rust
#[cfg(test)]
mod tests {
    use django_rs::db::{FieldDef, FieldType, Value, ModelMeta};
    use django_rs::db::query::queryset::QuerySet;
    use django_rs::db::query::lookups::Q;

    #[tokio::test]
    async fn test_queryset_filter() {
        let qs = QuerySet::new("blog", "post")
            .filter(Q::new("published", "exact", Value::Bool(true)));

        let (sql, params) = qs.to_sql(&backend);
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("published"));
    }

    #[tokio::test]
    async fn test_queryset_ordering() {
        let qs = QuerySet::new("blog", "post")
            .order_by(vec![OrderBy::desc("created_at")]);

        let (sql, _) = qs.to_sql(&backend);
        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("DESC"));
    }
}
```

### CRUD operations with TestDatabase

`TestDatabase` creates a fresh, in-memory SQLite database for each test. Every call to `TestDatabase::new()` produces a completely isolated database, so tests never interfere with each other -- even when running in parallel.

```rust
use django_rs_test::TestDatabase;
use django_rs_db::value::Value;
use django_rs_db::DbExecutor;

#[tokio::test]
async fn test_create_and_read_post() {
    let db = TestDatabase::new();
    db.execute_raw(
        "CREATE TABLE blog_post (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            content TEXT,
            published INTEGER NOT NULL DEFAULT 0
        )",
    )
    .await
    .unwrap();

    let pk = db
        .insert_returning_id(
            "INSERT INTO blog_post (title, content, published) VALUES (?, ?, ?)",
            &[
                Value::from("My First Post"),
                Value::from("This is the content."),
                Value::from(true),
            ],
        )
        .await
        .unwrap();

    assert_eq!(pk, Value::Int(1));

    let row = db
        .query_one(
            "SELECT id, title, published FROM blog_post WHERE id = ?",
            &[Value::from(1)],
        )
        .await
        .unwrap();

    assert_eq!(row.get::<String>("title").unwrap(), "My First Post");
}

#[tokio::test]
async fn test_query_published_posts_only() {
    let db = TestDatabase::new();
    db.execute_raw(
        "CREATE TABLE blog_post (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            published INTEGER NOT NULL DEFAULT 0
        )",
    )
    .await
    .unwrap();

    db.execute_sql(
        "INSERT INTO blog_post (title, published) VALUES (?, ?)",
        &[Value::from("Published"), Value::from(true)],
    )
    .await
    .unwrap();

    db.execute_sql(
        "INSERT INTO blog_post (title, published) VALUES (?, ?)",
        &[Value::from("Draft"), Value::from(false)],
    )
    .await
    .unwrap();

    let rows = db
        .query(
            "SELECT title FROM blog_post WHERE published = ?",
            &[Value::from(true)],
        )
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String>("title").unwrap(), "Published");
}
```

Each test function gets its own `TestDatabase::new()`, so there is no shared state between tests. This is a critical property: it means you can run tests in any order, in parallel, and they will always produce the same results.

---

## Testing forms

Form tests verify two things: that valid data passes validation and produces the correct cleaned values, and that invalid data is rejected with appropriate error messages.

### Valid form data

```rust
#[tokio::test]
async fn test_form_validation_valid_data() {
    let mut form = BaseForm::new(vec![
        FormFieldDef::new("email", FormFieldType::Email).required(true),
    ]);

    let data = QueryDict::parse("email=test@example.com");
    form.bind(&data);

    assert!(form.is_valid().await);
    assert_eq!(
        form.cleaned_data().get("email"),
        Some(&Value::String("test@example.com".into()))
    );
}
```

The `is_valid()` method runs the full two-phase validation pipeline: first field-level type coercion and constraint checking, then form-level cross-field validation. When it returns `true`, the `cleaned_data()` map contains typed `Value` entries for every field.

### Invalid form data

```rust
#[tokio::test]
async fn test_form_validation_invalid_email() {
    let mut form = BaseForm::new(vec![
        FormFieldDef::new("email", FormFieldType::Email).required(true),
    ]);

    let data = QueryDict::parse("email=not-an-email");
    form.bind(&data);

    assert!(!form.is_valid().await);
    assert!(form.errors().contains_key("email"));
}
```

When validation fails, `errors()` returns a `HashMap<String, Vec<String>>` mapping field names to their error messages. The framework does not short-circuit on the first error -- all fields are validated, so users see every problem at once.

### Testing multiple fields and edge cases

```rust
#[tokio::test]
async fn test_required_field_missing() {
    let mut form = BaseForm::new(vec![
        FormFieldDef::new("name", FormFieldType::Char {
            min_length: Some(2),
            max_length: Some(100),
            strip: true,
        })
        .required(true),
        FormFieldDef::new("email", FormFieldType::Email).required(true),
    ]);

    // Submit with email but no name
    let data = QueryDict::parse("email=alice@example.com");
    form.bind(&data);

    assert!(!form.is_valid().await);
    assert!(form.errors().contains_key("name"));
    assert!(!form.errors().contains_key("email"));
}

#[tokio::test]
async fn test_char_field_min_length() {
    let mut form = BaseForm::new(vec![
        FormFieldDef::new("name", FormFieldType::Char {
            min_length: Some(2),
            max_length: Some(100),
            strip: true,
        })
        .required(true),
    ]);

    let data = QueryDict::parse("name=A");
    form.bind(&data);

    assert!(!form.is_valid().await);
    let errors = form.errors();
    assert!(errors.get("name").unwrap()[0].contains("at least 2 characters"));
}
```

---

## Testing views with TestClient

`TestClient` lets you make simulated HTTP requests against your application without starting a real server. It wraps your Axum `Router` and sends requests through the full framework stack: URL resolution, middleware, views, and template rendering.

### GET requests

```rust
use django_rs::test::client::TestClient;

#[tokio::test]
async fn test_index_view() {
    let client = TestClient::new();
    let response = client.get("/").await;

    assert_eq!(response.status(), 200);
    assert!(response.content().contains("Welcome"));
}
```

The `TestResponse` returned by the client provides several methods for inspection:

| Method | Description |
|---|---|
| `status_code()` | The numeric HTTP status |
| `text()` | The response body as a string |
| `json::<T>()` | Deserialize the body as JSON |
| `header("name")` | Get a response header value |
| `has_header("name")` | Check if a header exists |
| `contains("text")` | Check if the body contains a substring |

### POST requests with form data

```rust
#[tokio::test]
async fn test_post_form() {
    let client = TestClient::new();
    let response = client.post("/contact/", &[
        ("name", "Alice"),
        ("email", "alice@test.com"),
    ]).await;

    assert_eq!(response.status(), 302); // Redirect after success
}
```

The `TestClient` also supports `put`, `patch`, `delete`, `head`, and `options` methods for the corresponding HTTP verbs.

### Testing authentication

The `force_login` method simulates a logged-in user without going through the actual login flow, mirroring Django's `Client.force_login()`:

```rust
use django_rs_auth::user::AbstractUser;

#[tokio::test]
async fn test_admin_requires_authentication() {
    let mut client = TestClient::new(make_app());

    // Unauthenticated request
    let response = client.get("/admin/").await;
    assert_eq!(response.status_code(), 302); // Redirected to login

    // Authenticated request
    let user = AbstractUser::new("admin");
    client.force_login(&user);

    let response = client.get("/admin/").await;
    assert_eq!(response.status_code(), 200);
}
```

### Cookie management

The `TestClient` automatically stores cookies from `Set-Cookie` response headers and sends them in subsequent requests, just like a browser:

```rust
#[tokio::test]
async fn test_cookies_persist_across_requests() {
    let mut client = TestClient::new(make_app());

    let response = client.get("/set-preference/").await;
    assert_eq!(response.cookies.get("theme"), Some(&"dark".to_string()));

    let response = client.get("/check-preference/").await;
    assert!(response.text().contains("theme=dark"));
}
```

---

## Test utilities

django-rs provides several utilities that make tests more expressive and easier to maintain.

### TestDatabase for isolated database testing

`TestDatabase` creates a fresh in-memory SQLite database for each test. There is no shared state between tests, so they can run in any order and in parallel:

```rust
use django_rs_test::TestDatabase;

#[tokio::test]
async fn test_isolated_database() {
    let db = TestDatabase::new(); // Fresh database every time
    db.setup_table(&blog_post_meta()).await.unwrap();

    // This test's data is invisible to every other test
    db.execute_sql(
        "INSERT INTO blog_post (title, published) VALUES (?, ?)",
        &[Value::from("Only Here"), Value::from(true)],
    )
    .await
    .unwrap();
}
```

The `teardown` method drops all user-created tables if you need to reset state mid-test:

```rust
db.teardown().await.unwrap();
```

### RequestFactory for building test requests

While `TestClient` sends requests through the full framework stack, `RequestFactory` builds raw `HttpRequest` objects that you can pass directly to a view function. Use it when you want to unit test a view in complete isolation:

```rust
use django_rs_test::RequestFactory;

#[test]
fn test_factory_builds_get_request() {
    let factory = RequestFactory::new();
    let request = factory.get("/posts/");

    assert_eq!(request.method(), &http::Method::GET);
    assert_eq!(request.path(), "/posts/");
}

#[test]
fn test_factory_builds_post_request() {
    let factory = RequestFactory::new();
    let mut data = std::collections::HashMap::new();
    data.insert("title".to_string(), "Test".to_string());

    let request = factory.post("/posts/create/", &data);

    assert_eq!(request.method(), &http::Method::POST);
    assert_eq!(request.post().get("title"), Some("Test"));
}
```

| Use TestClient when... | Use RequestFactory when... |
|---|---|
| You want to test the full request pipeline | You want to test a single view function |
| You need cookies and session persistence | You need a bare `HttpRequest` to pass to a function |
| You are testing URL routing and middleware | You are testing view logic in isolation |

### override_settings for temporary config changes

Some tests need different framework settings -- for example, testing behavior when `DEBUG` is off. The `override_settings` function temporarily swaps settings for the duration of a closure, then restores the originals:

```rust
use django_rs_test::{override_settings, get_settings, SettingsOverride};

#[test]
fn test_debug_mode_off() {
    override_settings(SettingsOverride::new().set_debug(false), || {
        let settings = get_settings();
        assert!(!settings.debug);
    });

    // After the closure, the original settings are restored
    let settings = get_settings();
    assert!(settings.debug); // Back to default
}
```

Settings are restored even if the closure panics. The implementation uses a drop guard, so there is no risk of leaving stale overrides behind.

### assert_num_queries for performance testing

One of the most common performance problems in web applications is the N+1 query problem. `assert_num_queries` catches this by letting you specify exactly how many database queries a block of code should execute:

```rust
use django_rs_test::assert_num_queries;

#[tokio::test]
async fn test_inserting_three_posts_executes_three_queries() {
    let db = TestDatabase::new();
    db.execute_raw(
        "CREATE TABLE blog_post (id INTEGER PRIMARY KEY, title TEXT NOT NULL)"
    ).await.unwrap();

    assert_num_queries(&db, 3, || async {
        for i in 1..=3 {
            db.execute_sql(
                "INSERT INTO blog_post (title) VALUES (?)",
                &[Value::from(format!("Post {i}"))],
            )
            .await
            .unwrap();
        }
    }).await;
}
```

If the count does not match, the test fails with a clear message:

```
Expected 3 SQL queries, but 5 were executed
```

### Assertion helpers

The test framework provides assertion functions that produce clear, descriptive error messages:

```rust
use django_rs_test::{assert_status, assert_contains, assert_not_contains, assert_redirects};

#[tokio::test]
async fn test_with_assertion_helpers() {
    let mut client = TestClient::new(make_app());

    let response = client.get("/").await;
    assert_status(&response, 200);
    assert_contains(&response, "Welcome");
    assert_not_contains(&response, "Error");

    let response = client.get("/old-url/").await;
    assert_redirects(&response, "/new-url/");
}
```

Using these instead of raw `assert!` calls makes test failures easier to diagnose. When `assert_contains` fails, it prints the actual response body. When a raw `assert!(response.text().contains("Welcome"))` fails, you only see `assertion failed`.

---

## Running tests

django-rs tests are standard Rust tests. You run them with `cargo test`:

```bash
# Run all tests in your workspace
cargo test --workspace

# Run tests for a specific crate
cargo test -p django-rs-db

# Run a single test by name
cargo test test_queryset_filter

# Run tests matching a pattern
cargo test --workspace queryset

# Run with stdout/stderr output visible
cargo test --workspace -- --nocapture
```

Cargo compiles your test binary, runs every function annotated with `#[test]` or `#[tokio::test]`, and reports pass/fail results. Tests run in parallel by default, which is why `TestDatabase` isolation is so important.

### Test project structure

Rust has built-in support for tests via `#[cfg(test)]` modules for unit tests, and a `tests/` directory for integration tests:

```
myblog/
  src/
    main.rs
    models.rs
    views.rs
  tests/
    test_models.rs
    test_views.rs
    test_forms.rs
  Cargo.toml
```

Unit tests live inside your source files in `#[cfg(test)]` modules. Integration tests live in the `tests/` directory and have access only to the public API of your crate.

---

## Best practices

### 1. One test per component

Each test function should verify one logical behavior. It is fine to have multiple `assert!` calls if they all verify different aspects of the same behavior, but avoid testing unrelated things in the same function.

```rust
// Good: focused on one behavior
#[tokio::test]
async fn test_create_redirects_to_list() {
    let mut client = TestClient::new(blog_app());
    let mut data = HashMap::new();
    data.insert("title".to_string(), "Post".to_string());
    let response = client.post("/posts/create/", &data).await;
    assert_redirects(&response, "/posts/");
}

// Good: multiple assertions about the same response
#[tokio::test]
async fn test_list_view_shows_all_posts() {
    let mut client = TestClient::new(blog_app());
    let response = client.get("/posts/").await;
    assert_status(&response, 200);
    assert_contains(&response, "First Post");
    assert_contains(&response, "Second Post");
}
```

### 2. Descriptive test names

Test names should describe what is being tested and what the expected outcome is. Rust convention is to use `snake_case` starting with `test_`:

```rust
// Good names
#[tokio::test] async fn test_published_posts_appear_in_list() { /* ... */ }
#[tokio::test] async fn test_draft_posts_hidden_from_anonymous_users() { /* ... */ }
#[tokio::test] async fn test_unauthenticated_user_cannot_create_post() { /* ... */ }

// Vague names -- avoid
#[tokio::test] async fn test_list() { /* ... */ }
#[tokio::test] async fn test_post() { /* ... */ }
#[tokio::test] async fn test_auth() { /* ... */ }
```

### 3. Test edge cases

Do not only test the happy path. Verify that your application handles boundary conditions and errors correctly:

```rust
#[tokio::test]
async fn test_create_with_valid_data() {
    // ... should succeed with 302 redirect
}

#[tokio::test]
async fn test_create_with_empty_title() {
    // ... should fail with 400 and error message
}

#[tokio::test]
async fn test_create_without_authentication() {
    // ... should redirect to login page
}

#[tokio::test]
async fn test_detail_view_with_nonexistent_id() {
    // ... should return 404
}
```

### 4. Use helper functions for common setup

Extract repeated setup into helper functions to keep tests focused on what they are actually verifying:

```rust
fn make_test_db() -> TestDatabase {
    let db = TestDatabase::new();
    // Common schema setup
    db
}

async fn seed_posts(db: &TestDatabase) {
    db.execute_sql(
        "INSERT INTO blog_post (title, published) VALUES (?, ?)",
        &[Value::from("Test Post"), Value::from(true)],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn test_query_published_posts() {
    let db = make_test_db();
    seed_posts(&db).await;
    // ... the actual test logic
}
```

### 5. Use assertion helpers over raw assertions

The django-rs assertion helpers produce better error messages:

```rust
// Prefer this -- prints the actual body on failure
assert_contains(&response, "Welcome");

// Over this -- only prints "assertion failed"
assert!(response.text().contains("Welcome"));
```

### 6. Keep tests fast

Tests should run quickly. The in-memory SQLite database and simulated HTTP client are designed for speed. Avoid:

- Sleeping (`tokio::time::sleep`) unless testing time-dependent behavior
- Making real network requests (use `TestClient` instead of `reqwest`)
- Writing to the filesystem unless necessary

Use `LiveServerTestCase` only when you genuinely need a real HTTP server.

### 7. Isolate test state

Every test should create its own `TestDatabase`, `TestClient`, and `MailOutbox`. Never share mutable state between tests:

```rust
// Good: each test is independent
#[tokio::test]
async fn test_a() {
    let db = TestDatabase::new();
    // ...
}

#[tokio::test]
async fn test_b() {
    let db = TestDatabase::new();
    // ...
}
```

### 8. Test the contract, not the implementation

Focus on what your views return, not how they produce the result. If a view returns a list of posts as HTML, test that the HTML contains the expected posts. Do not test the internal query structure unless you are specifically testing for performance with `assert_num_queries`.

---

## Summary

In this tutorial you learned how to:

- **Motivate testing** -- why automated tests complement the Rust compiler, and how django-rs's own 3,620 tests provide confidence in the framework
- **Use the test framework** -- `TestClient`, `TestDatabase`, `RequestFactory`, `TestCase`, and assertion helpers
- **Test models** -- verifying QuerySet filtering, ordering, and CRUD operations with isolated in-memory databases
- **Test forms** -- validating that correct data passes and incorrect data produces appropriate errors
- **Test views** -- simulating GET and POST requests with `TestClient`, checking status codes, response content, and redirects
- **Use test utilities** -- `override_settings` for configuration, `assert_num_queries` for performance, and `RequestFactory` for unit-level view testing
- **Run tests** -- `cargo test` with filtering by crate, test name, and pattern
- **Write effective tests** -- descriptive names, edge case coverage, isolated state, helper functions, and contract-focused assertions

You now have a complete blog application with models, admin interface, views, templates, forms, and a comprehensive test suite. From here, you can continue building features with confidence, knowing that your tests will catch regressions as your codebase grows.

---

This concludes the django-rs tutorial series. For more information, see the [django-rs source code](https://github.com/snejati86/django-rs) and the API documentation for each crate.
