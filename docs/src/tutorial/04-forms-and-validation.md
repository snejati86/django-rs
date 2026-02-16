# Tutorial 4: Forms and Validation

In the previous tutorial, you built views and templates for the blog application. In this tutorial, you will add **form handling** -- defining forms with typed fields, processing user input, validating data, rendering widgets, and protecting against CSRF attacks. This mirrors [Django's Tutorial Part 4](https://docs.djangoproject.com/en/stable/intro/tutorial04/), adapted for django-rs.

By the end of this tutorial, you will have a working contact form, a blog post creation form backed by your model, and an understanding of the full form lifecycle in django-rs.

---

## Overview

The django-rs forms framework lives in the `django-rs-forms` crate and provides:

- **`BaseForm`** -- A general-purpose form built from a list of field definitions
- **`FormFieldDef`** -- A builder for defining individual fields with type, label, help text, and validators
- **`FormFieldType`** -- An enum covering every common data type (strings, numbers, dates, choices, files, and more)
- **Widgets** -- HTML rendering components that map fields to `<input>`, `<select>`, `<textarea>`, and other elements
- **Validation** -- A two-phase pipeline: field-level type coercion, then form-level cross-field validation (async)
- **`ModelFormConfig`** -- Auto-generates form fields from ORM model metadata
- **`FormSet`** -- Manages collections of related forms on a single page
- **CSRF middleware** -- Token generation, masking, and validation to protect state-changing requests

---

## Part 1: Defining Forms

A form in django-rs is a `BaseForm` constructed from a `Vec<FormFieldDef>`. Each field definition specifies the field name, its type, and optional metadata like labels and help text.

### Your first form

Start by defining a contact form:

```rust
use django_rs_forms::form::BaseForm;
use django_rs_forms::fields::{FormFieldDef, FormFieldType};

fn contact_form() -> BaseForm {
    BaseForm::new(vec![
        FormFieldDef::new("name", FormFieldType::Char {
            min_length: Some(2),
            max_length: Some(100),
            strip: true,
        })
        .required(true)
        .label("Your Name")
        .help_text("Enter your full name"),

        FormFieldDef::new("email", FormFieldType::Email)
            .required(true)
            .label("Email Address")
            .help_text("We will never share your email"),

        FormFieldDef::new("subject", FormFieldType::Char {
            min_length: None,
            max_length: Some(200),
            strip: true,
        })
        .required(true)
        .label("Subject"),

        FormFieldDef::new("message", FormFieldType::Char {
            min_length: Some(10),
            max_length: Some(2000),
            strip: true,
        })
        .required(true)
        .label("Message")
        .widget(django_rs_forms::widgets::WidgetType::Textarea),

        FormFieldDef::new("age", FormFieldType::Integer {
            min_value: Some(18),
            max_value: Some(120),
        })
        .required(false)
        .label("Age"),
    ])
}
```

Every `FormFieldDef` starts with a field name and a `FormFieldType`. The name becomes the HTML `name` attribute and the key in `cleaned_data()`. The field type controls how raw string input is parsed, coerced, and validated.

### The FormFieldDef builder

`FormFieldDef::new()` returns a builder with sensible defaults. All builder methods consume and return `self`, so you can chain them:

| Method | Default | Description |
|---|---|---|
| `.required(bool)` | `true` | Whether the field must have a non-empty value |
| `.label(str)` | Derived from field name | Human-readable label for the `<label>` element |
| `.help_text(str)` | `""` | Explanatory text displayed alongside the field |
| `.widget(WidgetType)` | Inferred from field type | The HTML widget used for rendering |
| `.initial(Value)` | `None` | Default value when the form is unbound |
| `.disabled(bool)` | `false` | Renders the field but prevents editing |
| `.error_message(code, msg)` | Built-in messages | Custom error message for a specific error code |
| `.validator(Box<dyn Validator>)` | None | Additional validators beyond type-level validation |

Example:

```rust
FormFieldDef::new("email", FormFieldType::Email)
    .required(true)
    .label("Email Address")
    .help_text("Enter a valid email")
    .error_message("required", "Please provide your email address.")
    .disabled(false)
```

---

## Part 2: Form Field Types

`FormFieldType` is an enum with a variant for every common data type. Each variant carries type-specific parameters that control parsing and validation.

### Text fields

```rust
// General text input with length constraints and optional whitespace stripping
FormFieldType::Char {
    min_length: Some(2),
    max_length: Some(100),
    strip: true,  // trims leading/trailing whitespace
}

// Email address (validates format with regex)
FormFieldType::Email

// URL (must start with http:// or https://)
FormFieldType::Url

// Slug (letters, numbers, hyphens, underscores only)
FormFieldType::Slug

// IPv4 or IPv6 address
FormFieldType::IpAddress

// Validated against a custom regular expression
FormFieldType::Regex {
    regex: r"^[A-Z]{3}\d{3}$".to_string(),
}
```

### Numeric fields

```rust
// Integer with optional bounds
FormFieldType::Integer {
    min_value: Some(0),
    max_value: Some(150),
}

// Floating-point with optional bounds
FormFieldType::Float {
    min_value: Some(0.0),
    max_value: Some(99999.99),
}

// Fixed-precision decimal (total digits and decimal places)
FormFieldType::Decimal {
    max_digits: 10,
    decimal_places: 2,
}
```

### Temporal fields

```rust
// Date: expects YYYY-MM-DD
FormFieldType::Date

// DateTime: expects YYYY-MM-DDTHH:MM:SS (or shorter variants)
FormFieldType::DateTime

// Time: expects HH:MM:SS or HH:MM
FormFieldType::Time

// Duration: expects HH:MM:SS, MM:SS, or plain seconds
FormFieldType::Duration
```

### Boolean fields

```rust
// Accepts "true", "1", "yes", "on" as true; everything else is false
FormFieldType::Boolean

// Like Boolean but also accepts "null", "none", "unknown" as Value::Null
FormFieldType::NullBoolean
```

### Choice fields

```rust
// Single selection from a list of (value, display_label) pairs
FormFieldType::Choice {
    choices: vec![
        ("draft".into(), "Draft".into()),
        ("published".into(), "Published".into()),
        ("archived".into(), "Archived".into()),
    ],
}

// Multiple selection (values submitted as comma-separated)
FormFieldType::MultipleChoice {
    choices: vec![
        ("rust".into(), "Rust".into()),
        ("python".into(), "Python".into()),
        ("typescript".into(), "TypeScript".into()),
    ],
}

// Choice with a coercion function to convert the string value
FormFieldType::TypedChoice {
    choices: vec![
        ("1".into(), "Low".into()),
        ("2".into(), "Medium".into()),
        ("3".into(), "High".into()),
    ],
    coerce: |s| {
        s.parse::<i64>()
            .map(Value::Int)
            .map_err(|e| DjangoError::BadRequest(e.to_string()))
    },
}
```

### File and other fields

```rust
// File upload with size and extension constraints
FormFieldType::File {
    max_size: Some(5_000_000),  // 5 MB
    allowed_extensions: vec!["pdf".into(), "doc".into(), "docx".into()],
}

// Image upload (validates image file extensions)
FormFieldType::Image

// UUID
FormFieldType::Uuid

// Arbitrary JSON
FormFieldType::Json
```

### Complete field type reference

| Category | Types | Description |
|---|---|---|
| Text | `Char`, `Email`, `Url`, `Slug`, `IpAddress`, `Regex` | String-based fields with format validation |
| Numeric | `Integer`, `Float`, `Decimal` | Number fields with optional bounds |
| Temporal | `Date`, `DateTime`, `Time`, `Duration` | Date and time fields with format parsing |
| Boolean | `Boolean`, `NullBoolean` | True/false (and nullable) fields |
| Choice | `Choice`, `MultipleChoice`, `TypedChoice` | Selection from predefined options |
| File | `File`, `Image` | Upload fields with extension and size validation |
| Other | `Uuid`, `Json` | Specialized structured data fields |

---

## Part 3: Binding Data and Validation

Before a form can be validated, it must be **bound** to data. In django-rs, form data arrives as a `QueryDict` -- a dictionary parsed from URL-encoded form bodies or query strings.

### Binding data

```rust
use django_rs_forms::form::{BaseForm, Form};
use django_rs_forms::fields::{FormFieldDef, FormFieldType};
use django_rs_http::QueryDict;

let mut form = BaseForm::new(vec![
    FormFieldDef::new("name", FormFieldType::Char {
        min_length: Some(2),
        max_length: Some(100),
        strip: true,
    }),
    FormFieldDef::new("email", FormFieldType::Email),
]);

// Parse URL-encoded data into a QueryDict
let data = QueryDict::parse("name=Alice&email=alice@example.com");

// Bind the data to the form
form.bind(&data);

// The form is now bound
assert!(form.is_bound());
```

A form that has not been bound always returns `false` from `is_valid()`:

```rust
let mut form = contact_form();
assert!(!form.is_bound());
assert!(!form.is_valid().await);  // always false when unbound
```

### Validation

Validation in django-rs is a two-phase pipeline, and it is **async** because cross-field validation may require database access.

**Phase 1: Field-level validation.** When you call `form.is_valid().await`, each field goes through:

1. **Required check** -- If `required` is `true` and the value is empty or missing, validation fails with "This field is required."
2. **Type coercion** -- The raw string is parsed into the appropriate `Value` type (`Value::Int`, `Value::String`, `Value::Date`, etc.)
3. **Type-specific constraints** -- Bounds like `min_length`, `max_value`, allowed choices, regex patterns, etc.
4. **Custom validators** -- Any additional validators attached via `.validator()`

Errors accumulate across all fields. The framework does not short-circuit on the first error, so users see every problem at once.

**Phase 2: Form-level cross-field validation.** After all fields pass individually, the `clean()` method is called. The default implementation does nothing, but you can override it for cross-field validation by implementing the `Form` trait.

### Working with validation results

```rust
use django_rs_forms::form::{BaseForm, Form};
use django_rs_forms::fields::{FormFieldDef, FormFieldType};
use django_rs_http::QueryDict;

let mut form = BaseForm::new(vec![
    FormFieldDef::new("name", FormFieldType::Char {
        min_length: Some(2),
        max_length: Some(100),
        strip: true,
    })
    .required(true),

    FormFieldDef::new("email", FormFieldType::Email)
        .required(true),

    FormFieldDef::new("age", FormFieldType::Integer {
        min_value: Some(18),
        max_value: Some(120),
    })
    .required(false)
    .label("Age"),
]);

// Bind some invalid data
let data = QueryDict::parse("name=A&email=not-an-email&age=10");
form.bind(&data);

if form.is_valid().await {
    // Access cleaned, typed data
    let cleaned = form.cleaned_data();
    let name = cleaned.get("name");   // Some(Value::String("A"))
    let email = cleaned.get("email"); // Some(Value::String("..."))
    let age = cleaned.get("age");     // Some(Value::Int(10))
} else {
    // Inspect errors per field
    let errors = form.errors();
    // errors["name"]  = ["Ensure this value has at least 2 characters (it has 1)."]
    // errors["email"] = ["Enter a valid email address."]
    // errors["age"]   = ["Ensure this value is greater than or equal to 18."]

    for (field, messages) in errors {
        println!("{field}:");
        for msg in messages {
            println!("  - {msg}");
        }
    }
}
```

### Custom error messages

Override the default error messages for specific error codes:

```rust
FormFieldDef::new("name", FormFieldType::Char {
    min_length: None,
    max_length: None,
    strip: false,
})
.required(true)
.error_message("required", "Please tell us your name.")
```

Now if the field is left empty, the error message is "Please tell us your name." instead of the default "This field is required."

### Form prefixes

When you need multiple forms on the same page, use prefixes to namespace the HTML `name` attributes:

```rust
let mut form = BaseForm::new(vec![
    FormFieldDef::new("name", FormFieldType::Char {
        min_length: None, max_length: None, strip: false,
    }),
]).with_prefix("billing");

// The form expects prefixed field names
let data = QueryDict::parse("billing-name=Alice");
form.bind(&data);
assert!(form.is_valid().await);
```

### Initial values

Set default values that appear in the form before the user submits anything:

```rust
use std::collections::HashMap;
use django_rs_db::value::Value;

let mut initial = HashMap::new();
initial.insert("name".to_string(), Value::String("Default Name".into()));

let form = contact_form().with_initial(initial);
```

### Disabled fields

Disabled fields are rendered but not editable. They use their initial value and skip validation entirely:

```rust
FormFieldDef::new("status", FormFieldType::Char {
    min_length: None,
    max_length: None,
    strip: false,
})
.disabled(true)
.initial(Value::String("active".into()))
```

---

## Part 4: Form Processing in Views

The standard pattern for form processing follows the same GET/POST cycle as Django: on GET, display an empty form; on POST, validate the data and either redirect on success or re-render with errors.

### Manual approach

```rust
use django_rs_views::views::form_view::{
    bind_form_from_request, extract_post_data,
    form_context_to_json, form_errors, cleaned_data_as_strings,
};
use django_rs_http::{HttpRequest, HttpResponse};

async fn contact_view(request: &HttpRequest) -> HttpResponse {
    if request.method() == http::Method::POST {
        // Bind form data from the request body
        let mut form = contact_form();
        bind_form_from_request(&mut form, request);

        if form.is_valid().await {
            // Process the valid data
            let data = cleaned_data_as_strings(&form);
            let name = data.get("name").unwrap();
            let email = data.get("email").unwrap();

            // ... save to database, send email, etc.

            HttpResponse::redirect("/contact/thanks/")
        } else {
            // Re-render with errors
            let errors = form_errors(&form);
            let context = form.as_context();
            let json_ctx = form_context_to_json(&context);

            // Render template with form context and errors
            HttpResponse::ok("Form has errors")  // simplified
        }
    } else {
        // GET: display empty form
        let form = contact_form();
        let context = form.as_context();
        let json_ctx = form_context_to_json(&context);

        // Render template with empty form context
        HttpResponse::ok("Empty form")  // simplified
    }
}
```

The key helper functions from `django_rs_views::views::form_view` are:

| Function | Description |
|---|---|
| `extract_post_data(&request)` | Parses the request body as URL-encoded form data into a `QueryDict` |
| `bind_form_from_request(&mut form, &request)` | Extracts POST data and binds it to the form in one call |
| `form_errors(&form)` | Returns the form's validation errors as a `HashMap<String, Vec<String>>` |
| `cleaned_data_as_strings(&form)` | Returns cleaned data with all values converted to strings |
| `form_context_to_json(&context)` | Converts a form context map to `serde_json::Value` for template rendering |

### Using FormView

For the common case, django-rs provides `FormView` -- a generic view that handles the GET/POST cycle automatically:

```rust
use std::sync::Arc;
use django_rs_views::views::FormView;
use django_rs_forms::form::BaseForm;
use django_rs_forms::fields::{FormFieldDef, FormFieldType};
use django_rs_forms::widgets::WidgetType;

let view = FormView::new("contact.html", "/contact/thanks/")
    .form_factory(Arc::new(|| {
        BaseForm::new(vec![
            FormFieldDef::new("name", FormFieldType::Char {
                min_length: Some(1),
                max_length: Some(100),
                strip: true,
            })
            .required(true)
            .label("Your Name"),

            FormFieldDef::new("email", FormFieldType::Email)
                .required(true)
                .label("Email Address"),

            FormFieldDef::new("message", FormFieldType::Char {
                min_length: Some(10),
                max_length: Some(2000),
                strip: true,
            })
            .required(true)
            .label("Message")
            .widget(WidgetType::Textarea),
        ])
    }))
    .initial("name", "")
    .initial("email", "");
```

`FormView` dispatches requests as follows:

- **GET / HEAD** -- Creates a fresh form from the factory, calls `as_context()`, and renders the template
- **POST** -- Binds the POST data, validates, then calls `form_valid()` (which redirects to `success_url`) or `form_invalid()` (which re-renders with errors)
- **Other methods** -- Returns 405 Method Not Allowed

### The template

In your `contact.html` template, you can iterate over the form fields:

```html
{% extends "base.html" %}

{% block content %}
<h1>Contact Us</h1>

<form method="post">
    {% csrf_token %}

    {% for field in form.fields %}
    <div class="field{% if field.errors %} has-error{% endif %}">
        {{ field.label_tag }}
        {{ field.html }}
        {% if field.help_text %}
            <small>{{ field.help_text }}</small>
        {% endif %}
        {{ field.errors }}
    </div>
    {% endfor %}

    <button type="submit">Send Message</button>
</form>
{% endblock %}
```

---

## Part 5: Widgets

Widgets control how a form field is rendered as HTML. Every `FormFieldType` has a sensible default widget, but you can override it.

### Default widget mapping

| FormFieldType | Default Widget |
|---|---|
| `Char` | `TextInput` |
| `Integer`, `Float`, `Decimal` | `NumberInput` |
| `Boolean` | `CheckboxInput` |
| `NullBoolean` | `Select` |
| `Date` | `DateInput` |
| `DateTime` | `DateTimeInput` |
| `Time` | `TimeInput` |
| `Duration` | `TextInput` |
| `Email` | `EmailInput` |
| `Url` | `UrlInput` |
| `Uuid`, `Slug`, `IpAddress` | `TextInput` |
| `Choice`, `TypedChoice` | `Select` |
| `MultipleChoice` | `SelectMultiple` |
| `File`, `Image` | `FileInput` |
| `Json` | `Textarea` |
| `Regex` | `TextInput` |

### Available widget types

django-rs provides 17 built-in widget types in `WidgetType`:

```rust
use django_rs_forms::widgets::WidgetType;

WidgetType::TextInput           // <input type="text">
WidgetType::NumberInput         // <input type="number">
WidgetType::EmailInput          // <input type="email">
WidgetType::UrlInput            // <input type="url">
WidgetType::PasswordInput       // <input type="password">
WidgetType::HiddenInput         // <input type="hidden">
WidgetType::Textarea            // <textarea>
WidgetType::CheckboxInput       // <input type="checkbox">
WidgetType::Select              // <select>
WidgetType::SelectMultiple      // <select multiple>
WidgetType::RadioSelect         // group of <input type="radio">
WidgetType::CheckboxSelectMultiple  // group of <input type="checkbox">
WidgetType::DateInput           // <input type="date">
WidgetType::DateTimeInput       // <input type="datetime-local">
WidgetType::TimeInput           // <input type="time">
WidgetType::FileInput           // <input type="file">
WidgetType::ClearableFileInput  // <input type="file"> with a clear checkbox
```

### Overriding the default widget

Use the `.widget()` builder method to override a field's widget:

```rust
use django_rs_forms::widgets::WidgetType;

// Render a Char field as a textarea instead of a text input
FormFieldDef::new("bio", FormFieldType::Char {
    min_length: None,
    max_length: Some(5000),
    strip: true,
})
.widget(WidgetType::Textarea)

// Render a Choice field as radio buttons instead of a dropdown
FormFieldDef::new("priority", FormFieldType::Choice {
    choices: vec![
        ("low".into(), "Low".into()),
        ("medium".into(), "Medium".into()),
        ("high".into(), "High".into()),
    ],
})
.widget(WidgetType::RadioSelect)

// Use a password input for sensitive text
FormFieldDef::new("secret", FormFieldType::Char {
    min_length: Some(8),
    max_length: Some(128),
    strip: false,
})
.widget(WidgetType::PasswordInput)
```

### Widget rendering

Widgets render themselves as HTML strings. The `BoundField` type (created during form rendering) pairs a field definition with its current data, errors, and widget:

```rust
use django_rs_forms::widgets::{TextInput, Widget};
use std::collections::HashMap;

let widget = TextInput;
let html = widget.render(
    "username",                          // name attribute
    &Some("alice".to_string()),          // current value
    &HashMap::from([                     // extra attributes
        ("class".to_string(), "form-control".to_string()),
        ("id".to_string(), "id_username".to_string()),
    ]),
);
// Produces: <input type="text" name="username" value="alice" class="form-control" id="id_username" />
```

### Bound fields and template rendering

When you call `form.as_context()`, the form produces a template-ready context with a `fields` list. Each entry contains the rendered HTML, label tag, errors, and metadata:

```rust
let mut form = contact_form();
let data = QueryDict::parse(
    "name=Alice&email=alice@example.com&subject=Hello&message=This is my message to you"
);
form.bind(&data);
form.is_valid().await;

let context = form.as_context();
// context["fields"] is a list where each item has:
//   - "name"      : field name
//   - "label"     : human-readable label
//   - "help_text" : help text
//   - "html"      : rendered widget HTML
//   - "label_tag" : rendered <label> element
//   - "errors"    : rendered error list as <ul class="errorlist">
//   - "required"  : boolean
```

You can also get bound fields directly for finer control:

```rust
let bound_fields = form.bound_fields();
for bf in &bound_fields {
    println!("Label: {}", bf.label_tag());
    println!("Widget: {}", bf.render(&HashMap::new()));
    if bf.has_errors() {
        println!("Errors: {}", bf.errors_as_ul());
    }
}
```

---

## Part 6: FormSets

When you need several instances of the same form on one page -- such as adding multiple phone numbers or line items in an invoice -- use a `FormSet`.

### Creating a formset

```rust
use django_rs_forms::formset::{FormSet, create_formset};
use django_rs_forms::form::BaseForm;
use django_rs_forms::fields::{FormFieldDef, FormFieldType};

fn phone_form() -> BaseForm {
    BaseForm::new(vec![
        FormFieldDef::new("label", FormFieldType::Char {
            min_length: None,
            max_length: Some(50),
            strip: true,
        })
        .label("Label (e.g., Home, Work)"),

        FormFieldDef::new("number", FormFieldType::Char {
            min_length: Some(7),
            max_length: Some(20),
            strip: true,
        })
        .label("Phone Number"),
    ])
}

// Create a formset with 2 initial forms and 1 extra blank form
let formset = create_formset(
    |_index| Box::new(phone_form()),
    2,    // initial_count: 2 pre-populated forms
    1,    // extra: 1 additional blank form
);

assert_eq!(formset.total_form_count(), 3);
```

### Configuring the formset

```rust
let formset = FormSet::new(vec![
    Box::new(phone_form()),
    Box::new(phone_form()),
    Box::new(phone_form()),
])
.with_extra(1)
.with_min_num(1)        // at least 1 form must be submitted
.with_max_num(10)       // at most 10 forms allowed
.with_can_delete(true)  // allow marking forms for deletion
.with_can_order(true)   // allow reordering forms
.with_prefix("phones"); // namespace all HTML names
```

### The management form

Every formset includes a **management form** -- hidden inputs that tell the server how many forms to expect:

```rust
let html = formset.management_form_html();
// Produces hidden inputs for:
//   phones-TOTAL_FORMS
//   phones-INITIAL_FORMS
//   phones-MIN_NUM_FORMS
//   phones-MAX_NUM_FORMS
```

Always include the management form HTML in your template:

```html
<form method="post">
    {% csrf_token %}
    {{ formset.management_form }}

    {% for form in formset.forms %}
    <fieldset>
        {% for field in form.fields %}
            {{ field.label_tag }}
            {{ field.html }}
            {{ field.errors }}
        {% endfor %}
    </fieldset>
    {% endfor %}

    <button type="submit">Save All</button>
</form>
```

### Binding and validating a formset

```rust
use django_rs_http::QueryDict;

let mut formset = create_formset(
    |_| Box::new(phone_form()),
    2,
    0,
).with_prefix("phones");

// Bind form data (each form's fields are prefixed with "phones-{index}-")
let data = QueryDict::parse(
    "phones-0-label=Home&phones-0-number=5551234567\
     &phones-1-label=Work&phones-1-number=5559876543"
);
formset.bind(&data);

if formset.is_valid().await {
    for form in &formset.forms {
        let cleaned = form.cleaned_data();
        // Process each form's data...
    }
} else {
    // Check individual form errors
    for form in &formset.forms {
        if !form.errors().is_empty() {
            // Handle errors for this form
        }
    }
    // Check formset-level errors
    for error in formset.non_form_errors() {
        println!("Formset error: {error}");
    }
}
```

### Formset template context

Generate a context suitable for template rendering:

```rust
let context = formset.as_context();
// context["forms"]            - list of individual form contexts
// context["management_form"]  - rendered hidden inputs (SafeString)
// context["non_form_errors"]  - list of formset-level errors
// context["total_form_count"] - integer count
// context["can_delete"]       - boolean
// context["can_order"]        - boolean
```

---

## Part 7: CSRF Protection

django-rs includes a full CSRF protection system modeled after Django's. The `CsrfMiddleware` generates tokens, sets cookies, and validates state-changing requests.

### How it works

1. On **GET/HEAD/OPTIONS/TRACE** requests, the middleware sets a `csrftoken` cookie on the response if one is not already present.
2. On **POST/PUT/PATCH/DELETE** requests, the middleware checks for a valid CSRF token in either the `X-CSRFToken` header or the `csrfmiddlewaretoken` form field. The token must match the cookie value.
3. Requests without a valid token receive a **403 Forbidden** response.

Tokens are XOR-masked before being sent to the client to prevent BREACH attacks on compressed HTTPS responses.

### Setting up the middleware

```rust
use django_rs_auth::csrf::CsrfMiddleware;

let csrf = CsrfMiddleware::new();

// Configure options
let csrf = CsrfMiddleware {
    cookie_name: "csrftoken".to_string(),
    header_name: "X-CSRFToken".to_string(),
    cookie_secure: true,       // Require HTTPS
    cookie_httponly: false,     // Allow JavaScript access
    trusted_origins: vec![
        "https://mysite.example.com".to_string(),
    ],
    exempt_paths: std::collections::HashSet::new(),
};
```

### Generating and validating tokens

```rust
use django_rs_auth::csrf::{generate_csrf_token, mask_csrf_token, validate_csrf_token};

// Generate a cryptographically random token (64-char hex string)
let token = generate_csrf_token();

// Mask it for the HTML form (prevents BREACH attacks)
let masked = mask_csrf_token(&token);

// Validate a request token against the cookie token
let is_valid = validate_csrf_token(&masked, &token); // true
```

### Exempting paths

Some endpoints (webhooks, APIs with their own authentication) should be exempt from CSRF checking:

```rust
let mut csrf = CsrfMiddleware::new();
csrf.add_exempt_path("/api/webhook/");
csrf.add_exempt_path("/api/stripe/callback/");
```

### In templates

Use the `{% csrf_token %}` template tag in every form that submits via POST:

```html
<form method="post" action="/contact/">
    {% csrf_token %}
    <!-- form fields here -->
    <button type="submit">Submit</button>
</form>
```

This renders a hidden input:

```html
<input type="hidden" name="csrfmiddlewaretoken" value="a1b2c3...masked_token...">
```

### For AJAX requests

If you are submitting forms via JavaScript, include the token in the `X-CSRFToken` header:

```javascript
fetch("/api/endpoint/", {
    method: "POST",
    headers: {
        "X-CSRFToken": getCookie("csrftoken"),
        "Content-Type": "application/json",
    },
    body: JSON.stringify(data),
});
```

---

## Part 8: Complete Form Processing Example

Let us put everything together and build a working contact form from start to finish. This example covers form definition, view handling, template rendering, validation, and error display.

### Step 1: Define the form

Create a file `src/forms.rs`:

```rust
// src/forms.rs

use django_rs_forms::form::BaseForm;
use django_rs_forms::fields::{FormFieldDef, FormFieldType};
use django_rs_forms::widgets::WidgetType;

/// Creates a contact form with name, email, subject, and message fields.
pub fn contact_form() -> BaseForm {
    BaseForm::new(vec![
        FormFieldDef::new("name", FormFieldType::Char {
            min_length: Some(2),
            max_length: Some(100),
            strip: true,
        })
        .required(true)
        .label("Your Name")
        .help_text("Enter your full name")
        .error_message("required", "Please enter your name."),

        FormFieldDef::new("email", FormFieldType::Email)
            .required(true)
            .label("Email Address")
            .help_text("We will use this to reply to you")
            .error_message("required", "Please provide an email address."),

        FormFieldDef::new("subject", FormFieldType::Choice {
            choices: vec![
                ("general".into(), "General Inquiry".into()),
                ("support".into(), "Technical Support".into()),
                ("feedback".into(), "Feedback".into()),
                ("other".into(), "Other".into()),
            ],
        })
        .required(true)
        .label("Subject"),

        FormFieldDef::new("message", FormFieldType::Char {
            min_length: Some(10),
            max_length: Some(2000),
            strip: true,
        })
        .required(true)
        .label("Message")
        .help_text("Minimum 10 characters")
        .widget(WidgetType::Textarea),

        FormFieldDef::new("subscribe", FormFieldType::Boolean)
            .required(false)
            .label("Subscribe to newsletter"),
    ])
}
```

### Step 2: Write the view

```rust
// src/views.rs

use std::sync::Arc;
use std::collections::HashMap;

use django_rs_forms::form::Form;
use django_rs_http::{HttpRequest, HttpResponse, BoxFuture};
use django_rs_views::views::form_view::bind_form_from_request;
use django_rs_template::engine::Engine;
use django_rs_template::context::{Context, ContextValue};

use crate::forms::contact_form;

pub fn contact_view(engine: Arc<Engine>) -> Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> {
    Arc::new(move |request: HttpRequest| {
        let engine = engine.clone();
        Box::pin(async move {
            if request.method() == &http::Method::POST {
                // POST: bind and validate
                let mut form = contact_form();
                bind_form_from_request(&mut form, &request);

                if form.is_valid().await {
                    let cleaned = form.cleaned_data();
                    let name = cleaned.get("name").unwrap();
                    let email = cleaned.get("email").unwrap();

                    // In a real application, save to database or send email here
                    println!("Contact from {name} ({email})");

                    // Redirect to success page
                    HttpResponse::redirect("/contact/thanks/")
                } else {
                    // Re-render with errors
                    let form_ctx = form.as_context();
                    let mut ctx = Context::new();
                    for (key, value) in form_ctx {
                        ctx.set(&key, value);
                    }
                    ctx.set("title", ContextValue::from("Contact Us"));

                    match engine.render_to_string("contact.html", &mut ctx) {
                        Ok(html) => HttpResponse::ok(html),
                        Err(e) => HttpResponse::server_error(format!("Template error: {e}")),
                    }
                }
            } else {
                // GET: display empty form
                let form = contact_form();
                let form_ctx = form.as_context();
                let mut ctx = Context::new();
                for (key, value) in form_ctx {
                    ctx.set(&key, value);
                }
                ctx.set("title", ContextValue::from("Contact Us"));

                match engine.render_to_string("contact.html", &mut ctx) {
                    Ok(html) => HttpResponse::ok(html),
                    Err(e) => HttpResponse::server_error(format!("Template error: {e}")),
                }
            }
        })
    })
}

pub fn thanks_view() -> Arc<dyn Fn(HttpRequest) -> BoxFuture + Send + Sync> {
    Arc::new(|_request: HttpRequest| {
        Box::pin(async {
            HttpResponse::ok(
                "<h1>Thank You!</h1>\
                 <p>Your message has been received. We will get back to you soon.</p>\
                 <p><a href=\"/contact/\">Send another message</a></p>"
            )
        })
    })
}
```

### Step 3: Create the template

```html
<!-- templates/contact.html -->
{% extends "base.html" %}

{% block title %}{{ title }}{% endblock %}

{% block content %}
<h1>{{ title }}</h1>

{% if non_field_errors %}
<div class="alert alert-danger">
    <ul>
    {% for error in non_field_errors %}
        <li>{{ error }}</li>
    {% endfor %}
    </ul>
</div>
{% endif %}

<form method="post">
    {% csrf_token %}

    {% for field in fields %}
    <div class="form-group{% if field.errors %} has-error{% endif %}">
        {{ field.label_tag }}
        {{ field.html }}

        {% if field.help_text %}
            <p class="help-text">{{ field.help_text }}</p>
        {% endif %}

        {% if field.errors %}
            {{ field.errors }}
        {% endif %}
    </div>
    {% endfor %}

    <button type="submit" class="btn btn-primary">Send Message</button>
</form>
{% endblock %}
```

### Step 4: Wire up the URL patterns

```rust
// src/main.rs

use std::sync::Arc;

use django_rs_http::urls::pattern::path;
use django_rs_http::urls::resolver::{root, URLEntry};
use django_rs_template::engine::Engine;

mod forms;
mod views;

#[tokio::main]
async fn main() {
    let engine = Arc::new(Engine::new());
    // In production, load templates from the filesystem:
    // engine.set_dirs(vec![PathBuf::from("templates/")]);

    let patterns = vec![
        URLEntry::Pattern(
            path("contact/", views::contact_view(engine.clone()), Some("contact")).unwrap()
        ),
        URLEntry::Pattern(
            path("contact/thanks/", views::thanks_view(), Some("contact-thanks")).unwrap()
        ),
    ];

    let resolver = root(patterns).unwrap();

    // Build the Axum app and start the server
    // (See Tutorial 1 for the full server setup)
    let resolver = Arc::new(resolver);
    let resolver_handler = Arc::clone(&resolver);
    let app = axum::Router::new().fallback(move |req: axum::extract::Request| {
        let resolver = Arc::clone(&resolver_handler);
        async move {
            let (parts, body) = req.into_parts();
            let body_bytes = axum::body::to_bytes(body, usize::MAX)
                .await
                .unwrap_or_default()
                .to_vec();
            let mut django_request = django_rs_http::HttpRequest::from_axum(parts, body_bytes);
            let path = django_request.path().trim_start_matches('/').to_string();

            match resolver.resolve(&path) {
                Ok(resolver_match) => {
                    let handler = resolver_match.func.clone();
                    django_request.set_resolver_match(resolver_match);
                    let response = handler(django_request).await;
                    axum::response::IntoResponse::into_response(response)
                }
                Err(_) => {
                    let response = django_rs_http::HttpResponse::not_found("404 Not Found");
                    axum::response::IntoResponse::into_response(response)
                }
            }
        }
    });

    let addr = "127.0.0.1:8000";
    println!("Starting server at http://{addr}/");
    println!("Visit http://{addr}/contact/ to see the form");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### What happens when you submit the form

1. The user visits `/contact/` (GET). The view creates an empty `BaseForm`, calls `as_context()` to generate template variables, and renders `contact.html` with empty fields.
2. The user fills in the form and clicks "Send Message" (POST).
3. `bind_form_from_request` extracts the URL-encoded POST body and calls `form.bind(&data)`.
4. `form.is_valid().await` runs the two-phase validation pipeline:
   - Each field's raw string is parsed, coerced, and validated against its `FormFieldType` constraints.
   - If all fields pass, the async `clean()` method runs for cross-field validation.
5. If valid, the view processes the cleaned data and redirects to `/contact/thanks/`.
6. If invalid, the view re-renders the template. The form context now includes error messages for each invalid field, and the previously submitted values are preserved in the widgets.

---

## ModelForm: Auto-generating Forms from Models

When you have an ORM model, you rarely want to define every form field by hand. `ModelFormConfig` generates `FormFieldDef` instances directly from your model's metadata.

### Basic usage

```rust
use django_rs_forms::model_form::{ModelFormConfig, ModelFormFields, generate_form_fields};
use django_rs_forms::form::BaseForm;

// Assuming you have a model with registered metadata:
let config = ModelFormConfig::new(&POST_META);
let fields = generate_form_fields(&config);
let form = BaseForm::new(fields);
```

`generate_form_fields` automatically:

- **Skips** primary key fields, non-editable fields, and relational fields (foreign keys, many-to-many)
- **Maps** each model field type to the corresponding `FormFieldType` (e.g., `CharField` becomes `Char`, `EmailField` becomes `Email`, `DateField` becomes `Date`)
- **Sets** `required` based on whether the model field is nullable and has a default
- **Copies** labels, help texts, and defaults from the model metadata

### Field inclusion and exclusion

```rust
// Include only specific fields
let config = ModelFormConfig::new(&POST_META)
    .with_fields(ModelFormFields::Include(vec![
        "title".into(),
        "content".into(),
        "published".into(),
    ]));

// Exclude specific fields (include everything else)
let config = ModelFormConfig::new(&POST_META)
    .with_fields(ModelFormFields::Exclude(vec![
        "slug".into(),
        "views".into(),
    ]));
```

### Overriding widgets, labels, and help texts

```rust
use django_rs_forms::widgets::WidgetType;

let config = ModelFormConfig::new(&POST_META)
    .with_fields(ModelFormFields::Include(vec![
        "title".into(),
        "content".into(),
        "email".into(),
    ]))
    .with_widget("content", WidgetType::Textarea)
    .with_label("title", "Article Title")
    .with_label("email", "Author's Email")
    .with_help_text("title", "Keep it under 200 characters");

let fields = generate_form_fields(&config);
let form = BaseForm::new(fields);
```

### How field types are mapped

| Model FieldType | Form FormFieldType |
|---|---|
| `CharField`, `TextField` | `Char { max_length, strip: true }` |
| `IntegerField`, `BigIntegerField`, `SmallIntegerField` | `Integer` |
| `FloatField` | `Float` |
| `DecimalField { max_digits, decimal_places }` | `Decimal { max_digits, decimal_places }` |
| `BooleanField` | `Boolean` |
| `DateField` | `Date` |
| `DateTimeField` | `DateTime` |
| `TimeField` | `Time` |
| `DurationField` | `Duration` |
| `UuidField` | `Uuid` |
| `EmailField` | `Email` |
| `UrlField` | `Url` |
| `SlugField` | `Slug` |
| `IpAddressField` | `IpAddress` |
| `JsonField` | `Json` |

---

## Comparison with Django

If you are coming from Django, here is how the key form concepts map:

| Django (Python) | django-rs (Rust) |
|---|---|
| `from django import forms` | `use django_rs_forms::form::BaseForm;` |
| `forms.CharField(max_length=100)` | `FormFieldType::Char { max_length: Some(100), ... }` |
| `forms.EmailField()` | `FormFieldType::Email` |
| `forms.IntegerField(min_value=0)` | `FormFieldType::Integer { min_value: Some(0), ... }` |
| `forms.ChoiceField(choices=[...])` | `FormFieldType::Choice { choices: vec![...] }` |
| `widget=forms.Textarea` | `.widget(WidgetType::Textarea)` |
| `form = MyForm(request.POST)` | `form.bind(&data)` |
| `form.is_valid()` | `form.is_valid().await` (async!) |
| `form.cleaned_data['name']` | `form.cleaned_data().get("name")` |
| `form.errors` | `form.errors()` |
| `{% csrf_token %}` | `{% csrf_token %}` (identical) |
| `formset_factory(MyForm, extra=3)` | `create_formset(\|_\| ..., initial, extra)` |
| `ModelForm` with `class Meta` | `ModelFormConfig::new(&META).with_fields(...)` |

The main structural difference is that django-rs validation is **async**. This is because cross-field validation often requires database access (checking uniqueness, verifying foreign keys), and making validation async-first avoids blocking the thread pool.

---

## Summary

In this tutorial you learned how to:

1. **Define forms** using `BaseForm` and `FormFieldDef` with the builder pattern
2. **Choose field types** from the full range of `FormFieldType` variants for text, numbers, dates, choices, files, and more
3. **Bind data** from `QueryDict` to a form and understand bound vs. unbound state
4. **Validate** forms with the async two-phase pipeline (field-level, then form-level)
5. **Handle errors** by inspecting `form.errors()` and rendering error lists
6. **Render widgets** as HTML using the built-in widget types, with override support
7. **Process forms in views** following the GET/POST pattern, both manually and with `FormView`
8. **Protect against CSRF** with `CsrfMiddleware`, token generation, and the `{% csrf_token %}` template tag
9. **Auto-generate forms** from model metadata with `ModelFormConfig`
10. **Manage multiple forms** on one page with `FormSet`

In the next tutorial, [Tutorial 5: Testing Your App](./05-testing.md), you will write tests for the models, views, and forms you have built, using the django-rs test framework and test client.
