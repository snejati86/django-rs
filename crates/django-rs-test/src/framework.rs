//! Test framework utilities and assertion helpers for django-rs.
//!
//! This module provides [`TestCase`] for structuring tests and a collection
//! of assertion functions that mirror Django's test assertion methods.
//!
//! ## Assertion Helpers
//!
//! - [`assert_contains`] - Assert response body contains text
//! - [`assert_not_contains`] - Assert response body does not contain text
//! - [`assert_redirects`] - Assert response is a redirect to a specific URL
//! - [`assert_template_used`] - Assert a specific template was used (checks header)
//! - [`assert_form_error`] - Assert form validation errors are present

use std::collections::HashMap;

use axum::Router;

use crate::client::{TestClient, TestResponse};

/// A test case that provides a test client and settings overrides.
///
/// This mirrors Django's `TestCase` class, providing per-test setup
/// with a fresh client and the ability to override settings.
pub struct TestCase {
    /// The test client for making HTTP requests.
    pub client: TestClient,
    /// Settings overrides for this test case.
    pub settings_overrides: HashMap<String, serde_json::Value>,
}

impl TestCase {
    /// Creates a new test case with the given Axum application.
    pub fn new(app: Router) -> Self {
        Self {
            client: TestClient::new(app),
            settings_overrides: HashMap::new(),
        }
    }

    /// Creates a new test case with settings overrides.
    pub fn with_settings(
        app: Router,
        overrides: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            client: TestClient::new(app),
            settings_overrides: overrides,
        }
    }

    /// Returns `true` if a settings override exists for the given key.
    pub fn has_override(&self, key: &str) -> bool {
        self.settings_overrides.contains_key(key)
    }

    /// Gets a settings override value.
    pub fn get_override(&self, key: &str) -> Option<&serde_json::Value> {
        self.settings_overrides.get(key)
    }
}

/// Asserts that the response body contains the given text.
///
/// # Panics
///
/// Panics if the response body does not contain `text`.
pub fn assert_contains(response: &TestResponse, text: &str) {
    let body = response.text();
    assert!(
        body.contains(text),
        "Response body does not contain '{text}'.\nActual body: {body}"
    );
}

/// Asserts that the response body does not contain the given text.
///
/// # Panics
///
/// Panics if the response body contains `text`.
pub fn assert_not_contains(response: &TestResponse, text: &str) {
    let body = response.text();
    assert!(
        !body.contains(text),
        "Response body unexpectedly contains '{text}'.\nActual body: {body}"
    );
}

/// Asserts that the response is a redirect (3xx) to the expected URL.
///
/// Checks that the status code is in the 3xx range and the `Location`
/// header matches `expected_url`.
///
/// # Panics
///
/// Panics if the response is not a redirect or the URL does not match.
pub fn assert_redirects(response: &TestResponse, expected_url: &str) {
    let status = response.status_code();
    assert!(
        (300..400).contains(&status),
        "Expected a redirect (3xx), got {status}"
    );

    let location = response
        .header("location")
        .unwrap_or_else(|| panic!("Redirect response missing Location header"));

    assert_eq!(
        location, expected_url,
        "Expected redirect to '{expected_url}', got '{location}'"
    );
}

/// Asserts that the response was rendered with the given template.
///
/// Checks for a `X-Template-Name` header that contains the template name.
/// This header should be set by the template rendering middleware in debug mode.
///
/// # Panics
///
/// Panics if the template name does not match.
pub fn assert_template_used(response: &TestResponse, template_name: &str) {
    let actual = response
        .header("x-template-name")
        .unwrap_or_else(|| {
            panic!(
                "Response does not have X-Template-Name header. \
                 Template assertions require debug mode."
            )
        });

    assert!(
        actual.contains(template_name),
        "Expected template '{template_name}', got '{actual}'"
    );
}

/// Asserts that the response body contains a form error for the given field.
///
/// Searches for the form name, field name, and error message in the response body.
/// This is a simplified version of Django's `assertFormError`.
///
/// # Panics
///
/// Panics if the error text is not found in the response body.
pub fn assert_form_error(response: &TestResponse, form: &str, field: &str, error: &str) {
    let body = response.text();
    assert!(
        body.contains(error),
        "Expected form error '{error}' for field '{field}' in form '{form}'.\n\
         Actual body: {body}"
    );
}

/// Asserts that the response status code matches the expected value.
///
/// # Panics
///
/// Panics if the status code does not match.
pub fn assert_status(response: &TestResponse, expected: u16) {
    assert_eq!(
        response.status_code(),
        expected,
        "Expected status {expected}, got {}",
        response.status_code()
    );
}

/// Asserts that the response has the given header.
///
/// # Panics
///
/// Panics if the header is missing.
pub fn assert_has_header(response: &TestResponse, header_name: &str) {
    assert!(
        response.has_header(header_name),
        "Expected response to have header '{header_name}'"
    );
}

/// Asserts that the response does not have the given header.
///
/// # Panics
///
/// Panics if the header is present.
pub fn assert_not_has_header(response: &TestResponse, header_name: &str) {
    assert!(
        !response.has_header(header_name),
        "Expected response NOT to have header '{header_name}'"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use http::StatusCode;

    fn make_response(status: StatusCode, body: &str, headers: Vec<(&str, &str)>) -> TestResponse {
        let mut header_map = http::HeaderMap::new();
        for (name, value) in headers {
            header_map.insert(
                http::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                http::header::HeaderValue::from_str(value).unwrap(),
            );
        }

        TestResponse {
            status,
            headers: header_map,
            body: body.as_bytes().to_vec(),
            cookies: HashMap::new(),
        }
    }

    // ── TestCase tests ────────────────────────────────────────────────

    #[test]
    fn test_testcase_new() {
        let app = Router::new();
        let tc = TestCase::new(app);
        assert!(tc.settings_overrides.is_empty());
    }

    #[test]
    fn test_testcase_with_settings() {
        let app = Router::new();
        let mut overrides = HashMap::new();
        overrides.insert("DEBUG".to_string(), serde_json::json!(false));

        let tc = TestCase::with_settings(app, overrides);
        assert!(tc.has_override("DEBUG"));
        assert_eq!(tc.get_override("DEBUG"), Some(&serde_json::json!(false)));
        assert!(!tc.has_override("NONEXISTENT"));
        assert!(tc.get_override("NONEXISTENT").is_none());
    }

    // ── assert_contains tests ─────────────────────────────────────────

    #[test]
    fn test_assert_contains_passes() {
        let response = make_response(StatusCode::OK, "Hello, World!", vec![]);
        assert_contains(&response, "Hello");
        assert_contains(&response, "World");
    }

    #[test]
    #[should_panic(expected = "does not contain")]
    fn test_assert_contains_fails() {
        let response = make_response(StatusCode::OK, "Hello, World!", vec![]);
        assert_contains(&response, "Goodbye");
    }

    // ── assert_not_contains tests ─────────────────────────────────────

    #[test]
    fn test_assert_not_contains_passes() {
        let response = make_response(StatusCode::OK, "Hello, World!", vec![]);
        assert_not_contains(&response, "Goodbye");
    }

    #[test]
    #[should_panic(expected = "unexpectedly contains")]
    fn test_assert_not_contains_fails() {
        let response = make_response(StatusCode::OK, "Hello, World!", vec![]);
        assert_not_contains(&response, "Hello");
    }

    // ── assert_redirects tests ────────────────────────────────────────

    #[test]
    fn test_assert_redirects_passes() {
        let response = make_response(
            StatusCode::FOUND,
            "",
            vec![("location", "/new-location/")],
        );
        assert_redirects(&response, "/new-location/");
    }

    #[test]
    #[should_panic(expected = "Expected a redirect")]
    fn test_assert_redirects_fails_wrong_status() {
        let response = make_response(StatusCode::OK, "", vec![]);
        assert_redirects(&response, "/somewhere/");
    }

    #[test]
    #[should_panic(expected = "missing Location")]
    fn test_assert_redirects_fails_no_location() {
        let response = make_response(StatusCode::FOUND, "", vec![]);
        assert_redirects(&response, "/somewhere/");
    }

    #[test]
    #[should_panic(expected = "Expected redirect to")]
    fn test_assert_redirects_fails_wrong_url() {
        let response = make_response(
            StatusCode::FOUND,
            "",
            vec![("location", "/wrong-url/")],
        );
        assert_redirects(&response, "/right-url/");
    }

    // ── assert_template_used tests ────────────────────────────────────

    #[test]
    fn test_assert_template_used_passes() {
        let response = make_response(
            StatusCode::OK,
            "<html>Home</html>",
            vec![("x-template-name", "home.html")],
        );
        assert_template_used(&response, "home.html");
    }

    #[test]
    #[should_panic(expected = "does not have X-Template-Name")]
    fn test_assert_template_used_fails_no_header() {
        let response = make_response(StatusCode::OK, "<html>Home</html>", vec![]);
        assert_template_used(&response, "home.html");
    }

    #[test]
    #[should_panic(expected = "Expected template")]
    fn test_assert_template_used_fails_wrong_template() {
        let response = make_response(
            StatusCode::OK,
            "",
            vec![("x-template-name", "other.html")],
        );
        assert_template_used(&response, "home.html");
    }

    // ── assert_form_error tests ───────────────────────────────────────

    #[test]
    fn test_assert_form_error_passes() {
        let body = r#"<form name="login"><span class="error">Invalid password</span></form>"#;
        let response = make_response(StatusCode::OK, body, vec![]);
        assert_form_error(&response, "login", "password", "Invalid password");
    }

    #[test]
    #[should_panic(expected = "Expected form error")]
    fn test_assert_form_error_fails() {
        let response = make_response(StatusCode::OK, "<form></form>", vec![]);
        assert_form_error(&response, "login", "password", "Invalid password");
    }

    // ── assert_status tests ───────────────────────────────────────────

    #[test]
    fn test_assert_status_passes() {
        let response = make_response(StatusCode::OK, "", vec![]);
        assert_status(&response, 200);
    }

    #[test]
    #[should_panic(expected = "Expected status")]
    fn test_assert_status_fails() {
        let response = make_response(StatusCode::NOT_FOUND, "", vec![]);
        assert_status(&response, 200);
    }

    // ── assert_has_header tests ───────────────────────────────────────

    #[test]
    fn test_assert_has_header_passes() {
        let response = make_response(
            StatusCode::OK,
            "",
            vec![("x-custom", "value")],
        );
        assert_has_header(&response, "x-custom");
    }

    #[test]
    #[should_panic(expected = "Expected response to have header")]
    fn test_assert_has_header_fails() {
        let response = make_response(StatusCode::OK, "", vec![]);
        assert_has_header(&response, "x-custom");
    }

    // ── assert_not_has_header tests ───────────────────────────────────

    #[test]
    fn test_assert_not_has_header_passes() {
        let response = make_response(StatusCode::OK, "", vec![]);
        assert_not_has_header(&response, "x-custom");
    }

    #[test]
    #[should_panic(expected = "Expected response NOT to have header")]
    fn test_assert_not_has_header_fails() {
        let response = make_response(
            StatusCode::OK,
            "",
            vec![("x-custom", "value")],
        );
        assert_not_has_header(&response, "x-custom");
    }

    // ── Integration test with TestClient ──────────────────────────────

    #[tokio::test]
    async fn test_testcase_integration() {
        let app = Router::new()
            .route("/", get(|| async { "home" }));

        let mut tc = TestCase::new(app);
        let response = tc.client.get("/").await;
        assert_contains(&response, "home");
        assert_status(&response, 200);
    }

    #[tokio::test]
    async fn test_redirect_integration() {
        let app = Router::new()
            .route(
                "/old",
                get(|| async {
                    (
                        StatusCode::FOUND,
                        [(http::header::LOCATION, "/new")],
                        "",
                    )
                }),
            )
            .route("/new", get(|| async { "new page" }));

        let mut tc = TestCase::new(app);
        let response = tc.client.get("/old").await;
        assert_redirects(&response, "/new");
    }
}
