//! Bound fields — form fields populated with data and errors.
//!
//! A [`BoundField`] represents the combination of a form field definition,
//! its current data value, any validation errors, and the widget used for
//! rendering. It is the primary type used when iterating over a form's
//! fields in a template.
//!
//! This mirrors Django's `django.forms.boundfield.BoundField`.

use std::collections::HashMap;

use crate::fields::FormFieldDef;
use crate::widgets::{self, Widget};

/// A form field bound to data and validation state.
///
/// `BoundField` is created during form rendering to pair a field definition
/// with its current value, errors, and widget. Templates use bound fields
/// to render individual form rows.
pub struct BoundField {
    /// The field's HTML name attribute.
    pub name: String,
    /// Reference-counted field definition (not a reference to avoid lifetime issues).
    pub field: BoundFieldDef,
    /// The raw data value submitted for this field.
    pub data: Option<String>,
    /// Validation error messages for this field.
    pub errors: Vec<String>,
    /// The widget instance used for rendering.
    pub widget: Box<dyn Widget>,
}

/// Minimal field definition snapshot stored in a `BoundField`.
///
/// This avoids lifetime issues by owning copies of the relevant metadata.
#[derive(Debug, Clone)]
pub struct BoundFieldDef {
    /// The field name.
    pub name: String,
    /// Human-readable label.
    pub label: String,
    /// Help text.
    pub help_text: String,
    /// Whether the field is required.
    pub required: bool,
    /// Whether the field is disabled.
    pub disabled: bool,
}

impl BoundField {
    /// Creates a new `BoundField` from a field definition and current state.
    pub fn new(
        field_def: &FormFieldDef,
        data: Option<String>,
        errors: Vec<String>,
        prefix: Option<&str>,
    ) -> Self {
        let html_name = match prefix {
            Some(p) => format!("{p}-{}", field_def.name),
            None => field_def.name.clone(),
        };

        let widget = widgets::create_widget(&field_def.widget);

        Self {
            name: html_name,
            field: BoundFieldDef {
                name: field_def.name.clone(),
                label: field_def.label.clone(),
                help_text: field_def.help_text.clone(),
                required: field_def.required,
                disabled: field_def.disabled,
            },
            data,
            errors,
            widget,
        }
    }

    /// Renders the widget HTML for this bound field.
    pub fn render(&self, extra_attrs: &HashMap<String, String>) -> String {
        let mut attrs = extra_attrs.clone();
        let id = self.auto_id();
        if !id.is_empty() {
            attrs.entry("id".to_string()).or_insert(id);
        }
        if self.field.disabled {
            attrs.insert("disabled".to_string(), "disabled".to_string());
        }
        self.widget.render(&self.name, &self.data, &attrs)
    }

    /// Renders a `<label>` element for this field.
    pub fn label_tag(&self) -> String {
        let id = self.auto_id();
        let label_id = self.widget.id_for_label(&id);
        if label_id.is_empty() {
            format!("<label>{}</label>", self.field.label)
        } else {
            format!(r#"<label for="{label_id}">{}</label>"#, self.field.label)
        }
    }

    /// Returns the auto-generated HTML `id` for this field.
    pub fn auto_id(&self) -> String {
        format!("id_{}", self.name)
    }

    /// Returns `true` if this field has any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Renders the error list as an HTML `<ul>` element.
    pub fn errors_as_ul(&self) -> String {
        if self.errors.is_empty() {
            return String::new();
        }
        let items: Vec<String> = self
            .errors
            .iter()
            .map(|e| format!("<li>{e}</li>"))
            .collect();
        format!(r#"<ul class="errorlist">{}</ul>"#, items.join(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::{FormFieldDef, FormFieldType};
    use crate::widgets::WidgetType;

    fn make_char_field(name: &str) -> FormFieldDef {
        FormFieldDef::new(
            name,
            FormFieldType::Char {
                min_length: None,
                max_length: None,
                strip: false,
            },
        )
    }

    #[test]
    fn test_bound_field_new() {
        let field_def = make_char_field("username");
        let bf = BoundField::new(&field_def, Some("alice".into()), vec![], None);
        assert_eq!(bf.name, "username");
        assert_eq!(bf.data, Some("alice".to_string()));
        assert!(bf.errors.is_empty());
    }

    #[test]
    fn test_bound_field_with_prefix() {
        let field_def = make_char_field("username");
        let bf = BoundField::new(&field_def, None, vec![], Some("form0"));
        assert_eq!(bf.name, "form0-username");
    }

    #[test]
    fn test_bound_field_render() {
        let field_def = make_char_field("username");
        let bf = BoundField::new(&field_def, Some("alice".into()), vec![], None);
        let html = bf.render(&HashMap::new());
        assert!(html.contains(r#"name="username""#));
        assert!(html.contains(r#"value="alice""#));
        assert!(html.contains(r#"id="id_username""#));
    }

    #[test]
    fn test_bound_field_label_tag() {
        let field_def = make_char_field("first_name").label("First Name");
        let bf = BoundField::new(&field_def, None, vec![], None);
        let label = bf.label_tag();
        assert!(label.contains(r#"for="id_first_name""#));
        assert!(label.contains("First Name"));
    }

    #[test]
    fn test_bound_field_auto_id() {
        let field_def = make_char_field("email");
        let bf = BoundField::new(&field_def, None, vec![], None);
        assert_eq!(bf.auto_id(), "id_email");
    }

    #[test]
    fn test_bound_field_has_errors() {
        let field_def = make_char_field("email");
        let bf_no_errors = BoundField::new(&field_def, None, vec![], None);
        assert!(!bf_no_errors.has_errors());

        let bf_errors = BoundField::new(
            &field_def,
            None,
            vec!["This field is required.".to_string()],
            None,
        );
        assert!(bf_errors.has_errors());
    }

    #[test]
    fn test_bound_field_errors_as_ul() {
        let field_def = make_char_field("email");
        let bf = BoundField::new(
            &field_def,
            None,
            vec![
                "This field is required.".to_string(),
                "Enter a valid email.".to_string(),
            ],
            None,
        );
        let html = bf.errors_as_ul();
        assert!(html.contains(r#"class="errorlist""#));
        assert!(html.contains("<li>This field is required.</li>"));
        assert!(html.contains("<li>Enter a valid email.</li>"));
    }

    #[test]
    fn test_bound_field_errors_as_ul_empty() {
        let field_def = make_char_field("email");
        let bf = BoundField::new(&field_def, None, vec![], None);
        assert_eq!(bf.errors_as_ul(), "");
    }

    #[test]
    fn test_bound_field_disabled() {
        let field_def = make_char_field("locked").disabled(true);
        let bf = BoundField::new(&field_def, Some("value".into()), vec![], None);
        let html = bf.render(&HashMap::new());
        assert!(html.contains(r#"disabled="disabled""#));
    }

    #[test]
    fn test_bound_field_custom_widget() {
        let field_def = make_char_field("bio").widget(WidgetType::Textarea);
        let bf = BoundField::new(&field_def, Some("Hello".into()), vec![], None);
        let html = bf.render(&HashMap::new());
        assert!(html.contains("<textarea"));
        assert!(html.contains("Hello"));
    }
}
