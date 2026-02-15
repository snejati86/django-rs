//! # django-rs-macros
//!
//! Procedural macros for the django-rs framework. Provides derive macros and attribute
//! macros for models, views, forms, URL routing, and other framework components.
//!
//! This crate is independent of all other django-rs crates because proc-macro crates
//! cannot depend on crates that use them. The generated code references types from
//! `django_rs_db`, `django_rs_forms`, etc. by their full paths.
//!
//! ## Derive Macros
//!
//! - **`#[derive(Model)]`** — Generates a `django_rs_db::model::Model` implementation
//! - **`#[derive(Form)]`** — Generates form field definitions and a `BaseForm` constructor
//! - **`#[derive(Admin)]`** — Generates admin configuration methods
//!
//! ## Function-like Macros
//!
//! - **`urls!`** — Defines URL routing patterns with a Django-like DSL
//!
//! ## Attribute Macros
//!
//! - **`#[management_command]`** — Wraps a function as a management command
//! - **`#[middleware]`** — Wraps a struct impl as middleware
//! - **`#[signal_handler]`** — Registers a signal handler

extern crate proc_macro;

mod admin;
mod form;
mod model;
mod string_list;
mod urls;
mod utils;

use proc_macro::TokenStream;

/// Derive macro for implementing the `Model` trait.
///
/// # Struct-level attributes (`#[model(...)]`)
///
/// - `table = "table_name"` — Database table name (defaults to `{app}_{struct_lowercase}`)
/// - `app = "app_label"` — Application label
/// - `verbose_name = "..."` — Human-readable name
/// - `verbose_name_plural = "..."` — Human-readable plural name
/// - `abstract_model` — No database table is created
/// - `ordering = ["-created_at", "name"]` — Default query ordering
///
/// # Field-level attributes (`#[field(...)]`)
///
/// - `primary_key` — Marks as primary key
/// - `auto` — Auto-increment
/// - `max_length = N` — Maximum character length
/// - `blank` — Allows empty values
/// - `null` — Allows NULL
/// - `default = "value"` — Default value
/// - `db_index` — Create database index
/// - `unique` — Unique constraint
/// - `verbose_name = "..."` — Human-readable name
/// - `help_text = "..."` — Help text
/// - `foreign_key = "table"` — Foreign key relation
/// - `on_delete = "cascade|protect|set_null|set_default|do_nothing"`
/// - `auto_now` — Update timestamp on save
/// - `auto_now_add` — Set timestamp on creation
/// - `editable = false` — Not editable in forms
/// - `db_column = "col"` — Override database column name
///
/// # Example
///
/// ```ignore
/// #[derive(Model)]
/// #[model(table = "blog_post", app = "blog")]
/// pub struct Post {
///     #[field(primary_key, auto)]
///     pub id: i64,
///
///     #[field(max_length = 200)]
///     pub title: String,
///
///     #[field(blank, default = "")]
///     pub subtitle: Option<String>,
/// }
/// ```
#[proc_macro_derive(Model, attributes(model, field))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    model::derive_model_impl(input).into()
}

/// Derive macro for generating form field definitions.
///
/// # Struct-level attributes (`#[form(...)]`)
///
/// - `action = "/url/"` — Form action URL
/// - `method = "post"` — HTTP method
///
/// # Field-level attributes (`#[form_field(...)]`)
///
/// - `required` — Whether the field is required (default: true for non-Option types)
/// - `field_type = "email"` — Override the form field type
/// - `max_length = N` — Maximum character length
/// - `min_length = N` — Minimum character length
/// - `label = "..."` — Human-readable label
/// - `help_text = "..."` — Help text
/// - `widget = "textarea"` — Widget type override
/// - `choices = ["val1:Label 1", "val2:Label 2"]` — Choice options
/// - `min_value = N` — Minimum numeric value
/// - `max_value = N` — Maximum numeric value
///
/// # Example
///
/// ```ignore
/// #[derive(Form)]
/// #[form(action = "/submit/", method = "post")]
/// pub struct ContactForm {
///     #[form_field(required, max_length = 100, label = "Your Name")]
///     pub name: String,
///
///     #[form_field(field_type = "email", required)]
///     pub email: String,
/// }
/// ```
#[proc_macro_derive(Form, attributes(form, form_field))]
pub fn derive_form(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    form::derive_form_impl(input).into()
}

/// Derive macro for generating admin panel configuration.
///
/// # Struct-level attributes (`#[admin(...)]`)
///
/// - `list_display = ["field1", "field2"]` — Fields shown in list view
/// - `list_filter = ["field1"]` — Fields available as filters
/// - `search_fields = ["field1", "field2"]` — Fields searchable in admin
/// - `ordering = ["-created_at"]` — Default list ordering
/// - `list_per_page = 25` — Items per page
/// - `readonly_fields = ["field1"]` — Read-only fields in edit view
/// - `list_display_links = ["field1"]` — Clickable fields in list view
/// - `list_editable = ["field1"]` — Inline-editable fields in list view
/// - `date_hierarchy = "created_at"` — Date drill-down navigation
///
/// # Example
///
/// ```ignore
/// #[derive(Admin)]
/// #[admin(
///     list_display = ["title", "author", "published"],
///     list_filter = ["published"],
///     search_fields = ["title", "body"],
///     ordering = ["-created_at"],
///     list_per_page = 25
/// )]
/// pub struct PostAdmin;
/// ```
#[proc_macro_derive(Admin, attributes(admin))]
pub fn derive_admin(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    admin::derive_admin_impl(input).into()
}

/// Macro for defining URL routing patterns using a Django-like DSL.
///
/// # Syntax
///
/// ```ignore
/// urls! {
///     "" => index_view,
///     "articles/<int:year>/" => article_year,
///     "articles/<slug:slug>/" => article_detail,
/// }
/// ```
///
/// ## Capture types
///
/// - `<int:name>` — Matches digits (`[0-9]+`)
/// - `<str:name>` — Matches any non-slash characters (`[^/]+`)
/// - `<slug:name>` — Matches slugs (`[-a-zA-Z0-9_]+`)
/// - `<uuid:name>` — Matches UUIDs
/// - `<path:name>` — Matches any path (`.+`)
/// - `<name>` — Same as `<str:name>`
#[proc_macro]
pub fn urls(input: TokenStream) -> TokenStream {
    let entries = syn::parse_macro_input!(input as urls::UrlEntries);
    urls::expand_urls(entries).into()
}

/// Attribute macro for defining management commands.
///
/// # Example
///
/// ```ignore
/// #[management_command(name = "import_data", help = "Import data from CSV")]
/// async fn handle(matches: &ArgMatches, settings: &Settings) -> Result<(), DjangoError> {
///     // Command implementation
/// }
/// ```
#[proc_macro_attribute]
pub fn management_command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_args = match darling::ast::NestedMeta::parse_meta_list(attr.into()) {
        Ok(v) => v,
        Err(e) => return TokenStream::from(darling::Error::from(e).write_errors()),
    };
    let func = syn::parse_macro_input!(item as syn::ItemFn);

    let opts = match <utils::ManagementCommandOpts as darling::FromMeta>::from_list(&attr_args) {
        Ok(o) => o,
        Err(e) => return e.write_errors().into(),
    };

    utils::expand_management_command(opts, func).into()
}

/// Attribute macro for defining middleware.
///
/// # Example
///
/// ```ignore
/// #[middleware]
/// impl MyMiddleware {
///     async fn process_request(&self, request: &mut HttpRequest) -> Option<HttpResponse> { ... }
///     async fn process_response(&self, request: &HttpRequest, response: HttpResponse) -> HttpResponse { ... }
/// }
/// ```
#[proc_macro_attribute]
pub fn middleware(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let impl_block = syn::parse_macro_input!(item as syn::ItemImpl);
    utils::expand_middleware(impl_block).into()
}

/// Attribute macro for registering signal handlers.
///
/// # Example
///
/// ```ignore
/// #[signal_handler(signal = "pre_save")]
/// fn on_pre_save(instance: &dyn std::any::Any) -> Option<String> {
///     // Signal handler implementation
/// }
/// ```
#[proc_macro_attribute]
pub fn signal_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_args = match darling::ast::NestedMeta::parse_meta_list(attr.into()) {
        Ok(v) => v,
        Err(e) => return TokenStream::from(darling::Error::from(e).write_errors()),
    };
    let func = syn::parse_macro_input!(item as syn::ItemFn);

    let opts = match <utils::SignalHandlerOpts as darling::FromMeta>::from_list(&attr_args) {
        Ok(o) => o,
        Err(e) => return e.write_errors().into(),
    };

    utils::expand_signal_handler(opts, func).into()
}
