//! Widget system for rendering HTML form elements.
//!
//! Widgets are the bridge between form fields and their HTML representation.
//! Each widget knows how to render itself as HTML, extract a value from
//! submitted form data, and generate an appropriate `id` attribute for
//! its `<label>` element.
//!
//! This mirrors Django's `django.forms.widgets` module.

use std::collections::HashMap;
use std::fmt;

use django_rs_http::QueryDict;

/// Enumerates all built-in widget types.
///
/// Each variant corresponds to a distinct HTML form element or input type.
/// Widgets are matched by this enum for default widget selection and
/// serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetType {
    /// `<input type="text">`.
    TextInput,
    /// `<input type="number">`.
    NumberInput,
    /// `<input type="email">`.
    EmailInput,
    /// `<input type="url">`.
    UrlInput,
    /// `<input type="password">`.
    PasswordInput,
    /// `<input type="hidden">`.
    HiddenInput,
    /// `<textarea>`.
    Textarea,
    /// `<input type="checkbox">`.
    CheckboxInput,
    /// `<select>`.
    Select,
    /// `<select multiple>`.
    SelectMultiple,
    /// A set of `<input type="radio">` elements.
    RadioSelect,
    /// A set of `<input type="checkbox">` elements.
    CheckboxSelectMultiple,
    /// `<input type="date">`.
    DateInput,
    /// `<input type="datetime-local">`.
    DateTimeInput,
    /// `<input type="time">`.
    TimeInput,
    /// `<input type="file">`.
    FileInput,
    /// `<input type="file">` with a clear checkbox.
    ClearableFileInput,
}

impl fmt::Display for WidgetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::TextInput => "TextInput",
            Self::NumberInput => "NumberInput",
            Self::EmailInput => "EmailInput",
            Self::UrlInput => "UrlInput",
            Self::PasswordInput => "PasswordInput",
            Self::HiddenInput => "HiddenInput",
            Self::Textarea => "Textarea",
            Self::CheckboxInput => "CheckboxInput",
            Self::Select => "Select",
            Self::SelectMultiple => "SelectMultiple",
            Self::RadioSelect => "RadioSelect",
            Self::CheckboxSelectMultiple => "CheckboxSelectMultiple",
            Self::DateInput => "DateInput",
            Self::DateTimeInput => "DateTimeInput",
            Self::TimeInput => "TimeInput",
            Self::FileInput => "FileInput",
            Self::ClearableFileInput => "ClearableFileInput",
        };
        write!(f, "{name}")
    }
}

/// A trait for HTML form widgets.
///
/// Widgets are responsible for:
/// - Rendering an HTML element for a given field name and value
/// - Extracting the raw value from submitted `QueryDict` data
/// - Generating the `id` attribute for an associated `<label>` element
///
/// All widgets must be `Send + Sync` to support async form processing
/// across threads.
pub trait Widget: Send + Sync + fmt::Debug {
    /// Returns the widget type enum variant.
    fn widget_type(&self) -> WidgetType;

    /// Renders the widget as an HTML string.
    ///
    /// # Arguments
    /// - `name` - The HTML `name` attribute
    /// - `value` - The current value to display (if any)
    /// - `attrs` - Additional HTML attributes
    fn render(&self, name: &str, value: &Option<String>, attrs: &HashMap<String, String>)
        -> String;

    /// Extracts a raw string value from the submitted form data.
    ///
    /// Returns `None` if no value was submitted for this field name.
    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String>;

    /// Returns the HTML `id` attribute value for a label targeting this widget.
    fn id_for_label(&self, id: &str) -> String;
}

/// Formats an HTML attributes map into a string like ` key="value" key2="value2"`.
fn render_attrs(attrs: &HashMap<String, String>) -> String {
    if attrs.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = attrs
        .iter()
        .map(|(k, v)| format!(r#" {k}="{v}""#))
        .collect();
    parts.sort(); // deterministic output for testing
    parts.join("")
}

// ---------------------------------------------------------------------------
// Built-in widgets
// ---------------------------------------------------------------------------

/// A basic `<input type="text">` widget.
#[derive(Debug, Clone)]
pub struct TextInput;

impl Widget for TextInput {
    fn widget_type(&self) -> WidgetType {
        WidgetType::TextInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let val = value.as_deref().unwrap_or("");
        format!(
            r#"<input type="text" name="{name}" value="{val}"{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<input type="number">` widget.
#[derive(Debug, Clone)]
pub struct NumberInput;

impl Widget for NumberInput {
    fn widget_type(&self) -> WidgetType {
        WidgetType::NumberInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let val = value.as_deref().unwrap_or("");
        format!(
            r#"<input type="number" name="{name}" value="{val}"{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<input type="email">` widget.
#[derive(Debug, Clone)]
pub struct EmailInputWidget;

impl Widget for EmailInputWidget {
    fn widget_type(&self) -> WidgetType {
        WidgetType::EmailInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let val = value.as_deref().unwrap_or("");
        format!(
            r#"<input type="email" name="{name}" value="{val}"{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<input type="url">` widget.
#[derive(Debug, Clone)]
pub struct UrlInputWidget;

impl Widget for UrlInputWidget {
    fn widget_type(&self) -> WidgetType {
        WidgetType::UrlInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let val = value.as_deref().unwrap_or("");
        format!(
            r#"<input type="url" name="{name}" value="{val}"{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<input type="password">` widget.
///
/// By default, does not render the current value (for security).
#[derive(Debug, Clone)]
pub struct PasswordInput {
    /// Whether to render the value attribute. Defaults to `false`.
    pub render_value: bool,
}

impl Default for PasswordInput {
    fn default() -> Self {
        Self {
            render_value: false,
        }
    }
}

impl Widget for PasswordInput {
    fn widget_type(&self) -> WidgetType {
        WidgetType::PasswordInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let val = if self.render_value {
            value.as_deref().unwrap_or("")
        } else {
            ""
        };
        format!(
            r#"<input type="password" name="{name}" value="{val}"{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<input type="hidden">` widget.
#[derive(Debug, Clone)]
pub struct HiddenInput;

impl Widget for HiddenInput {
    fn widget_type(&self) -> WidgetType {
        WidgetType::HiddenInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let val = value.as_deref().unwrap_or("");
        format!(
            r#"<input type="hidden" name="{name}" value="{val}"{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<textarea>` widget.
#[derive(Debug, Clone)]
pub struct Textarea;

impl Widget for Textarea {
    fn widget_type(&self) -> WidgetType {
        WidgetType::Textarea
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let val = value.as_deref().unwrap_or("");
        format!(
            r#"<textarea name="{name}"{}>{val}</textarea>"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<input type="checkbox">` widget.
///
/// For a boolean field. The value in form data is typically "on" or absent.
#[derive(Debug, Clone)]
pub struct CheckboxInput;

impl Widget for CheckboxInput {
    fn widget_type(&self) -> WidgetType {
        WidgetType::CheckboxInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let checked = value
            .as_deref()
            .map_or(false, |v| v == "true" || v == "on" || v == "1");
        let checked_attr = if checked { " checked" } else { "" };
        format!(
            r#"<input type="checkbox" name="{name}"{checked_attr}{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        // Checkbox: presence means "on", absence means not checked
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<select>` widget.
#[derive(Debug, Clone)]
pub struct Select {
    /// The available choices as `(value, display_label)` pairs.
    pub choices: Vec<(String, String)>,
}

impl Select {
    /// Creates a new `Select` widget with the given choices.
    pub fn new(choices: Vec<(String, String)>) -> Self {
        Self { choices }
    }
}

impl Widget for Select {
    fn widget_type(&self) -> WidgetType {
        WidgetType::Select
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let current = value.as_deref().unwrap_or("");
        let mut options = String::new();
        for (val, label) in &self.choices {
            let selected = if val == current { " selected" } else { "" };
            options.push_str(&format!(
                r#"<option value="{val}"{selected}>{label}</option>"#
            ));
        }
        format!(
            r#"<select name="{name}"{}>{options}</select>"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<select multiple>` widget.
#[derive(Debug, Clone)]
pub struct SelectMultiple {
    /// The available choices as `(value, display_label)` pairs.
    pub choices: Vec<(String, String)>,
}

impl SelectMultiple {
    /// Creates a new `SelectMultiple` widget with the given choices.
    pub fn new(choices: Vec<(String, String)>) -> Self {
        Self { choices }
    }
}

impl Widget for SelectMultiple {
    fn widget_type(&self) -> WidgetType {
        WidgetType::SelectMultiple
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        // value is comma-separated list of selected values
        let selected_values: Vec<&str> = value
            .as_deref()
            .map_or_else(Vec::new, |v| v.split(',').collect());
        let mut options = String::new();
        for (val, label) in &self.choices {
            let selected = if selected_values.contains(&val.as_str()) {
                " selected"
            } else {
                ""
            };
            options.push_str(&format!(
                r#"<option value="{val}"{selected}>{label}</option>"#
            ));
        }
        format!(
            r#"<select name="{name}" multiple{}>{options}</select>"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get_list(name).map(|vals| vals.join(","))
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A set of `<input type="radio">` elements.
#[derive(Debug, Clone)]
pub struct RadioSelect {
    /// The available choices as `(value, display_label)` pairs.
    pub choices: Vec<(String, String)>,
}

impl RadioSelect {
    /// Creates a new `RadioSelect` widget with the given choices.
    pub fn new(choices: Vec<(String, String)>) -> Self {
        Self { choices }
    }
}

impl Widget for RadioSelect {
    fn widget_type(&self) -> WidgetType {
        WidgetType::RadioSelect
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let current = value.as_deref().unwrap_or("");
        let id_base = attrs.get("id").map_or(name, String::as_str);
        let mut html = String::from("<div>");
        for (i, (val, label)) in self.choices.iter().enumerate() {
            let checked = if val == current { " checked" } else { "" };
            let option_id = format!("{id_base}_{i}");
            html.push_str(&format!(
                r#"<div><input type="radio" name="{name}" value="{val}" id="{option_id}"{checked} />"#
            ));
            html.push_str(&format!(
                r#" <label for="{option_id}">{label}</label></div>"#
            ));
        }
        html.push_str("</div>");
        html
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        format!("{id}_0")
    }
}

/// A set of `<input type="checkbox">` elements for multiple selection.
#[derive(Debug, Clone)]
pub struct CheckboxSelectMultiple {
    /// The available choices as `(value, display_label)` pairs.
    pub choices: Vec<(String, String)>,
}

impl CheckboxSelectMultiple {
    /// Creates a new `CheckboxSelectMultiple` widget with the given choices.
    pub fn new(choices: Vec<(String, String)>) -> Self {
        Self { choices }
    }
}

impl Widget for CheckboxSelectMultiple {
    fn widget_type(&self) -> WidgetType {
        WidgetType::CheckboxSelectMultiple
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let selected_values: Vec<&str> = value
            .as_deref()
            .map_or_else(Vec::new, |v| v.split(',').collect());
        let id_base = attrs.get("id").map_or(name, String::as_str);
        let mut html = String::from("<div>");
        for (i, (val, label)) in self.choices.iter().enumerate() {
            let checked = if selected_values.contains(&val.as_str()) {
                " checked"
            } else {
                ""
            };
            let option_id = format!("{id_base}_{i}");
            html.push_str(&format!(
                r#"<div><input type="checkbox" name="{name}" value="{val}" id="{option_id}"{checked} />"#
            ));
            html.push_str(&format!(
                r#" <label for="{option_id}">{label}</label></div>"#
            ));
        }
        html.push_str("</div>");
        html
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get_list(name).map(|vals| vals.join(","))
    }

    fn id_for_label(&self, id: &str) -> String {
        format!("{id}_0")
    }
}

/// A `<input type="date">` widget.
#[derive(Debug, Clone)]
pub struct DateInput;

impl Widget for DateInput {
    fn widget_type(&self) -> WidgetType {
        WidgetType::DateInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let val = value.as_deref().unwrap_or("");
        format!(
            r#"<input type="date" name="{name}" value="{val}"{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<input type="datetime-local">` widget.
#[derive(Debug, Clone)]
pub struct DateTimeInputWidget;

impl Widget for DateTimeInputWidget {
    fn widget_type(&self) -> WidgetType {
        WidgetType::DateTimeInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let val = value.as_deref().unwrap_or("");
        format!(
            r#"<input type="datetime-local" name="{name}" value="{val}"{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<input type="time">` widget.
#[derive(Debug, Clone)]
pub struct TimeInputWidget;

impl Widget for TimeInputWidget {
    fn widget_type(&self) -> WidgetType {
        WidgetType::TimeInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let val = value.as_deref().unwrap_or("");
        format!(
            r#"<input type="time" name="{name}" value="{val}"{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A `<input type="file">` widget.
#[derive(Debug, Clone)]
pub struct FileInput;

impl Widget for FileInput {
    fn widget_type(&self) -> WidgetType {
        WidgetType::FileInput
    }

    fn render(
        &self,
        name: &str,
        _value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        // File inputs never render a value for security reasons
        format!(
            r#"<input type="file" name="{name}"{} />"#,
            render_attrs(attrs)
        )
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        // In a real implementation, file data comes from multipart form data.
        // For now, fall back to query dict.
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// A file input with a "clear" checkbox for optional file fields.
#[derive(Debug, Clone)]
pub struct ClearableFileInput;

impl Widget for ClearableFileInput {
    fn widget_type(&self) -> WidgetType {
        WidgetType::ClearableFileInput
    }

    fn render(
        &self,
        name: &str,
        value: &Option<String>,
        attrs: &HashMap<String, String>,
    ) -> String {
        let mut html = String::new();
        if let Some(val) = value.as_deref() {
            if !val.is_empty() {
                html.push_str(&format!(
                    r#"<span>Currently: {val}</span> <input type="checkbox" name="{name}-clear" /> Clear<br />"#
                ));
            }
        }
        html.push_str(&format!(
            r#"<input type="file" name="{name}"{} />"#,
            render_attrs(attrs)
        ));
        html
    }

    fn value_from_data(&self, data: &QueryDict, name: &str) -> Option<String> {
        // Check if the clear checkbox was ticked
        let clear_name = format!("{name}-clear");
        if data.get(&clear_name).is_some() {
            return Some(String::new()); // Signal to clear the file
        }
        data.get(name).map(String::from)
    }

    fn id_for_label(&self, id: &str) -> String {
        id.to_string()
    }
}

/// Creates a boxed widget from a `WidgetType` enum.
///
/// This is used to create default widgets for form field types and to
/// instantiate widgets from configuration.
///
/// For choice-based widgets (`Select`, `SelectMultiple`, `RadioSelect`,
/// `CheckboxSelectMultiple`), empty choices are used; callers should
/// update the choices after creation.
pub fn create_widget(widget_type: &WidgetType) -> Box<dyn Widget> {
    match widget_type {
        WidgetType::TextInput => Box::new(TextInput),
        WidgetType::NumberInput => Box::new(NumberInput),
        WidgetType::EmailInput => Box::new(EmailInputWidget),
        WidgetType::UrlInput => Box::new(UrlInputWidget),
        WidgetType::PasswordInput => Box::new(PasswordInput::default()),
        WidgetType::HiddenInput => Box::new(HiddenInput),
        WidgetType::Textarea => Box::new(Textarea),
        WidgetType::CheckboxInput => Box::new(CheckboxInput),
        WidgetType::Select => Box::new(Select::new(vec![])),
        WidgetType::SelectMultiple => Box::new(SelectMultiple::new(vec![])),
        WidgetType::RadioSelect => Box::new(RadioSelect::new(vec![])),
        WidgetType::CheckboxSelectMultiple => Box::new(CheckboxSelectMultiple::new(vec![])),
        WidgetType::DateInput => Box::new(DateInput),
        WidgetType::DateTimeInput => Box::new(DateTimeInputWidget),
        WidgetType::TimeInput => Box::new(TimeInputWidget),
        WidgetType::FileInput => Box::new(FileInput),
        WidgetType::ClearableFileInput => Box::new(ClearableFileInput),
    }
}

/// Creates a boxed widget from a `WidgetType`, populating choices if applicable.
pub fn create_widget_with_choices(
    widget_type: &WidgetType,
    choices: &[(String, String)],
) -> Box<dyn Widget> {
    match widget_type {
        WidgetType::Select => Box::new(Select::new(choices.to_vec())),
        WidgetType::SelectMultiple => Box::new(SelectMultiple::new(choices.to_vec())),
        WidgetType::RadioSelect => Box::new(RadioSelect::new(choices.to_vec())),
        WidgetType::CheckboxSelectMultiple => {
            Box::new(CheckboxSelectMultiple::new(choices.to_vec()))
        }
        other => create_widget(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_attrs() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn test_text_input_render() {
        let w = TextInput;
        let html = w.render("name", &Some("Alice".into()), &empty_attrs());
        assert!(html.contains(r#"type="text""#));
        assert!(html.contains(r#"name="name""#));
        assert!(html.contains(r#"value="Alice""#));
    }

    #[test]
    fn test_text_input_render_empty() {
        let w = TextInput;
        let html = w.render("name", &None, &empty_attrs());
        assert!(html.contains(r#"value="""#));
    }

    #[test]
    fn test_text_input_with_attrs() {
        let w = TextInput;
        let mut attrs = HashMap::new();
        attrs.insert("class".to_string(), "form-control".to_string());
        let html = w.render("name", &None, &attrs);
        assert!(html.contains(r#"class="form-control""#));
    }

    #[test]
    fn test_number_input_render() {
        let w = NumberInput;
        let html = w.render("age", &Some("25".into()), &empty_attrs());
        assert!(html.contains(r#"type="number""#));
        assert!(html.contains(r#"value="25""#));
    }

    #[test]
    fn test_email_input_render() {
        let w = EmailInputWidget;
        let html = w.render("email", &Some("test@example.com".into()), &empty_attrs());
        assert!(html.contains(r#"type="email""#));
    }

    #[test]
    fn test_url_input_render() {
        let w = UrlInputWidget;
        let html = w.render(
            "website",
            &Some("https://example.com".into()),
            &empty_attrs(),
        );
        assert!(html.contains(r#"type="url""#));
    }

    #[test]
    fn test_password_input_no_render_value() {
        let w = PasswordInput::default();
        let html = w.render("pass", &Some("secret".into()), &empty_attrs());
        assert!(html.contains(r#"type="password""#));
        assert!(html.contains(r#"value="""#)); // should NOT render the value
    }

    #[test]
    fn test_password_input_render_value() {
        let w = PasswordInput { render_value: true };
        let html = w.render("pass", &Some("secret".into()), &empty_attrs());
        assert!(html.contains(r#"value="secret""#));
    }

    #[test]
    fn test_hidden_input_render() {
        let w = HiddenInput;
        let html = w.render("csrf", &Some("token123".into()), &empty_attrs());
        assert!(html.contains(r#"type="hidden""#));
        assert!(html.contains(r#"value="token123""#));
    }

    #[test]
    fn test_textarea_render() {
        let w = Textarea;
        let html = w.render("bio", &Some("Hello world".into()), &empty_attrs());
        assert!(html.contains("<textarea"));
        assert!(html.contains("Hello world"));
        assert!(html.contains("</textarea>"));
    }

    #[test]
    fn test_checkbox_checked() {
        let w = CheckboxInput;
        let html = w.render("agree", &Some("true".into()), &empty_attrs());
        assert!(html.contains("checked"));
    }

    #[test]
    fn test_checkbox_unchecked() {
        let w = CheckboxInput;
        let html = w.render("agree", &Some("false".into()), &empty_attrs());
        assert!(!html.contains("checked"));
    }

    #[test]
    fn test_checkbox_on() {
        let w = CheckboxInput;
        let html = w.render("agree", &Some("on".into()), &empty_attrs());
        assert!(html.contains("checked"));
    }

    #[test]
    fn test_select_render() {
        let w = Select::new(vec![
            ("m".into(), "Male".into()),
            ("f".into(), "Female".into()),
        ]);
        let html = w.render("gender", &Some("f".into()), &empty_attrs());
        assert!(html.contains("<select"));
        assert!(html.contains(r#"<option value="m">Male</option>"#));
        assert!(html.contains(r#"<option value="f" selected>Female</option>"#));
    }

    #[test]
    fn test_select_multiple_render() {
        let w = SelectMultiple::new(vec![
            ("r".into(), "Red".into()),
            ("g".into(), "Green".into()),
            ("b".into(), "Blue".into()),
        ]);
        let html = w.render("colors", &Some("r,b".into()), &empty_attrs());
        assert!(html.contains("multiple"));
        assert!(html.contains(r#"<option value="r" selected>Red</option>"#));
        assert!(html.contains(r#"<option value="g">Green</option>"#));
        assert!(html.contains(r#"<option value="b" selected>Blue</option>"#));
    }

    #[test]
    fn test_radio_select_render() {
        let w = RadioSelect::new(vec![("1".into(), "One".into()), ("2".into(), "Two".into())]);
        let html = w.render("choice", &Some("1".into()), &empty_attrs());
        assert!(html.contains(r#"type="radio""#));
        assert!(html.contains("checked"));
    }

    #[test]
    fn test_radio_select_id_for_label() {
        let w = RadioSelect::new(vec![]);
        assert_eq!(w.id_for_label("id_choice"), "id_choice_0");
    }

    #[test]
    fn test_checkbox_select_multiple_render() {
        let w = CheckboxSelectMultiple::new(vec![
            ("a".into(), "Apple".into()),
            ("b".into(), "Banana".into()),
        ]);
        let html = w.render("fruits", &Some("a".into()), &empty_attrs());
        assert!(html.contains(r#"type="checkbox""#));
        assert!(html.contains("checked"));
    }

    #[test]
    fn test_date_input_render() {
        let w = DateInput;
        let html = w.render("birthday", &Some("2024-01-15".into()), &empty_attrs());
        assert!(html.contains(r#"type="date""#));
    }

    #[test]
    fn test_datetime_input_render() {
        let w = DateTimeInputWidget;
        let html = w.render("event", &Some("2024-01-15T10:30".into()), &empty_attrs());
        assert!(html.contains(r#"type="datetime-local""#));
    }

    #[test]
    fn test_time_input_render() {
        let w = TimeInputWidget;
        let html = w.render("start", &Some("14:30".into()), &empty_attrs());
        assert!(html.contains(r#"type="time""#));
    }

    #[test]
    fn test_file_input_render_no_value() {
        let w = FileInput;
        let html = w.render("photo", &Some("old_photo.jpg".into()), &empty_attrs());
        assert!(html.contains(r#"type="file""#));
        assert!(!html.contains("old_photo.jpg")); // should not render value
    }

    #[test]
    fn test_clearable_file_input_with_value() {
        let w = ClearableFileInput;
        let html = w.render("photo", &Some("photo.jpg".into()), &empty_attrs());
        assert!(html.contains("Currently: photo.jpg"));
        assert!(html.contains(r#"name="photo-clear""#));
        assert!(html.contains(r#"type="file""#));
    }

    #[test]
    fn test_clearable_file_input_no_value() {
        let w = ClearableFileInput;
        let html = w.render("photo", &None, &empty_attrs());
        assert!(!html.contains("Currently:"));
        assert!(html.contains(r#"type="file""#));
    }

    #[test]
    fn test_value_from_data_text() {
        let w = TextInput;
        let qd = QueryDict::parse("name=Alice");
        assert_eq!(w.value_from_data(&qd, "name"), Some("Alice".into()));
        assert_eq!(w.value_from_data(&qd, "missing"), None);
    }

    #[test]
    fn test_value_from_data_select_multiple() {
        let w = SelectMultiple::new(vec![]);
        let qd = QueryDict::parse("colors=red&colors=blue");
        let val = w.value_from_data(&qd, "colors");
        assert_eq!(val, Some("red,blue".to_string()));
    }

    #[test]
    fn test_value_from_data_clearable_file_clear() {
        let w = ClearableFileInput;
        let qd = QueryDict::parse("photo-clear=on");
        let val = w.value_from_data(&qd, "photo");
        assert_eq!(val, Some(String::new()));
    }

    #[test]
    fn test_create_widget() {
        let w = create_widget(&WidgetType::TextInput);
        assert_eq!(w.widget_type(), WidgetType::TextInput);

        let w = create_widget(&WidgetType::NumberInput);
        assert_eq!(w.widget_type(), WidgetType::NumberInput);

        let w = create_widget(&WidgetType::EmailInput);
        assert_eq!(w.widget_type(), WidgetType::EmailInput);

        let w = create_widget(&WidgetType::Textarea);
        assert_eq!(w.widget_type(), WidgetType::Textarea);

        let w = create_widget(&WidgetType::CheckboxInput);
        assert_eq!(w.widget_type(), WidgetType::CheckboxInput);

        let w = create_widget(&WidgetType::Select);
        assert_eq!(w.widget_type(), WidgetType::Select);

        let w = create_widget(&WidgetType::HiddenInput);
        assert_eq!(w.widget_type(), WidgetType::HiddenInput);

        let w = create_widget(&WidgetType::FileInput);
        assert_eq!(w.widget_type(), WidgetType::FileInput);
    }

    #[test]
    fn test_create_widget_with_choices() {
        let choices = vec![("a".into(), "Alpha".into()), ("b".into(), "Beta".into())];
        let w = create_widget_with_choices(&WidgetType::Select, &choices);
        let html = w.render("test", &None, &empty_attrs());
        assert!(html.contains("Alpha"));
        assert!(html.contains("Beta"));
    }

    #[test]
    fn test_widget_type_display() {
        assert_eq!(WidgetType::TextInput.to_string(), "TextInput");
        assert_eq!(WidgetType::Textarea.to_string(), "Textarea");
        assert_eq!(WidgetType::Select.to_string(), "Select");
    }

    #[test]
    fn test_id_for_label() {
        let w = TextInput;
        assert_eq!(w.id_for_label("id_name"), "id_name");

        let w = RadioSelect::new(vec![]);
        assert_eq!(w.id_for_label("id_choice"), "id_choice_0");

        let w = CheckboxSelectMultiple::new(vec![]);
        assert_eq!(w.id_for_label("id_items"), "id_items_0");
    }
}
