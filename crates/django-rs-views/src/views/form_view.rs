//! Form-view integration for django-rs.
//!
//! This module provides helpers to connect the forms framework to views,
//! enabling extraction of POST data from requests, binding to forms,
//! validation, and template rendering with form errors.
//!
//! ## Key Functions
//!
//! - [`bind_form_from_request`] - Extracts POST data from an `HttpRequest` and binds it to a form
//! - [`extract_post_data`] - Extracts form data from the request body as a `QueryDict`
//! - [`form_context_to_json`] - Converts form context (ContextValues) to serde_json for views

use std::collections::HashMap;

use django_rs_forms::form::Form;
use django_rs_http::{HttpRequest, QueryDict};
use django_rs_template::context::ContextValue;

/// Extracts POST form data from an `HttpRequest`.
///
/// Parses the request body as URL-encoded form data if the content type is
/// `application/x-www-form-urlencoded`. Otherwise returns an empty `QueryDict`.
///
/// # Examples
///
/// ```
/// use django_rs_views::views::form_view::extract_post_data;
/// use django_rs_http::HttpRequest;
///
/// let request = HttpRequest::builder()
///     .method(http::Method::POST)
///     .content_type("application/x-www-form-urlencoded")
///     .body(b"name=Alice&email=alice@example.com".to_vec())
///     .build();
///
/// let data = extract_post_data(&request);
/// assert_eq!(data.get("name"), Some("Alice"));
/// ```
pub fn extract_post_data(request: &HttpRequest) -> QueryDict {
    // The HttpRequest already parses POST data in its constructor
    // We can use the post() method directly, but we return a clone
    // since QueryDict is immutable by default
    request.post().clone()
}

/// Binds form data from an HTTP request to a form.
///
/// Extracts URL-encoded POST data from the request body and calls
/// `form.bind()` with it. After this call, the form is bound and
/// ready for validation via `form.is_valid()`.
///
/// # Examples
///
/// ```
/// use django_rs_views::views::form_view::bind_form_from_request;
/// use django_rs_forms::form::{BaseForm, Form};
/// use django_rs_forms::fields::{FormFieldDef, FormFieldType};
/// use django_rs_http::HttpRequest;
///
/// let mut form = BaseForm::new(vec![
///     FormFieldDef::new("name", FormFieldType::Char {
///         min_length: None, max_length: None, strip: false,
///     }),
/// ]);
///
/// let request = HttpRequest::builder()
///     .method(http::Method::POST)
///     .content_type("application/x-www-form-urlencoded")
///     .body(b"name=Alice".to_vec())
///     .build();
///
/// bind_form_from_request(&mut form, &request);
/// assert!(form.is_bound());
/// ```
pub fn bind_form_from_request(form: &mut dyn Form, request: &HttpRequest) {
    let data = extract_post_data(request);
    form.bind(&data);
}

/// Converts a form's template context (HashMap<String, ContextValue>) to
/// serde_json::Value for use in generic view contexts.
///
/// This bridges the gap between the forms framework (which uses ContextValue)
/// and the views framework (which uses serde_json::Value).
pub fn form_context_to_json(
    form_ctx: &HashMap<String, ContextValue>,
) -> HashMap<String, serde_json::Value> {
    form_ctx
        .iter()
        .map(|(k, v)| (k.clone(), context_value_to_json(v)))
        .collect()
}

/// Converts a `ContextValue` to a `serde_json::Value`.
fn context_value_to_json(value: &ContextValue) -> serde_json::Value {
    match value {
        ContextValue::String(s) | ContextValue::SafeString(s) => {
            serde_json::Value::String(s.clone())
        }
        ContextValue::Integer(i) => serde_json::json!(i),
        ContextValue::Float(f) => serde_json::json!(f),
        ContextValue::Bool(b) => serde_json::Value::Bool(*b),
        ContextValue::None => serde_json::Value::Null,
        ContextValue::List(items) => {
            let arr: Vec<serde_json::Value> =
                items.iter().map(context_value_to_json).collect();
            serde_json::Value::Array(arr)
        }
        ContextValue::Dict(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), context_value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
    }
}

/// Helper to get cleaned data from a valid form as a `HashMap<String, String>`.
///
/// Converts `Value` types to strings for simpler processing in view handlers.
pub fn cleaned_data_as_strings(form: &dyn Form) -> HashMap<String, String> {
    form.cleaned_data()
        .iter()
        .map(|(k, v)| (k.clone(), format!("{v}")))
        .collect()
}

/// Helper to get form errors as a `HashMap<String, Vec<String>>`.
pub fn form_errors(form: &dyn Form) -> HashMap<String, Vec<String>> {
    form.errors().clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use django_rs_forms::fields::{FormFieldDef, FormFieldType};
    use django_rs_forms::form::BaseForm;

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
            FormFieldDef::new("email", FormFieldType::Email),
        ])
    }

    #[test]
    fn test_extract_post_data_form_encoded() {
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"username=alice&email=alice@example.com".to_vec())
            .build();

        let data = extract_post_data(&request);
        assert_eq!(data.get("username"), Some("alice"));
        assert_eq!(data.get("email"), Some("alice@example.com"));
    }

    #[test]
    fn test_extract_post_data_empty() {
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();

        let data = extract_post_data(&request);
        assert!(data.is_empty());
    }

    #[test]
    fn test_bind_form_from_request() {
        let mut form = make_test_form();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"username=alice&email=alice@example.com".to_vec())
            .build();

        bind_form_from_request(&mut form, &request);
        assert!(form.is_bound());
    }

    #[tokio::test]
    async fn test_bind_form_valid() {
        let mut form = make_test_form();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"username=alice&email=alice@example.com".to_vec())
            .build();

        bind_form_from_request(&mut form, &request);
        assert!(form.is_valid().await);
        assert!(form.errors().is_empty());
    }

    #[tokio::test]
    async fn test_bind_form_invalid() {
        let mut form = make_test_form();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"username=ab&email=not-email".to_vec())
            .build();

        bind_form_from_request(&mut form, &request);
        assert!(!form.is_valid().await);
        assert!(form.errors().contains_key("username"));
        assert!(form.errors().contains_key("email"));
    }

    #[tokio::test]
    async fn test_bind_form_missing_fields() {
        let mut form = make_test_form();
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"".to_vec())
            .build();

        bind_form_from_request(&mut form, &request);
        assert!(!form.is_valid().await);
        assert!(form.errors().contains_key("username"));
        assert!(form.errors().contains_key("email"));
    }

    #[tokio::test]
    async fn test_cleaned_data_as_strings() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=alice&email=alice@example.com");
        form.bind(&qd);
        form.is_valid().await;

        let strings = cleaned_data_as_strings(&form);
        assert!(strings.contains_key("username"));
        assert!(strings.contains_key("email"));
    }

    #[tokio::test]
    async fn test_form_errors_helper() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=ab&email=bad");
        form.bind(&qd);
        form.is_valid().await;

        let errors = form_errors(&form);
        assert!(!errors.is_empty());
        assert!(errors.contains_key("username"));
    }

    #[test]
    fn test_form_context_to_json() {
        let mut ctx = HashMap::new();
        ctx.insert(
            "is_bound".to_string(),
            ContextValue::Bool(true),
        );
        ctx.insert(
            "name".to_string(),
            ContextValue::String("test".to_string()),
        );

        let json = form_context_to_json(&ctx);
        assert_eq!(json.get("is_bound"), Some(&serde_json::json!(true)));
        assert_eq!(json.get("name"), Some(&serde_json::json!("test")));
    }

    #[test]
    fn test_context_value_to_json_types() {
        assert_eq!(
            context_value_to_json(&ContextValue::Integer(42)),
            serde_json::json!(42)
        );
        assert_eq!(
            context_value_to_json(&ContextValue::Float(3.14)),
            serde_json::json!(3.14)
        );
        assert_eq!(
            context_value_to_json(&ContextValue::Bool(true)),
            serde_json::json!(true)
        );
        assert_eq!(
            context_value_to_json(&ContextValue::None),
            serde_json::Value::Null
        );
        assert_eq!(
            context_value_to_json(&ContextValue::String("hello".to_string())),
            serde_json::json!("hello")
        );
        assert_eq!(
            context_value_to_json(&ContextValue::SafeString("<b>safe</b>".to_string())),
            serde_json::json!("<b>safe</b>")
        );

        let list = ContextValue::List(vec![
            ContextValue::Integer(1),
            ContextValue::Integer(2),
        ]);
        assert_eq!(
            context_value_to_json(&list),
            serde_json::json!([1, 2])
        );

        let mut dict = HashMap::new();
        dict.insert("key".to_string(), ContextValue::String("value".to_string()));
        let dict_val = ContextValue::Dict(dict);
        assert_eq!(
            context_value_to_json(&dict_val),
            serde_json::json!({"key": "value"})
        );
    }

    #[tokio::test]
    async fn test_form_as_context_to_json() {
        let mut form = make_test_form();
        let qd = QueryDict::parse("username=alice&email=alice@example.com");
        form.bind(&qd);
        form.is_valid().await;

        let ctx = form.as_context();
        let json = form_context_to_json(&ctx);
        assert!(json.contains_key("fields"));
        assert!(json.contains_key("errors"));
        assert!(json.contains_key("is_bound"));
    }

    #[test]
    fn test_extract_post_data_json_body_ignored() {
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/json")
            .body(b"{\"key\": \"value\"}".to_vec())
            .build();

        let data = extract_post_data(&request);
        assert!(data.is_empty());
    }

    #[tokio::test]
    async fn test_full_form_submission_flow() {
        // Simulate: GET shows empty form, POST with good data validates
        let mut form = make_test_form();
        assert!(!form.is_bound());

        // POST with valid data
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .content_type("application/x-www-form-urlencoded")
            .body(b"username=alice&email=alice@example.com".to_vec())
            .build();

        bind_form_from_request(&mut form, &request);
        assert!(form.is_bound());
        assert!(form.is_valid().await);

        let cleaned = cleaned_data_as_strings(&form);
        assert!(!cleaned.is_empty());
    }
}
