//! Integration tests for `#[derive(Form)]`.
//!
//! These tests verify that the generated form field definitions and
//! `BaseForm` constructor work correctly.

use django_rs_forms::fields::FormFieldType;
use django_rs_forms::form::Form as FormTrait;
use django_rs_macros::Form;

// ── Basic form ──────────────────────────────────────────────────────────

#[derive(Form)]
#[form(action = "/submit/", method = "post")]
pub struct ContactForm {
    #[form_field(max_length = 100, label = "Your Name")]
    pub name: String,

    #[form_field(field_type = "email")]
    pub email: String,

    #[form_field(widget = "textarea")]
    pub message: String,
}

#[test]
fn test_contact_form_field_count() {
    let fields = ContactForm::form_fields();
    assert_eq!(fields.len(), 3);
}

#[test]
fn test_contact_form_field_names() {
    let fields = ContactForm::form_fields();
    assert_eq!(fields[0].name, "name");
    assert_eq!(fields[1].name, "email");
    assert_eq!(fields[2].name, "message");
}

#[test]
fn test_contact_form_name_field_label() {
    let fields = ContactForm::form_fields();
    assert_eq!(fields[0].label, "Your Name");
}

#[test]
fn test_contact_form_email_type() {
    let fields = ContactForm::form_fields();
    assert!(matches!(fields[1].field_type, FormFieldType::Email));
}

#[test]
fn test_contact_form_message_widget() {
    let fields = ContactForm::form_fields();
    assert_eq!(
        fields[2].widget,
        django_rs_forms::widgets::WidgetType::Textarea
    );
}

#[test]
fn test_contact_form_base_form() {
    let form = ContactForm::as_base_form();
    assert_eq!(FormTrait::fields(&form).len(), 3);
}

// ── Form with optional fields ───────────────────────────────────────────

#[derive(Form)]
#[form]
pub struct RegistrationForm {
    #[form_field(max_length = 150)]
    pub username: String,

    #[form_field(field_type = "email")]
    pub email: String,

    #[form_field(widget = "password")]
    pub password: String,

    #[form_field]
    pub bio: Option<String>,
}

#[test]
fn test_registration_form_optional_field() {
    let fields = RegistrationForm::form_fields();
    // Option<String> should default to required = false
    let bio = fields.iter().find(|f| f.name == "bio").unwrap();
    assert!(
        !bio.required,
        "Option<String> should default to not required"
    );
}

#[test]
fn test_registration_form_required_fields() {
    let fields = RegistrationForm::form_fields();
    let username = fields.iter().find(|f| f.name == "username").unwrap();
    assert!(username.required, "String should default to required");

    let email = fields.iter().find(|f| f.name == "email").unwrap();
    assert!(email.required);
}

#[test]
fn test_registration_form_password_widget() {
    let fields = RegistrationForm::form_fields();
    let pw = fields.iter().find(|f| f.name == "password").unwrap();
    assert_eq!(
        pw.widget,
        django_rs_forms::widgets::WidgetType::PasswordInput
    );
}

// ── Form with numeric fields ────────────────────────────────────────────

#[derive(Form)]
#[form]
pub struct FilterForm {
    #[form_field(min_value = 0, max_value = 1000)]
    pub min_price: i64,

    #[form_field(min_value = 0, max_value = 1000)]
    pub max_price: i64,
}

#[test]
fn test_filter_form_integer_type() {
    let fields = FilterForm::form_fields();
    if let FormFieldType::Integer {
        min_value,
        max_value,
    } = &fields[0].field_type
    {
        assert_eq!(*min_value, Some(0));
        assert_eq!(*max_value, Some(1000));
    } else {
        panic!("Expected Integer field type for min_price");
    }
}

// ── Form with boolean field ─────────────────────────────────────────────

#[derive(Form)]
#[form]
pub struct SettingsForm {
    #[form_field]
    pub notifications: bool,

    #[form_field]
    pub dark_mode: bool,
}

#[test]
fn test_settings_form_boolean_type() {
    let fields = SettingsForm::form_fields();
    assert!(matches!(fields[0].field_type, FormFieldType::Boolean));
    assert!(matches!(fields[1].field_type, FormFieldType::Boolean));
}
