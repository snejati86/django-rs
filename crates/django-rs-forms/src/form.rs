//! Form trait and `BaseForm` implementation.
//!
//! The [`Form`] trait is the core abstraction for all form types in the
//! framework. It supports async validation (for database uniqueness checks
//! and other I/O-bound validation), data binding from `QueryDict`, and
//! template context generation.
//!
//! [`BaseForm`] provides a concrete, general-purpose implementation of the
//! `Form` trait that can be constructed from a list of field definitions.
//!
//! This mirrors Django's `django.forms.Form` and `BaseForm`.

use std::collections::HashMap;

use async_trait::async_trait;

use django_rs_db::value::Value;
use django_rs_http::QueryDict;
use django_rs_template::context::ContextValue;

use crate::bound_field::BoundField;
use crate::fields::FormFieldDef;
use crate::validation;

/// The core form trait. All form types implement this.
///
/// Forms support async validation to allow hitting the database for
/// uniqueness checks and other I/O-bound validation during `is_valid()`.
/// All implementations must be `Send + Sync` to work safely across
/// async task boundaries and thread pools.
///
/// # Async Design
///
/// `is_valid()` and `clean()` are async because cross-field validation
/// commonly requires database access (e.g., checking that a username is
/// unique, verifying foreign key references). Making these operations
/// async-first avoids the need for `block_on` hacks and enables true
/// concurrent request handling.
#[async_trait]
pub trait Form: Send + Sync {
    /// Returns the form's field definitions.
    fn fields(&self) -> &[FormFieldDef];

    /// Returns the initial (default) values for fields.
    fn initial(&self) -> &HashMap<String, Value>;

    /// Returns the form prefix (for namespacing multiple forms on one page).
    fn prefix(&self) -> Option<&str>;

    /// Binds raw form data to this form.
    fn bind(&mut self, data: &QueryDict);

    /// Returns `true` if this form has been bound to data.
    fn is_bound(&self) -> bool;

    /// Validates the form asynchronously. Returns `true` if valid.
    ///
    /// This is async because validation may require database access for
    /// uniqueness checks, foreign key validation, etc. After calling this,
    /// `errors()` and `cleaned_data()` are populated.
    async fn is_valid(&mut self) -> bool;

    /// Returns per-field validation errors.
    ///
    /// Keys are field names, values are lists of error messages.
    fn errors(&self) -> &HashMap<String, Vec<String>>;

    /// Returns the cleaned (validated and coerced) data.
    ///
    /// Only populated after a successful call to `is_valid()`.
    fn cleaned_data(&self) -> &HashMap<String, Value>;

    /// Generates a template context dictionary for rendering.
    ///
    /// The returned map contains keys like "fields", "errors", "is_bound",
    /// etc. for use in template rendering.
    fn as_context(&self) -> HashMap<String, ContextValue>;

    /// Cross-field validation hook. Override to add form-level validation.
    ///
    /// This is async to support database lookups during validation.
    /// The default implementation does nothing.
    async fn clean(&self) -> Result<(), HashMap<String, Vec<String>>> {
        Ok(())
    }
}

/// A general-purpose form implementation.
///
/// `BaseForm` holds a list of field definitions and manages binding,
/// validation, and cleaned data. It is the most common way to create
/// forms without a model backing.
pub struct BaseForm {
    field_defs: Vec<FormFieldDef>,
    initial_data: HashMap<String, Value>,
    prefix: Option<String>,
    bound: bool,
    raw_data: HashMap<String, Option<String>>,
    errors: HashMap<String, Vec<String>>,
    cleaned_data: HashMap<String, Value>,
}

impl BaseForm {
    /// Creates a new `BaseForm` with the given field definitions.
    pub fn new(fields: Vec<FormFieldDef>) -> Self {
        Self {
            field_defs: fields,
            initial_data: HashMap::new(),
            prefix: None,
            bound: false,
            raw_data: HashMap::new(),
            errors: HashMap::new(),
            cleaned_data: HashMap::new(),
        }
    }

    /// Sets initial (default) values for fields.
    pub fn with_initial(mut self, initial: HashMap<String, Value>) -> Self {
        self.initial_data = initial;
        self
    }

    /// Sets the form prefix.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Returns bound fields for template iteration.
    pub fn bound_fields(&self) -> Vec<BoundField> {
        self.field_defs
            .iter()
            .map(|field| {
                let data = self.raw_data.get(&field.name).cloned().flatten();
                let errors = self.errors.get(&field.name).cloned().unwrap_or_default();
                BoundField::new(field, data, errors, self.prefix.as_deref())
            })
            .collect()
    }

    /// Returns the non-field (form-level) errors.
    pub fn non_field_errors(&self) -> &[String] {
        self.errors
            .get("__all__")
            .map_or(&[], Vec::as_slice)
    }
}

#[async_trait]
impl Form for BaseForm {
    fn fields(&self) -> &[FormFieldDef] {
        &self.field_defs
    }

    fn initial(&self) -> &HashMap<String, Value> {
        &self.initial_data
    }

    fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    fn bind(&mut self, data: &QueryDict) {
        self.bound = true;
        self.raw_data.clear();
        self.errors.clear();
        self.cleaned_data.clear();

        for field in &self.field_defs {
            let html_name = match &self.prefix {
                Some(p) => format!("{p}-{}", field.name),
                None => field.name.clone(),
            };
            let value = data.get(&html_name).map(String::from);
            self.raw_data.insert(field.name.clone(), value);
        }
    }

    fn is_bound(&self) -> bool {
        self.bound
    }

    async fn is_valid(&mut self) -> bool {
        if !self.bound {
            return false;
        }

        self.errors.clear();
        self.cleaned_data.clear();

        // Step 1: Field-level validation
        validation::clean_fields(
            &self.field_defs,
            &self.raw_data,
            &mut self.cleaned_data,
            &mut self.errors,
        );

        // Step 2: Form-level cross-field validation (async)
        if let Err(form_errors) = self.clean().await {
            for (key, msgs) in form_errors {
                self.errors
                    .entry(key)
                    .or_default()
                    .extend(msgs);
            }
        }

        self.errors.is_empty()
    }

    fn errors(&self) -> &HashMap<String, Vec<String>> {
        &self.errors
    }

    fn cleaned_data(&self) -> &HashMap<String, Value> {
        &self.cleaned_data
    }

    fn as_context(&self) -> HashMap<String, ContextValue> {
        let mut ctx = HashMap::new();

        // Fields as a list of dicts
        let fields: Vec<ContextValue> = self
            .bound_fields()
            .iter()
            .map(|bf| {
                let mut field_ctx = HashMap::new();
                field_ctx.insert("name".to_string(), ContextValue::String(bf.name.clone()));
                field_ctx.insert(
                    "label".to_string(),
                    ContextValue::String(bf.field.label.clone()),
                );
                field_ctx.insert(
                    "help_text".to_string(),
                    ContextValue::String(bf.field.help_text.clone()),
                );
                field_ctx.insert(
                    "html".to_string(),
                    ContextValue::SafeString(bf.render(&HashMap::new())),
                );
                field_ctx.insert(
                    "label_tag".to_string(),
                    ContextValue::SafeString(bf.label_tag()),
                );
                field_ctx.insert(
                    "errors".to_string(),
                    ContextValue::SafeString(bf.errors_as_ul()),
                );
                field_ctx.insert(
                    "required".to_string(),
                    ContextValue::Bool(bf.field.required),
                );
                ContextValue::Dict(field_ctx)
            })
            .collect();
        ctx.insert("fields".to_string(), ContextValue::List(fields));

        // Errors
        let error_ctx: HashMap<String, ContextValue> = self
            .errors
            .iter()
            .map(|(k, v)| {
                let error_list: Vec<ContextValue> =
                    v.iter().map(|e| ContextValue::String(e.clone())).collect();
                (k.clone(), ContextValue::List(error_list))
            })
            .collect();
        ctx.insert("errors".to_string(), ContextValue::Dict(error_ctx));

        // Non-field errors
        let non_field: Vec<ContextValue> = self
            .non_field_errors()
            .iter()
            .map(|e| ContextValue::String(e.clone()))
            .collect();
        ctx.insert(
            "non_field_errors".to_string(),
            ContextValue::List(non_field),
        );

        ctx.insert("is_bound".to_string(), ContextValue::Bool(self.bound));

        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::FormFieldType;

    fn make_test_form() -> BaseForm {
        BaseForm::new(vec![
            FormFieldDef::new(
                "username",
                FormFieldType::Char {
                    min_length: Some(3),
                    max_length: Some(20),
                    strip: true,
                },
            ),
            FormFieldDef::new(
                "email",
                FormFieldType::Email,
            ),
            FormFieldDef::new(
                "age",
                FormFieldType::Integer {
                    min_value: Some(0),
                    max_value: Some(150),
                },
            )
            .required(false),
        ])
    }

    #[tokio::test]
    async fn test_form_unbound() {
        let mut form = make_test_form();
        assert!(!form.is_bound());
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_form_bind_and_validate() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=alice&email=alice@example.com&age=30");
        form.bind(&qd);
        assert!(form.is_bound());
        assert!(form.is_valid().await);
        assert_eq!(
            form.cleaned_data().get("username"),
            Some(&Value::String("alice".to_string()))
        );
        assert_eq!(
            form.cleaned_data().get("email"),
            Some(&Value::String("alice@example.com".to_string()))
        );
        assert_eq!(form.cleaned_data().get("age"), Some(&Value::Int(30)));
    }

    #[tokio::test]
    async fn test_form_validation_errors() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=ab&email=not-email");
        form.bind(&qd);
        assert!(!form.is_valid().await);
        assert!(form.errors().contains_key("username"));
        assert!(form.errors().contains_key("email"));
    }

    #[tokio::test]
    async fn test_form_required_field_missing() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("age=25");
        form.bind(&qd);
        assert!(!form.is_valid().await);
        assert!(form.errors().contains_key("username"));
        assert!(form.errors().contains_key("email"));
    }

    #[tokio::test]
    async fn test_form_optional_field() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=alice&email=alice@example.com");
        form.bind(&qd);
        assert!(form.is_valid().await);
        // age is optional, should have a null/default value
        let age = form.cleaned_data().get("age");
        assert!(age.is_some());
    }

    #[tokio::test]
    async fn test_form_with_prefix() {
        let mut form = make_test_form().with_prefix("myform");
        assert_eq!(form.prefix(), Some("myform"));
        let qd = QueryDict::parse(
            "myform-username=alice&myform-email=alice@example.com&myform-age=25",
        );
        form.bind(&qd);
        assert!(form.is_valid().await);
    }

    #[tokio::test]
    async fn test_form_with_initial() {
        let mut initial = HashMap::new();
        initial.insert("username".to_string(), Value::String("default_user".into()));
        let form = make_test_form().with_initial(initial);
        assert_eq!(
            form.initial().get("username"),
            Some(&Value::String("default_user".into()))
        );
    }

    #[test]
    fn test_form_fields() {
        let form = make_test_form();
        assert_eq!(form.fields().len(), 3);
        assert_eq!(form.fields()[0].name, "username");
        assert_eq!(form.fields()[1].name, "email");
        assert_eq!(form.fields()[2].name, "age");
    }

    #[test]
    fn test_form_bound_fields() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=alice&email=test@test.com");
        form.bind(&qd);
        let bfs = form.bound_fields();
        assert_eq!(bfs.len(), 3);
        assert_eq!(bfs[0].name, "username");
        assert_eq!(bfs[0].data, Some("alice".to_string()));
    }

    #[tokio::test]
    async fn test_form_as_context() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=alice&email=alice@example.com");
        form.bind(&qd);
        form.is_valid().await;

        let ctx = form.as_context();
        assert!(ctx.contains_key("fields"));
        assert!(ctx.contains_key("errors"));
        assert!(ctx.contains_key("is_bound"));

        if let ContextValue::List(fields) = ctx.get("fields").unwrap() {
            assert_eq!(fields.len(), 3);
        } else {
            panic!("Expected fields to be a list");
        }
    }

    #[tokio::test]
    async fn test_form_non_field_errors() {
        let form = make_test_form();
        assert!(form.non_field_errors().is_empty());
    }

    #[tokio::test]
    async fn test_form_rebind_clears_state() {
        let mut form = make_test_form();
        // First bind with invalid data
        let qd1 = QueryDict::parse("username=ab");
        form.bind(&qd1);
        assert!(!form.is_valid().await);
        assert!(!form.errors().is_empty());

        // Rebind with valid data
        let qd2 = QueryDict::parse("username=alice&email=alice@example.com");
        form.bind(&qd2);
        assert!(form.is_valid().await);
        assert!(form.errors().is_empty());
    }
}
