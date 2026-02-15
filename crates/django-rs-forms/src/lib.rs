//! # django-rs-forms
//!
//! Forms framework for the django-rs framework. Provides the [`Form`](form::Form) trait,
//! [`BaseForm`](form::BaseForm) implementation, form field types with validation,
//! widgets for HTML rendering, model-backed form generation, and formset support.
//!
//! ## Design Principles
//!
//! This crate is designed with Rust's async/concurrency strengths in mind:
//! - **Async validation**: `is_valid()` and `clean()` are async, enabling
//!   database lookups for uniqueness checks without blocking threads.
//! - **Send + Sync**: All core traits require `Send + Sync`, allowing forms
//!   to be safely shared across async task boundaries and thread pools.
//! - **No global locks**: Form processing never serializes request handling.
//!
//! ## Modules
//!
//! - [`form`] - The [`Form`](form::Form) trait and [`BaseForm`](form::BaseForm) implementation
//! - [`fields`] - Form field definitions and type-level validation
//! - [`bound_field`] - Bound fields for template rendering
//! - [`widgets`] - Widget trait and 15+ built-in HTML widgets
//! - [`validation`] - The validation pipeline (`clean_fields`, `full_clean`)
//! - [`model_form`] - Model-backed form generation from ORM metadata
//! - [`formset`] - Formsets for managing collections of forms
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_forms::form::BaseForm;
//! use django_rs_forms::fields::{FormFieldDef, FormFieldType};
//! use django_rs_forms::form::Form;
//! use django_rs_http::QueryDict;
//!
//! let mut form = BaseForm::new(vec![
//!     FormFieldDef::new("name", FormFieldType::Char {
//!         min_length: Some(2),
//!         max_length: Some(100),
//!         strip: true,
//!     }),
//!     FormFieldDef::new("email", FormFieldType::Email),
//! ]);
//!
//! let data = QueryDict::parse("name=Alice&email=alice@example.com");
//! form.bind(&data);
//! // form.is_valid().await; // async validation
//! ```

// These clippy lints are intentionally allowed for the forms crate:
// - cast_precision_loss: i64 -> f64 casts are acceptable for numeric validation
// - cast_possible_truncation/wrap: usize -> i64 casts are acceptable for context values
// - result_large_err: DjangoError is the framework error type used consistently
// - needless_pass_by_value: Some API signatures follow Django's patterns
// - return_self_not_must_use: Builder pattern methods are self-documenting
// - use_self: Explicit type names are clearer in some contexts
// - doc_markdown: Backtick requirements for documentation items are too strict
// - missing_const_for_fn: Some functions could be const but readability is preferred
// - option_if_let_else: if-let is often clearer than map_or
// - trivially_copy_pass_by_ref: &WidgetType in some positions matches trait signatures
// - struct_excessive_bools: FormFieldDef has multiple boolean fields by design (mirrors Django)
// - single_match_else: match is clearer than if-let for parsing result types
// - if_not_else: negative conditions are sometimes clearer for validation logic
// - branches_sharing_code: shared return values in validation are intentional for clarity
// - format_push_string: format! with push_str is clearer for HTML generation
// - implicit_hasher: HashMap<String, _> is the standard usage, no need to generalize
// - assigning_clones: clone() is clearer than clone_from() for simple assignments
// - match_same_arms: separate arms document different conceptual cases
// - derivable_impls: explicit Default impls document expected behavior
// - unnecessary_map_or: map_or is idiomatic for Option chains
// - similar_names: field names like min_value/max_value are distinct in context
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::result_large_err)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::use_self)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::single_match_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::branches_sharing_code)]
#![allow(clippy::format_push_string)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::assigning_clones)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]

pub mod bound_field;
pub mod fields;
pub mod form;
pub mod formset;
pub mod model_form;
pub mod validation;
pub mod widgets;

// Re-export commonly used types at the crate root.
pub use fields::{FormFieldDef, FormFieldType};
pub use form::{BaseForm, Form};
pub use formset::FormSet;
pub use model_form::{ModelFormConfig, ModelFormFields};
pub use widgets::{Widget, WidgetType};
