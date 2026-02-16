//! Authentication forms for django-rs.
//!
//! This module provides form types for common authentication workflows:
//!
//! - [`AuthenticationForm`] - Login form with username and password
//! - [`UserCreationForm`] - Registration form with password confirmation
//! - [`PasswordChangeForm`] - Change password (requires old password)
//! - [`SetPasswordForm`] - Set password (no old password required)
//! - [`PasswordResetForm`] - Request password reset via email
//!
//! Each form defines its fields using [`FormFieldDef`] from the forms framework
//! and provides custom validation methods. Forms are designed to integrate with
//! the form-view pipeline established in Wave 7.

use std::collections::HashMap;

use django_rs_db::value::Value;
use django_rs_forms::fields::{FormFieldDef, FormFieldType};
use django_rs_forms::form::{BaseForm, Form};
use django_rs_http::QueryDict;

// ── AuthenticationForm ──────────────────────────────────────────────

/// Login form with username and password fields.
///
/// Mirrors Django's `django.contrib.auth.forms.AuthenticationForm`.
/// Validates that both fields are present and non-empty.
pub struct AuthenticationForm {
    inner: BaseForm,
    /// Errors from credential verification (added by the view layer).
    custom_errors: HashMap<String, Vec<String>>,
}

impl AuthenticationForm {
    /// Creates a new authentication form with unbound state.
    pub fn new() -> Self {
        Self {
            inner: BaseForm::new(vec![
                FormFieldDef::new(
                    "username",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(150),
                        strip: true,
                    },
                ),
                FormFieldDef::new(
                    "password",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(128),
                        strip: false,
                    },
                ),
            ]),
            custom_errors: HashMap::new(),
        }
    }

    /// Binds form data to this form.
    pub fn bind(&mut self, data: &QueryDict) {
        self.inner.bind(data);
        self.custom_errors.clear();
    }

    /// Returns whether this form is bound.
    pub fn is_bound(&self) -> bool {
        self.inner.is_bound()
    }

    /// Validates the form fields.
    ///
    /// Note: This only validates field presence/format. Actual credential
    /// verification (password checking) is done in the view layer.
    pub async fn is_valid(&mut self) -> bool {
        self.custom_errors.clear();
        self.inner.is_valid().await
    }

    /// Returns all form errors (base form + custom errors).
    pub fn errors(&self) -> HashMap<String, Vec<String>> {
        let mut all_errors = self.inner.errors().clone();
        for (field, errors) in &self.custom_errors {
            all_errors
                .entry(field.clone())
                .or_default()
                .extend(errors.clone());
        }
        all_errors
    }

    /// Adds a non-field error (e.g., "Invalid username or password").
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.custom_errors
            .entry("__all__".to_string())
            .or_default()
            .push(error.into());
    }

    /// Returns cleaned data if the form is valid.
    pub fn cleaned_data(&self) -> &HashMap<String, Value> {
        self.inner.cleaned_data()
    }

    /// Returns the username from cleaned data.
    pub fn get_username(&self) -> Option<String> {
        self.inner
            .cleaned_data()
            .get("username")
            .map(|v| format!("{v}"))
    }

    /// Returns the password from cleaned data.
    pub fn get_password(&self) -> Option<String> {
        self.inner
            .cleaned_data()
            .get("password")
            .map(|v| format!("{v}"))
    }

    /// Returns the field definitions.
    pub fn field_defs(&self) -> &[FormFieldDef] {
        self.inner.fields()
    }
}

impl Default for AuthenticationForm {
    fn default() -> Self {
        Self::new()
    }
}

// ── UserCreationForm ────────────────────────────────────────────────

/// User registration form with password confirmation.
///
/// Mirrors Django's `django.contrib.auth.forms.UserCreationForm`.
/// Includes username, password1, and password2 (confirmation) fields.
pub struct UserCreationForm {
    inner: BaseForm,
    /// Errors from custom password validation.
    custom_errors: HashMap<String, Vec<String>>,
}

impl UserCreationForm {
    /// Creates a new user creation form.
    pub fn new() -> Self {
        Self {
            inner: BaseForm::new(vec![
                FormFieldDef::new(
                    "username",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(150),
                        strip: true,
                    },
                ),
                FormFieldDef::new(
                    "password1",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(128),
                        strip: false,
                    },
                ),
                FormFieldDef::new(
                    "password2",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(128),
                        strip: false,
                    },
                ),
            ]),
            custom_errors: HashMap::new(),
        }
    }

    /// Binds form data to this form.
    pub fn bind(&mut self, data: &QueryDict) {
        self.inner.bind(data);
        self.custom_errors.clear();
    }

    /// Returns whether this form is bound.
    pub fn is_bound(&self) -> bool {
        self.inner.is_bound()
    }

    /// Validates the form, including password match and strength checks.
    pub async fn is_valid(&mut self) -> bool {
        let base_valid = self.inner.is_valid().await;
        if !base_valid {
            return false;
        }

        // Check passwords match
        let password1 = self
            .inner
            .cleaned_data()
            .get("password1")
            .map(|v| format!("{v}"));
        let password2 = self
            .inner
            .cleaned_data()
            .get("password2")
            .map(|v| format!("{v}"));

        match (password1.as_deref(), password2.as_deref()) {
            (Some(p1), Some(p2)) if p1 != p2 => {
                self.custom_errors
                    .entry("password2".to_string())
                    .or_default()
                    .push("The two password fields didn't match.".to_string());
                false
            }
            (Some(p1), Some(_)) => {
                // Run password validators
                if let Err(errors) = crate::hashers::validate_password(p1) {
                    self.custom_errors
                        .entry("password1".to_string())
                        .or_default()
                        .extend(errors);
                    false
                } else {
                    true
                }
            }
            _ => false,
        }
    }

    /// Returns all errors (base form + custom).
    pub fn errors(&self) -> HashMap<String, Vec<String>> {
        let mut all_errors = self.inner.errors().clone();
        for (field, errors) in &self.custom_errors {
            all_errors
                .entry(field.clone())
                .or_default()
                .extend(errors.clone());
        }
        all_errors
    }

    /// Returns the cleaned data.
    pub fn cleaned_data(&self) -> &HashMap<String, Value> {
        self.inner.cleaned_data()
    }

    /// Returns the username from cleaned data.
    pub fn get_username(&self) -> Option<String> {
        self.inner
            .cleaned_data()
            .get("username")
            .map(|v| format!("{v}"))
    }

    /// Returns the password from cleaned data (password1).
    pub fn get_password(&self) -> Option<String> {
        self.inner
            .cleaned_data()
            .get("password1")
            .map(|v| format!("{v}"))
    }

    /// Returns the field definitions.
    pub fn field_defs(&self) -> &[FormFieldDef] {
        self.inner.fields()
    }
}

impl Default for UserCreationForm {
    fn default() -> Self {
        Self::new()
    }
}

// ── PasswordChangeForm ──────────────────────────────────────────────

/// Password change form that requires the old password.
///
/// Mirrors Django's `django.contrib.auth.forms.PasswordChangeForm`.
/// Requires the old password for verification, plus new password with confirmation.
pub struct PasswordChangeForm {
    inner: BaseForm,
    custom_errors: HashMap<String, Vec<String>>,
}

impl PasswordChangeForm {
    /// Creates a new password change form.
    pub fn new() -> Self {
        Self {
            inner: BaseForm::new(vec![
                FormFieldDef::new(
                    "old_password",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(128),
                        strip: false,
                    },
                ),
                FormFieldDef::new(
                    "new_password1",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(128),
                        strip: false,
                    },
                ),
                FormFieldDef::new(
                    "new_password2",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(128),
                        strip: false,
                    },
                ),
            ]),
            custom_errors: HashMap::new(),
        }
    }

    /// Binds form data to this form.
    pub fn bind(&mut self, data: &QueryDict) {
        self.inner.bind(data);
        self.custom_errors.clear();
    }

    /// Returns whether this form is bound.
    pub fn is_bound(&self) -> bool {
        self.inner.is_bound()
    }

    /// Validates the form, including password match and strength.
    pub async fn is_valid(&mut self) -> bool {
        let base_valid = self.inner.is_valid().await;
        if !base_valid {
            return false;
        }

        let p1 = self
            .inner
            .cleaned_data()
            .get("new_password1")
            .map(|v| format!("{v}"));
        let p2 = self
            .inner
            .cleaned_data()
            .get("new_password2")
            .map(|v| format!("{v}"));

        match (p1.as_deref(), p2.as_deref()) {
            (Some(p1), Some(p2)) if p1 != p2 => {
                self.custom_errors
                    .entry("new_password2".to_string())
                    .or_default()
                    .push("The two password fields didn't match.".to_string());
                false
            }
            (Some(p1), Some(_)) => {
                if let Err(errors) = crate::hashers::validate_password(p1) {
                    self.custom_errors
                        .entry("new_password1".to_string())
                        .or_default()
                        .extend(errors);
                    false
                } else {
                    true
                }
            }
            _ => false,
        }
    }

    /// Returns all errors.
    pub fn errors(&self) -> HashMap<String, Vec<String>> {
        let mut all_errors = self.inner.errors().clone();
        for (field, errors) in &self.custom_errors {
            all_errors
                .entry(field.clone())
                .or_default()
                .extend(errors.clone());
        }
        all_errors
    }

    /// Returns the cleaned data.
    pub fn cleaned_data(&self) -> &HashMap<String, Value> {
        self.inner.cleaned_data()
    }

    /// Returns the old password from cleaned data.
    pub fn get_old_password(&self) -> Option<String> {
        self.inner
            .cleaned_data()
            .get("old_password")
            .map(|v| format!("{v}"))
    }

    /// Returns the new password from cleaned data.
    pub fn get_new_password(&self) -> Option<String> {
        self.inner
            .cleaned_data()
            .get("new_password1")
            .map(|v| format!("{v}"))
    }

    /// Returns the field definitions.
    pub fn field_defs(&self) -> &[FormFieldDef] {
        self.inner.fields()
    }
}

impl Default for PasswordChangeForm {
    fn default() -> Self {
        Self::new()
    }
}

// ── SetPasswordForm ─────────────────────────────────────────────────

/// Set password form (no old password required).
///
/// Mirrors Django's `django.contrib.auth.forms.SetPasswordForm`.
/// Used for password reset flows where the old password is not available.
pub struct SetPasswordForm {
    inner: BaseForm,
    custom_errors: HashMap<String, Vec<String>>,
}

impl SetPasswordForm {
    /// Creates a new set password form.
    pub fn new() -> Self {
        Self {
            inner: BaseForm::new(vec![
                FormFieldDef::new(
                    "new_password1",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(128),
                        strip: false,
                    },
                ),
                FormFieldDef::new(
                    "new_password2",
                    FormFieldType::Char {
                        min_length: Some(1),
                        max_length: Some(128),
                        strip: false,
                    },
                ),
            ]),
            custom_errors: HashMap::new(),
        }
    }

    /// Binds form data to this form.
    pub fn bind(&mut self, data: &QueryDict) {
        self.inner.bind(data);
        self.custom_errors.clear();
    }

    /// Returns whether this form is bound.
    pub fn is_bound(&self) -> bool {
        self.inner.is_bound()
    }

    /// Validates the form.
    pub async fn is_valid(&mut self) -> bool {
        let base_valid = self.inner.is_valid().await;
        if !base_valid {
            return false;
        }

        let p1 = self
            .inner
            .cleaned_data()
            .get("new_password1")
            .map(|v| format!("{v}"));
        let p2 = self
            .inner
            .cleaned_data()
            .get("new_password2")
            .map(|v| format!("{v}"));

        match (p1.as_deref(), p2.as_deref()) {
            (Some(p1), Some(p2)) if p1 != p2 => {
                self.custom_errors
                    .entry("new_password2".to_string())
                    .or_default()
                    .push("The two password fields didn't match.".to_string());
                false
            }
            (Some(p1), Some(_)) => {
                if let Err(errors) = crate::hashers::validate_password(p1) {
                    self.custom_errors
                        .entry("new_password1".to_string())
                        .or_default()
                        .extend(errors);
                    false
                } else {
                    true
                }
            }
            _ => false,
        }
    }

    /// Returns all errors.
    pub fn errors(&self) -> HashMap<String, Vec<String>> {
        let mut all_errors = self.inner.errors().clone();
        for (field, errors) in &self.custom_errors {
            all_errors
                .entry(field.clone())
                .or_default()
                .extend(errors.clone());
        }
        all_errors
    }

    /// Returns the cleaned data.
    pub fn cleaned_data(&self) -> &HashMap<String, Value> {
        self.inner.cleaned_data()
    }

    /// Returns the new password from cleaned data.
    pub fn get_new_password(&self) -> Option<String> {
        self.inner
            .cleaned_data()
            .get("new_password1")
            .map(|v| format!("{v}"))
    }

    /// Returns the field definitions.
    pub fn field_defs(&self) -> &[FormFieldDef] {
        self.inner.fields()
    }
}

impl Default for SetPasswordForm {
    fn default() -> Self {
        Self::new()
    }
}

// ── PasswordResetForm ───────────────────────────────────────────────

/// Password reset request form (email field).
///
/// Mirrors Django's `django.contrib.auth.forms.PasswordResetForm`.
/// Contains a single email field to initiate the reset flow.
pub struct PasswordResetForm {
    inner: BaseForm,
}

impl PasswordResetForm {
    /// Creates a new password reset form.
    pub fn new() -> Self {
        Self {
            inner: BaseForm::new(vec![FormFieldDef::new("email", FormFieldType::Email)]),
        }
    }

    /// Binds form data to this form.
    pub fn bind(&mut self, data: &QueryDict) {
        self.inner.bind(data);
    }

    /// Returns whether this form is bound.
    pub fn is_bound(&self) -> bool {
        self.inner.is_bound()
    }

    /// Validates the form.
    pub async fn is_valid(&mut self) -> bool {
        self.inner.is_valid().await
    }

    /// Returns the form errors.
    pub fn errors(&self) -> &HashMap<String, Vec<String>> {
        self.inner.errors()
    }

    /// Returns the cleaned data.
    pub fn cleaned_data(&self) -> &HashMap<String, Value> {
        self.inner.cleaned_data()
    }

    /// Returns the email from cleaned data.
    pub fn get_email(&self) -> Option<String> {
        self.inner
            .cleaned_data()
            .get("email")
            .map(|v| format!("{v}"))
    }

    /// Returns the field definitions.
    pub fn field_defs(&self) -> &[FormFieldDef] {
        self.inner.fields()
    }
}

impl Default for PasswordResetForm {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── AuthenticationForm tests ────────────────────────────────────

    #[test]
    fn test_auth_form_new_unbound() {
        let form = AuthenticationForm::new();
        assert!(!form.is_bound());
    }

    #[test]
    fn test_auth_form_has_fields() {
        let form = AuthenticationForm::new();
        assert_eq!(form.field_defs().len(), 2);
        assert_eq!(form.field_defs()[0].name, "username");
        assert_eq!(form.field_defs()[1].name, "password");
    }

    #[tokio::test]
    async fn test_auth_form_valid() {
        let mut form = AuthenticationForm::new();
        let data = QueryDict::parse("username=alice&password=secret123");
        form.bind(&data);
        assert!(form.is_bound());
        assert!(form.is_valid().await);
        assert!(form.errors().is_empty());
    }

    #[tokio::test]
    async fn test_auth_form_missing_username() {
        let mut form = AuthenticationForm::new();
        let data = QueryDict::parse("password=secret123");
        form.bind(&data);
        assert!(!form.is_valid().await);
        assert!(form.errors().contains_key("username"));
    }

    #[tokio::test]
    async fn test_auth_form_missing_password() {
        let mut form = AuthenticationForm::new();
        let data = QueryDict::parse("username=alice");
        form.bind(&data);
        assert!(!form.is_valid().await);
        assert!(form.errors().contains_key("password"));
    }

    #[tokio::test]
    async fn test_auth_form_empty() {
        let mut form = AuthenticationForm::new();
        let data = QueryDict::parse("");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_auth_form_get_username() {
        let mut form = AuthenticationForm::new();
        let data = QueryDict::parse("username=alice&password=secret123");
        form.bind(&data);
        form.is_valid().await;
        assert_eq!(form.get_username().as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn test_auth_form_get_password() {
        let mut form = AuthenticationForm::new();
        let data = QueryDict::parse("username=alice&password=secret123");
        form.bind(&data);
        form.is_valid().await;
        assert_eq!(form.get_password().as_deref(), Some("secret123"));
    }

    #[tokio::test]
    async fn test_auth_form_add_error() {
        let mut form = AuthenticationForm::new();
        let data = QueryDict::parse("username=alice&password=secret123");
        form.bind(&data);
        form.is_valid().await;
        form.add_error("Invalid username or password.");
        let errors = form.errors();
        assert!(errors.contains_key("__all__"));
        assert!(errors["__all__"][0].contains("Invalid username"));
    }

    #[tokio::test]
    async fn test_auth_form_long_username_rejected() {
        let mut form = AuthenticationForm::new();
        let long_name = "a".repeat(151);
        let data = QueryDict::parse(&format!("username={long_name}&password=secret123"));
        form.bind(&data);
        assert!(!form.is_valid().await);
        assert!(form.errors().contains_key("username"));
    }

    #[test]
    fn test_auth_form_default() {
        let form = AuthenticationForm::default();
        assert!(!form.is_bound());
    }

    // ── UserCreationForm tests ──────────────────────────────────────

    #[test]
    fn test_creation_form_new() {
        let form = UserCreationForm::new();
        assert!(!form.is_bound());
        assert_eq!(form.field_defs().len(), 3);
    }

    #[tokio::test]
    async fn test_creation_form_valid() {
        let mut form = UserCreationForm::new();
        let data = QueryDict::parse("username=alice&password1=Str0ngP@ss!&password2=Str0ngP@ss!");
        form.bind(&data);
        assert!(form.is_valid().await);
        assert!(form.errors().is_empty());
    }

    #[tokio::test]
    async fn test_creation_form_passwords_dont_match() {
        let mut form = UserCreationForm::new();
        let data = QueryDict::parse("username=alice&password1=Str0ngP@ss!&password2=Different!");
        form.bind(&data);
        assert!(!form.is_valid().await);
        let errors = form.errors();
        assert!(errors.contains_key("password2"));
        assert!(errors["password2"][0].contains("didn't match"));
    }

    #[tokio::test]
    async fn test_creation_form_weak_password() {
        let mut form = UserCreationForm::new();
        // "123" is too short and common
        let data = QueryDict::parse("username=alice&password1=123&password2=123");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_creation_form_common_password() {
        let mut form = UserCreationForm::new();
        let data = QueryDict::parse("username=alice&password1=password&password2=password");
        form.bind(&data);
        assert!(!form.is_valid().await);
        let errors = form.errors();
        assert!(errors.contains_key("password1"));
    }

    #[tokio::test]
    async fn test_creation_form_numeric_password() {
        let mut form = UserCreationForm::new();
        let data = QueryDict::parse("username=alice&password1=12345678&password2=12345678");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_creation_form_missing_username() {
        let mut form = UserCreationForm::new();
        let data = QueryDict::parse("password1=Str0ngP@ss!&password2=Str0ngP@ss!");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_creation_form_missing_passwords() {
        let mut form = UserCreationForm::new();
        let data = QueryDict::parse("username=alice");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_creation_form_get_username() {
        let mut form = UserCreationForm::new();
        let data = QueryDict::parse("username=alice&password1=Str0ngP@ss!&password2=Str0ngP@ss!");
        form.bind(&data);
        form.is_valid().await;
        assert_eq!(form.get_username().as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn test_creation_form_get_password() {
        let mut form = UserCreationForm::new();
        let data = QueryDict::parse("username=alice&password1=Str0ngP@ss!&password2=Str0ngP@ss!");
        form.bind(&data);
        form.is_valid().await;
        assert_eq!(form.get_password().as_deref(), Some("Str0ngP@ss!"));
    }

    #[tokio::test]
    async fn test_creation_form_rebind_clears_errors() {
        let mut form = UserCreationForm::new();
        let data = QueryDict::parse("username=alice&password1=Str0ngP@ss!&password2=Different!");
        form.bind(&data);
        form.is_valid().await;
        assert!(!form.errors().is_empty());

        let data2 = QueryDict::parse("username=alice&password1=Str0ngP@ss!&password2=Str0ngP@ss!");
        form.bind(&data2);
        form.is_valid().await;
        assert!(form.errors().is_empty());
    }

    #[test]
    fn test_creation_form_default() {
        let form = UserCreationForm::default();
        assert!(!form.is_bound());
    }

    // ── PasswordChangeForm tests ────────────────────────────────────

    #[test]
    fn test_password_change_form_new() {
        let form = PasswordChangeForm::new();
        assert!(!form.is_bound());
        assert_eq!(form.field_defs().len(), 3);
    }

    #[tokio::test]
    async fn test_password_change_valid() {
        let mut form = PasswordChangeForm::new();
        let data = QueryDict::parse(
            "old_password=OldP@ss123&new_password1=NewStr0ng!&new_password2=NewStr0ng!",
        );
        form.bind(&data);
        assert!(form.is_valid().await);
        assert!(form.errors().is_empty());
    }

    #[tokio::test]
    async fn test_password_change_passwords_dont_match() {
        let mut form = PasswordChangeForm::new();
        let data = QueryDict::parse(
            "old_password=OldP@ss123&new_password1=NewStr0ng!&new_password2=Different!",
        );
        form.bind(&data);
        assert!(!form.is_valid().await);
        let errors = form.errors();
        assert!(errors.contains_key("new_password2"));
    }

    #[tokio::test]
    async fn test_password_change_weak_new_password() {
        let mut form = PasswordChangeForm::new();
        let data = QueryDict::parse(
            "old_password=OldP@ss123&new_password1=password&new_password2=password",
        );
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_password_change_missing_old() {
        let mut form = PasswordChangeForm::new();
        let data = QueryDict::parse("new_password1=NewStr0ng!&new_password2=NewStr0ng!");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_password_change_get_old_password() {
        let mut form = PasswordChangeForm::new();
        let data = QueryDict::parse(
            "old_password=OldP@ss123&new_password1=NewStr0ng!&new_password2=NewStr0ng!",
        );
        form.bind(&data);
        form.is_valid().await;
        assert_eq!(form.get_old_password().as_deref(), Some("OldP@ss123"));
    }

    #[tokio::test]
    async fn test_password_change_get_new_password() {
        let mut form = PasswordChangeForm::new();
        let data = QueryDict::parse(
            "old_password=OldP@ss123&new_password1=NewStr0ng!&new_password2=NewStr0ng!",
        );
        form.bind(&data);
        form.is_valid().await;
        assert_eq!(form.get_new_password().as_deref(), Some("NewStr0ng!"));
    }

    #[tokio::test]
    async fn test_password_change_rebind_clears_errors() {
        let mut form = PasswordChangeForm::new();
        let data = QueryDict::parse(
            "old_password=OldP@ss123&new_password1=NewStr0ng!&new_password2=Different!",
        );
        form.bind(&data);
        form.is_valid().await;
        assert!(!form.errors().is_empty());

        let data2 = QueryDict::parse(
            "old_password=OldP@ss123&new_password1=NewStr0ng!&new_password2=NewStr0ng!",
        );
        form.bind(&data2);
        form.is_valid().await;
        assert!(form.errors().is_empty());
    }

    #[test]
    fn test_password_change_default() {
        let form = PasswordChangeForm::default();
        assert!(!form.is_bound());
    }

    // ── SetPasswordForm tests ───────────────────────────────────────

    #[test]
    fn test_set_password_form_new() {
        let form = SetPasswordForm::new();
        assert!(!form.is_bound());
        assert_eq!(form.field_defs().len(), 2);
    }

    #[tokio::test]
    async fn test_set_password_valid() {
        let mut form = SetPasswordForm::new();
        let data = QueryDict::parse("new_password1=NewStr0ng!&new_password2=NewStr0ng!");
        form.bind(&data);
        assert!(form.is_valid().await);
        assert!(form.errors().is_empty());
    }

    #[tokio::test]
    async fn test_set_password_dont_match() {
        let mut form = SetPasswordForm::new();
        let data = QueryDict::parse("new_password1=NewStr0ng!&new_password2=Different!");
        form.bind(&data);
        assert!(!form.is_valid().await);
        let errors = form.errors();
        assert!(errors.contains_key("new_password2"));
    }

    #[tokio::test]
    async fn test_set_password_weak() {
        let mut form = SetPasswordForm::new();
        let data = QueryDict::parse("new_password1=password&new_password2=password");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_set_password_get_new_password() {
        let mut form = SetPasswordForm::new();
        let data = QueryDict::parse("new_password1=NewStr0ng!&new_password2=NewStr0ng!");
        form.bind(&data);
        form.is_valid().await;
        assert_eq!(form.get_new_password().as_deref(), Some("NewStr0ng!"));
    }

    #[tokio::test]
    async fn test_set_password_missing_fields() {
        let mut form = SetPasswordForm::new();
        let data = QueryDict::parse("");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_set_password_missing_confirmation() {
        let mut form = SetPasswordForm::new();
        let data = QueryDict::parse("new_password1=NewStr0ng!");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_set_password_numeric() {
        let mut form = SetPasswordForm::new();
        let data = QueryDict::parse("new_password1=12345678&new_password2=12345678");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[test]
    fn test_set_password_default() {
        let form = SetPasswordForm::default();
        assert!(!form.is_bound());
    }

    // ── PasswordResetForm tests ─────────────────────────────────────

    #[test]
    fn test_reset_form_new() {
        let form = PasswordResetForm::new();
        assert!(!form.is_bound());
        assert_eq!(form.field_defs().len(), 1);
    }

    #[tokio::test]
    async fn test_reset_form_valid_email() {
        let mut form = PasswordResetForm::new();
        let data = QueryDict::parse("email=alice@example.com");
        form.bind(&data);
        assert!(form.is_valid().await);
        assert!(form.errors().is_empty());
    }

    #[tokio::test]
    async fn test_reset_form_invalid_email() {
        let mut form = PasswordResetForm::new();
        let data = QueryDict::parse("email=not-an-email");
        form.bind(&data);
        assert!(!form.is_valid().await);
        assert!(form.errors().contains_key("email"));
    }

    #[tokio::test]
    async fn test_reset_form_missing_email() {
        let mut form = PasswordResetForm::new();
        let data = QueryDict::parse("");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[tokio::test]
    async fn test_reset_form_get_email() {
        let mut form = PasswordResetForm::new();
        let data = QueryDict::parse("email=alice@example.com");
        form.bind(&data);
        form.is_valid().await;
        assert_eq!(form.get_email().as_deref(), Some("alice@example.com"));
    }

    #[tokio::test]
    async fn test_reset_form_empty_email() {
        let mut form = PasswordResetForm::new();
        let data = QueryDict::parse("email=");
        form.bind(&data);
        assert!(!form.is_valid().await);
    }

    #[test]
    fn test_reset_form_default() {
        let form = PasswordResetForm::default();
        assert!(!form.is_bound());
    }
}
