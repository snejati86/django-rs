//! Formsets — collections of related forms on a single page.
//!
//! A [`FormSet`] manages multiple instances of the same form, handling the
//! management form data (TOTAL_FORMS, INITIAL_FORMS, etc.) and coordinating
//! validation across all forms.
//!
//! This mirrors Django's `django.forms.formsets.BaseFormSet`.

use std::collections::HashMap;

use django_rs_http::QueryDict;
use django_rs_template::context::ContextValue;

use crate::fields::{FormFieldDef, FormFieldType};
use crate::form::Form;
use crate::widgets::WidgetType;

/// The prefix used for management form fields.
const MANAGEMENT_FORM_PREFIX: &str = "form";

/// Management form field names.
const TOTAL_FORMS: &str = "TOTAL_FORMS";
const INITIAL_FORMS: &str = "INITIAL_FORMS";
const MIN_NUM_FORMS: &str = "MIN_NUM_FORMS";
const MAX_NUM_FORMS: &str = "MAX_NUM_FORMS";

/// A collection of related forms managed together.
///
/// Formsets handle:
/// - Rendering multiple copies of the same form
/// - Management form data (tracking how many forms exist)
/// - Coordinated validation across all forms
/// - Optional delete and ordering support
pub struct FormSet {
    /// The individual form instances.
    pub forms: Vec<Box<dyn Form>>,
    /// Number of extra (empty) forms to display.
    pub extra: usize,
    /// Minimum number of forms required.
    pub min_num: usize,
    /// Maximum number of forms allowed.
    pub max_num: usize,
    /// Whether forms can be marked for deletion.
    pub can_delete: bool,
    /// Whether forms can be reordered.
    pub can_order: bool,
    /// The formset prefix for HTML name attributes.
    prefix: String,
    /// Errors specific to the formset (not individual forms).
    non_form_errors: Vec<String>,
    /// Whether the formset has been bound to data.
    is_bound: bool,
}

impl FormSet {
    /// Creates a new `FormSet` with the given form instances.
    pub fn new(forms: Vec<Box<dyn Form>>) -> Self {
        Self {
            forms,
            extra: 1,
            min_num: 0,
            max_num: 1000,
            can_delete: false,
            can_order: false,
            prefix: MANAGEMENT_FORM_PREFIX.to_string(),
            non_form_errors: Vec::new(),
            is_bound: false,
        }
    }

    /// Sets the number of extra forms.
    pub fn with_extra(mut self, extra: usize) -> Self {
        self.extra = extra;
        self
    }

    /// Sets the minimum number of forms.
    pub fn with_min_num(mut self, min_num: usize) -> Self {
        self.min_num = min_num;
        self
    }

    /// Sets the maximum number of forms.
    pub fn with_max_num(mut self, max_num: usize) -> Self {
        self.max_num = max_num;
        self
    }

    /// Enables form deletion support.
    pub fn with_can_delete(mut self, can_delete: bool) -> Self {
        self.can_delete = can_delete;
        self
    }

    /// Enables form ordering support.
    pub fn with_can_order(mut self, can_order: bool) -> Self {
        self.can_order = can_order;
        self
    }

    /// Sets the formset prefix.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Returns the total number of forms (initial + extra).
    pub fn total_form_count(&self) -> usize {
        self.forms.len()
    }

    /// Returns the number of initial (pre-populated) forms.
    pub fn initial_form_count(&self) -> usize {
        self.forms
            .iter()
            .filter(|f| f.is_bound())
            .count()
    }

    /// Returns the management form data as a `HashMap`.
    ///
    /// This data is typically rendered as hidden inputs on the page.
    pub fn management_form_data(&self) -> HashMap<String, String> {
        let mut data = HashMap::new();
        let prefix = &self.prefix;
        data.insert(
            format!("{prefix}-{TOTAL_FORMS}"),
            self.total_form_count().to_string(),
        );
        data.insert(
            format!("{prefix}-{INITIAL_FORMS}"),
            self.initial_form_count().to_string(),
        );
        data.insert(
            format!("{prefix}-{MIN_NUM_FORMS}"),
            self.min_num.to_string(),
        );
        data.insert(
            format!("{prefix}-{MAX_NUM_FORMS}"),
            self.max_num.to_string(),
        );
        data
    }

    /// Renders the management form as hidden HTML inputs.
    pub fn management_form_html(&self) -> String {
        let data = self.management_form_data();
        let mut html = String::new();
        let mut keys: Vec<&String> = data.keys().collect();
        keys.sort(); // deterministic output
        for key in keys {
            let value = &data[key];
            html.push_str(&format!(
                r#"<input type="hidden" name="{key}" value="{value}" />"#
            ));
        }
        html
    }

    /// Binds form data to all forms in the formset.
    ///
    /// Parses the management form data to determine how many forms
    /// to process, then binds each form's data using the formset prefix
    /// and form index.
    pub fn bind(&mut self, data: &QueryDict) {
        self.is_bound = true;
        for (i, form) in self.forms.iter_mut().enumerate() {
            // Create a QueryDict with just this form's data
            let form_prefix = format!("{}-{i}", self.prefix);
            let mut form_data = QueryDict::new_mutable();
            for key in data.keys() {
                if key.starts_with(&form_prefix) {
                    if let Some(val) = data.get(key) {
                        let _ = form_data.set(key, val);
                    }
                }
            }
            form.bind(&form_data);
        }
    }

    /// Validates all forms in the formset asynchronously.
    ///
    /// Returns `true` if all forms are valid and the formset-level
    /// validation passes.
    pub async fn is_valid(&mut self) -> bool {
        if !self.is_bound {
            return false;
        }

        self.non_form_errors.clear();
        let mut all_valid = true;

        for form in &mut self.forms {
            if !form.is_valid().await {
                all_valid = false;
            }
        }

        // Formset-level validation
        if self.forms.len() < self.min_num {
            self.non_form_errors.push(format!(
                "Please submit at least {} forms.",
                self.min_num
            ));
            all_valid = false;
        }
        if self.forms.len() > self.max_num {
            self.non_form_errors.push(format!(
                "Please submit at most {} forms.",
                self.max_num
            ));
            all_valid = false;
        }

        all_valid
    }

    /// Returns formset-level (non-form) errors.
    pub fn non_form_errors(&self) -> &[String] {
        &self.non_form_errors
    }

    /// Returns `true` if the formset has been bound to data.
    pub fn is_bound(&self) -> bool {
        self.is_bound
    }

    /// Generates a template context for the formset.
    pub fn as_context(&self) -> HashMap<String, ContextValue> {
        let mut ctx = HashMap::new();

        let form_contexts: Vec<ContextValue> = self
            .forms
            .iter()
            .map(|f| ContextValue::Dict(
                f.as_context()
                    .into_iter()
                    .collect(),
            ))
            .collect();
        ctx.insert("forms".to_string(), ContextValue::List(form_contexts));

        ctx.insert(
            "management_form".to_string(),
            ContextValue::SafeString(self.management_form_html()),
        );

        let nfe: Vec<ContextValue> = self
            .non_form_errors
            .iter()
            .map(|e| ContextValue::String(e.clone()))
            .collect();
        ctx.insert("non_form_errors".to_string(), ContextValue::List(nfe));

        ctx.insert(
            "total_form_count".to_string(),
            ContextValue::Integer(self.total_form_count() as i64),
        );
        ctx.insert(
            "can_delete".to_string(),
            ContextValue::Bool(self.can_delete),
        );
        ctx.insert(
            "can_order".to_string(),
            ContextValue::Bool(self.can_order),
        );

        ctx
    }
}

/// Creates a formset from a factory function that produces form instances.
///
/// This is a convenience function for creating formsets with a given number
/// of forms, similar to Django's `formset_factory`.
pub fn create_formset<F>(
    form_factory: F,
    initial_count: usize,
    extra: usize,
) -> FormSet
where
    F: Fn(usize) -> Box<dyn Form>,
{
    let total = initial_count + extra;
    let forms: Vec<Box<dyn Form>> = (0..total).map(&form_factory).collect();
    FormSet::new(forms).with_extra(extra)
}

/// Creates a `BaseForm` suitable for use as a management form.
///
/// The management form contains hidden inputs tracking the total number
/// of forms, initial forms, and min/max constraints.
pub fn management_form_fields(prefix: &str) -> Vec<FormFieldDef> {
    let p = prefix.to_string();
    vec![
        FormFieldDef::new(
            format!("{p}-{TOTAL_FORMS}"),
            FormFieldType::Integer {
                min_value: Some(0),
                max_value: None,
            },
        )
        .widget(WidgetType::HiddenInput),
        FormFieldDef::new(
            format!("{p}-{INITIAL_FORMS}"),
            FormFieldType::Integer {
                min_value: Some(0),
                max_value: None,
            },
        )
        .widget(WidgetType::HiddenInput),
        FormFieldDef::new(
            format!("{p}-{MIN_NUM_FORMS}"),
            FormFieldType::Integer {
                min_value: Some(0),
                max_value: None,
            },
        )
        .widget(WidgetType::HiddenInput),
        FormFieldDef::new(
            format!("{p}-{MAX_NUM_FORMS}"),
            FormFieldType::Integer {
                min_value: Some(0),
                max_value: None,
            },
        )
        .widget(WidgetType::HiddenInput),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::FormFieldDef;
    use crate::form::BaseForm;

    fn make_simple_form() -> BaseForm {
        BaseForm::new(vec![
            FormFieldDef::new(
                "name",
                FormFieldType::Char {
                    min_length: None,
                    max_length: None,
                    strip: false,
                },
            ),
        ])
    }

    #[test]
    fn test_formset_new() {
        let forms: Vec<Box<dyn Form>> = vec![
            Box::new(make_simple_form()),
            Box::new(make_simple_form()),
        ];
        let fs = FormSet::new(forms);
        assert_eq!(fs.total_form_count(), 2);
        assert_eq!(fs.initial_form_count(), 0); // Not bound yet
    }

    #[test]
    fn test_formset_builder() {
        let fs = FormSet::new(vec![])
            .with_extra(3)
            .with_min_num(1)
            .with_max_num(10)
            .with_can_delete(true)
            .with_can_order(true)
            .with_prefix("items");
        assert_eq!(fs.extra, 3);
        assert_eq!(fs.min_num, 1);
        assert_eq!(fs.max_num, 10);
        assert!(fs.can_delete);
        assert!(fs.can_order);
        assert_eq!(fs.prefix, "items");
    }

    #[test]
    fn test_management_form_data() {
        let forms: Vec<Box<dyn Form>> = vec![
            Box::new(make_simple_form()),
            Box::new(make_simple_form()),
        ];
        let fs = FormSet::new(forms).with_min_num(1).with_max_num(5);
        let data = fs.management_form_data();
        assert_eq!(data.get("form-TOTAL_FORMS"), Some(&"2".to_string()));
        assert_eq!(data.get("form-INITIAL_FORMS"), Some(&"0".to_string()));
        assert_eq!(data.get("form-MIN_NUM_FORMS"), Some(&"1".to_string()));
        assert_eq!(data.get("form-MAX_NUM_FORMS"), Some(&"5".to_string()));
    }

    #[test]
    fn test_management_form_html() {
        let fs = FormSet::new(vec![Box::new(make_simple_form())]);
        let html = fs.management_form_html();
        assert!(html.contains("TOTAL_FORMS"));
        assert!(html.contains("INITIAL_FORMS"));
        assert!(html.contains(r#"type="hidden""#));
    }

    #[tokio::test]
    async fn test_formset_unbound_invalid() {
        let mut fs = FormSet::new(vec![Box::new(make_simple_form())]);
        assert!(!fs.is_bound());
        assert!(!fs.is_valid().await);
    }

    #[tokio::test]
    async fn test_formset_min_num_validation() {
        let mut fs = FormSet::new(vec![])
            .with_min_num(2);
        fs.is_bound = true; // simulate binding
        assert!(!fs.is_valid().await);
        assert!(fs.non_form_errors()[0].contains("at least 2"));
    }

    #[tokio::test]
    async fn test_formset_max_num_validation() {
        let forms: Vec<Box<dyn Form>> = (0..5).map(|_| {
            let f: Box<dyn Form> = Box::new(make_simple_form());
            f
        }).collect();
        let mut fs = FormSet::new(forms).with_max_num(3);
        fs.is_bound = true;
        assert!(!fs.is_valid().await);
        assert!(fs.non_form_errors()[0].contains("at most 3"));
    }

    #[test]
    fn test_create_formset() {
        let fs = create_formset(
            |_i| Box::new(make_simple_form()),
            2,  // initial
            1,  // extra
        );
        assert_eq!(fs.total_form_count(), 3);
        assert_eq!(fs.extra, 1);
    }

    #[test]
    fn test_management_form_fields() {
        let fields = management_form_fields("myform");
        assert_eq!(fields.len(), 4);
        assert!(fields.iter().all(|f| f.widget == WidgetType::HiddenInput));
    }

    #[test]
    fn test_formset_as_context() {
        let forms: Vec<Box<dyn Form>> = vec![
            Box::new(make_simple_form()),
        ];
        let fs = FormSet::new(forms).with_can_delete(true);
        let ctx = fs.as_context();
        assert!(ctx.contains_key("forms"));
        assert!(ctx.contains_key("management_form"));
        assert!(ctx.contains_key("total_form_count"));
        assert!(ctx.contains_key("can_delete"));

        if let ContextValue::List(form_list) = ctx.get("forms").unwrap() {
            assert_eq!(form_list.len(), 1);
        } else {
            panic!("Expected forms to be a list");
        }

        assert_eq!(
            ctx.get("can_delete"),
            Some(&ContextValue::Bool(true))
        );
    }

    #[test]
    fn test_formset_non_form_errors_initially_empty() {
        let fs = FormSet::new(vec![]);
        assert!(fs.non_form_errors().is_empty());
    }

    #[tokio::test]
    async fn test_formset_all_forms_valid() {
        let forms: Vec<Box<dyn Form>> = vec![
            Box::new(make_simple_form()),
        ];
        let mut fs = FormSet::new(forms);
        // Bind each form individually
        let _qd = QueryDict::parse("form-0-name=Alice");
        // Manually bind
        let form_data = QueryDict::parse("name=Alice");
        fs.forms[0].bind(&form_data);
        fs.is_bound = true;
        assert!(fs.is_valid().await);
    }
}
