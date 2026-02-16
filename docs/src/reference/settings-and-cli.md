# Settings and CLI

This reference covers framework configuration and the command-line management tool.

---

## Settings

django-rs uses a settings module (`django-rs-core`) to configure the framework. Settings control database connections, installed apps, middleware, templates, static files, and more.

### Core settings

| Setting | Type | Description |
|---------|------|-------------|
| `debug` | `bool` | Enable debug mode (detailed error pages, SQL logging) |
| `secret_key` | `String` | Cryptographic key for CSRF tokens, sessions, etc. |
| `allowed_hosts` | `Vec<String>` | Hostnames the server will accept |
| `installed_apps` | `Vec<String>` | List of active application labels |
| `root_urlconf` | `String` | The root URL configuration module |
| `language_code` | `String` | Default language (e.g., `"en-us"`) |
| `time_zone` | `String` | Default timezone (e.g., `"UTC"`) |
| `use_tz` | `bool` | Store datetimes in UTC |

### Database settings

| Setting | Type | Description |
|---------|------|-------------|
| `databases.default.engine` | `String` | Backend: `"postgresql"`, `"sqlite3"`, `"mysql"` |
| `databases.default.name` | `String` | Database name or file path |
| `databases.default.host` | `String` | Database host |
| `databases.default.port` | `u16` | Database port |
| `databases.default.user` | `String` | Database user |
| `databases.default.password` | `String` | Database password |

### Middleware settings

| Setting | Type | Description |
|---------|------|-------------|
| `middleware` | `Vec<String>` | Ordered list of middleware classes |

### Template settings

| Setting | Type | Description |
|---------|------|-------------|
| `templates.dirs` | `Vec<PathBuf>` | Template search directories |
| `templates.app_dirs` | `bool` | Search `templates/` in each app |
| `templates.context_processors` | `Vec<String>` | Context processors to apply |

### Static file settings

| Setting | Type | Description |
|---------|------|-------------|
| `static_url` | `String` | URL prefix for static files (e.g., `"/static/"`) |
| `static_root` | `PathBuf` | Directory for collected static files |
| `staticfiles_dirs` | `Vec<PathBuf>` | Additional static file directories |
| `media_url` | `String` | URL prefix for user-uploaded files |
| `media_root` | `PathBuf` | Directory for user-uploaded files |

### Email settings

| Setting | Type | Description |
|---------|------|-------------|
| `email_backend` | `String` | Email sending backend |
| `email_host` | `String` | SMTP server host |
| `email_port` | `u16` | SMTP server port |
| `email_host_user` | `String` | SMTP authentication user |
| `email_host_password` | `String` | SMTP authentication password |
| `email_use_tls` | `bool` | Use TLS for SMTP |

### Overriding settings in tests

```rust
use django_rs_test::{override_settings, SettingsOverride};

override_settings(SettingsOverride::new().set_debug(false), || {
    // Tests run with DEBUG=false
});
// Original settings restored automatically
```

---

## CLI

The `django-rs-cli` crate provides a command-line management tool similar to Django's `manage.py`. It supports project scaffolding, migrations, and development server management.

### Available commands

| Command | Description |
|---------|-------------|
| `startproject <name>` | Create a new project with default structure |
| `startapp <name>` | Create a new application within a project |
| `makemigrations` | Generate migration files from model changes |
| `migrate` | Apply pending migrations to the database |
| `showmigrations` | List all migrations and their status |
| `sqlmigrate <app> <migration>` | Show the SQL for a specific migration |
| `runserver [addr:port]` | Start the development server |
| `shell` | Open an interactive Rust shell |
| `createsuperuser` | Create a superuser account |
| `collectstatic` | Collect static files into STATIC_ROOT |
| `check` | Run system checks |
| `test [pattern]` | Run tests (delegates to `cargo test`) |

### Usage examples

```bash
# Create a new project
django-rs startproject mysite

# Create an app within the project
django-rs startapp blog

# Generate migrations after changing models
django-rs makemigrations

# Apply all pending migrations
django-rs migrate

# Show migration status
django-rs showmigrations

# Show SQL for a specific migration
django-rs sqlmigrate blog 0001

# Start the development server
django-rs runserver
django-rs runserver 0.0.0.0:8080

# Create a superuser
django-rs createsuperuser

# Collect static files for production
django-rs collectstatic

# Run system checks
django-rs check
```

### Custom management commands

You can define custom management commands using the `#[management_command]` proc macro:

```rust
use django_rs_macros::management_command;

#[management_command]
fn seed_data() {
    println!("Seeding database with sample data...");
    // Insert sample records
}
```

Custom commands are automatically discovered and made available through the CLI.
