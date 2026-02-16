# django-rs

[![CI](https://github.com/snejati86/django-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/snejati86/django-rs/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![Rust: 1.75+](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

**A full-featured Django-equivalent web framework for Rust.**

django-rs brings Django's batteries-included philosophy to the Rust ecosystem. It provides an ORM, admin panel, authentication, forms, templates, migrations, URL routing, middleware, and a management CLI — all with Rust's type safety and performance.

## Features

- **ORM** — Model definitions, QuerySets, Managers, field types, and expressions
- **Database backends** — PostgreSQL, MySQL, and SQLite with async drivers
- **Migrations** — Auto-detected schema migrations with forward/backward support
- **Admin panel** — Auto-generated CRUD admin with search, filtering, and inline editing
- **Authentication** — Users, groups, permissions, sessions, and password hashing (Argon2/bcrypt)
- **Forms** — Form and ModelForm with field validation and widget rendering
- **Templates** — Django Template Language (DTL)-compatible template engine
- **Views** — Class-based views, generic views, and mixins
- **HTTP** — Request/Response abstractions, URL routing, and middleware pipeline
- **Signals** — Decoupled event dispatch (pre_save, post_save, etc.)
- **CLI** — Management commands: runserver, migrate, shell, and custom commands
- **Testing** — Test client, fixtures, assertions, and database test utilities
- **Procedural macros** — Derive macros for models, views, forms, and admin registration

## Quick Start

Add django-rs to your `Cargo.toml`:

```toml
[dependencies]
django-rs = { git = "https://github.com/snejati86/django-rs" }
tokio = { version = "1", features = ["full"] }
axum = "0.8"
```

Define models and register them with the admin:

```rust
use django_rs::admin::{AdminSite, ModelAdmin, FieldSchema};
use std::sync::Arc;

// Configure a model for the admin panel
let post_admin = ModelAdmin::new("blog", "post")
    .verbose_name("Post")
    .verbose_name_plural("Posts")
    .list_display(vec!["id", "title", "author", "published", "created_at"])
    .search_fields(vec!["title", "content", "author"])
    .list_filter_fields(vec!["published", "author"])
    .ordering(vec!["-id"])
    .fields_schema(vec![
        FieldSchema::new("id", "BigAutoField").primary_key(),
        FieldSchema::new("title", "CharField")
            .max_length(200)
            .label("Title"),
        FieldSchema::new("content", "TextField")
            .label("Content"),
        FieldSchema::new("author", "CharField")
            .max_length(100)
            .label("Author"),
        FieldSchema::new("published", "BooleanField")
            .label("Published"),
        FieldSchema::new("created_at", "DateTimeField")
            .label("Created at")
            .read_only(),
    ]);

// Register with admin site and mount on Axum
let mut site = AdminSite::new("My App Admin");
site.register("blog.post", post_admin);

let app = axum::Router::new()
    .nest("/api/admin", site.into_axum_router());

let listener = tokio::net::TcpListener::bind("127.0.0.1:8000").await.unwrap();
axum::serve(listener, app).await.unwrap();
```

See the [blog example](examples/blog/) for a complete working application with admin panel.

## Crate Architecture

django-rs is composed of 15 crates, mirroring Django's modular design:

| Crate | Description |
|-------|-------------|
| [`django-rs`](django-rs/) | Meta-crate that re-exports all sub-crates |
| [`django-rs-core`](crates/django-rs-core/) | Core types, settings, app registry, and error types |
| [`django-rs-macros`](crates/django-rs-macros/) | Procedural macros for models, views, forms, and admin |
| [`django-rs-db`](crates/django-rs-db/) | ORM: Model definitions, QuerySet, Manager, and expressions |
| [`django-rs-db-backends`](crates/django-rs-db-backends/) | Database backends: PostgreSQL, MySQL, and SQLite |
| [`django-rs-db-migrations`](crates/django-rs-db-migrations/) | Migration engine with auto-detection and management |
| [`django-rs-http`](crates/django-rs-http/) | HTTP layer: Request, Response, URL routing, and middleware |
| [`django-rs-views`](crates/django-rs-views/) | Class-based views, generic views, and mixins |
| [`django-rs-forms`](crates/django-rs-forms/) | Forms, ModelForms, widgets, and validation |
| [`django-rs-template`](crates/django-rs-template/) | DTL-compatible template rendering |
| [`django-rs-auth`](crates/django-rs-auth/) | Authentication: Users, Permissions, Groups, Sessions |
| [`django-rs-admin`](crates/django-rs-admin/) | Auto-generated admin panel with CRUD API |
| [`django-rs-signals`](crates/django-rs-signals/) | Signal dispatcher for decoupled event handling |
| [`django-rs-cli`](crates/django-rs-cli/) | Management commands CLI: runserver, migrate, shell |
| [`django-rs-test`](crates/django-rs-test/) | Testing framework: test client, fixtures, assertions |

```
django-rs (meta-crate)
├── django-rs-core          ← settings, app registry, error types
├── django-rs-macros        ← #[derive(Model)], #[admin], etc.
├── django-rs-db            ← ORM layer (QuerySet, Manager, Fields)
│   └── django-rs-db-backends    ← PostgreSQL, MySQL, SQLite drivers
│   └── django-rs-db-migrations  ← schema migration engine
├── django-rs-http          ← request/response, URL routing, middleware
├── django-rs-views         ← class-based views, generic views
├── django-rs-forms         ← forms, model forms, validation
├── django-rs-template      ← DTL template engine
├── django-rs-auth          ← users, groups, permissions, sessions
├── django-rs-admin         ← auto-generated admin panel
├── django-rs-signals       ← signal dispatcher
├── django-rs-cli           ← management commands
└── django-rs-test          ← test utilities
```

## Django Feature Parity

| Django Feature | django-rs Equivalent | Status |
|---|---|---|
| Models & ORM | `django-rs-db` | Implemented |
| QuerySets | `django-rs-db` QuerySet | Implemented |
| Migrations | `django-rs-db-migrations` | Implemented |
| Admin site | `django-rs-admin` + admin-frontend | Implemented |
| Authentication | `django-rs-auth` | Implemented |
| Forms & validation | `django-rs-forms` | Implemented |
| Template engine | `django-rs-template` | Implemented |
| URL routing | `django-rs-http` | Implemented |
| Middleware | `django-rs-http` | Implemented |
| Class-based views | `django-rs-views` | Implemented |
| Signals | `django-rs-signals` | Implemented |
| Management commands | `django-rs-cli` | Implemented |
| Test framework | `django-rs-test` | Implemented |

## Minimum Supported Rust Version

The MSRV is **Rust 1.75**.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on how to contribute.
