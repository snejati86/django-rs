//! URL path pattern parsing and matching.
//!
//! This module provides [`URLPattern`] for defining URL routes using either
//! Django-style `path()` syntax (e.g., `articles/<int:year>/`) or regex-based
//! `re_path()` syntax. It mirrors Django's `django.urls.path()` and `re_path()`.

use std::collections::HashMap;
use std::fmt;
use std::fmt::Write as _;
use std::sync::Arc;

use regex::Regex;

use django_rs_core::{DjangoError, DjangoResult};

use super::converters::{self, PathConverter};

/// A named converter entry: `(parameter_name, converter)`.
pub type ConverterEntry = (String, Box<dyn PathConverter>);

/// The type for route handler functions.
///
/// A handler is an async function that takes an [`HttpRequest`](crate::HttpRequest)
/// and returns an [`HttpResponse`](crate::HttpResponse). It is wrapped in an `Arc`
/// so it can be shared across threads.
pub type RouteHandler = Arc<dyn Fn(crate::HttpRequest) -> crate::BoxFuture + Send + Sync>;

/// A single URL pattern that matches a path and invokes a handler.
///
/// Combines a regex pattern, named path converters, and a route handler.
/// This is the Rust equivalent of a Django `URLPattern` created by `path()` or `re_path()`.
pub struct URLPattern {
    /// The original route string (e.g., `"articles/<int:year>/"`)
    route: String,
    /// The compiled regex used for matching
    regex: Regex,
    /// An optional name for reverse URL lookup
    name: Option<String>,
    /// Named converters extracted from the route, in order
    converters: Vec<ConverterEntry>,
    /// The handler function to invoke on match
    callback: RouteHandler,
}

impl fmt::Debug for URLPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("URLPattern")
            .field("route", &self.route)
            .field("regex", &self.regex.as_str())
            .field("name", &self.name)
            .field("converters", &self.converters)
            .finish_non_exhaustive()
    }
}

impl URLPattern {
    /// Returns the original route string.
    pub fn route(&self) -> &str {
        &self.route
    }

    /// Returns the compiled regex pattern.
    pub const fn regex(&self) -> &Regex {
        &self.regex
    }

    /// Returns the optional name for this pattern.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the named converters for this pattern.
    pub fn converters(&self) -> &[ConverterEntry] {
        &self.converters
    }

    /// Returns a reference to the callback handler.
    pub fn callback(&self) -> &RouteHandler {
        &self.callback
    }

    /// Attempts to match the given path against this pattern.
    ///
    /// Returns `Some((matched_kwargs, remaining_path))` on success, where
    /// `matched_kwargs` maps parameter names to their string values and
    /// `remaining_path` is the part of the path after the match.
    ///
    /// Returns `None` if the path does not match.
    pub fn match_path(&self, path: &str) -> Option<(HashMap<String, String>, String)> {
        let captures = self.regex.captures(path)?;
        let full_match = captures.get(0)?;

        let mut kwargs = HashMap::new();

        if self.converters.is_empty() {
            // For re_path patterns: extract all named groups from the regex
            for name in self.regex.capture_names().flatten() {
                if let Some(m) = captures.name(name) {
                    kwargs.insert(name.to_string(), m.as_str().to_string());
                }
            }
        } else {
            // For path patterns: validate through converters
            for (name, converter) in &self.converters {
                if let Some(m) = captures.name(name) {
                    let raw = m.as_str();
                    if converter.to_rust(raw).is_ok() {
                        kwargs.insert(name.clone(), raw.to_string());
                    } else {
                        return None;
                    }
                }
            }
        }

        let remaining = &path[full_match.end()..];
        Some((kwargs, remaining.to_string()))
    }

    /// Attempts a full match of the path (no remaining portion allowed).
    ///
    /// Returns the matched kwargs on success, or `None` if the path does not
    /// match or has unmatched trailing content.
    pub fn full_match(&self, path: &str) -> Option<HashMap<String, String>> {
        let (kwargs, remaining) = self.match_path(path)?;
        if remaining.is_empty() {
            Some(kwargs)
        } else {
            None
        }
    }
}

/// Parses the `<type:name>` portion of a pattern segment, returning `(type_name, param_name)`.
/// Defaults to `"str"` if no colon is present.
fn parse_type_and_name(inner: &str) -> (&str, &str) {
    inner
        .find(':')
        .map_or(("str", inner), |pos| (&inner[..pos], &inner[pos + 1..]))
}

/// Parses a Django-style path pattern (e.g., `"articles/<int:year>/"`) into
/// a regex string and a list of named converters.
///
/// # Errors
///
/// Returns an error if a converter type is unknown.
#[allow(clippy::type_complexity)]
fn parse_django_pattern(route: &str) -> DjangoResult<(String, Vec<ConverterEntry>)> {
    let mut regex_parts = String::from("^");
    let mut converter_list = Vec::new();
    let mut remaining = route;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find('<') {
            // Add the literal prefix
            let prefix = &remaining[..start];
            regex_parts.push_str(&regex::escape(prefix));

            let end = remaining[start..].find('>').ok_or_else(|| {
                DjangoError::ImproperlyConfigured(format!(
                    "Unclosed angle bracket in route: {route}"
                ))
            })? + start;

            let inner = &remaining[start + 1..end];
            let (type_name, param_name) = parse_type_and_name(inner);

            let converter = converters::get_converter(type_name)?;
            let converter_regex = converter.regex();

            write!(regex_parts, "(?P<{param_name}>{converter_regex})").ok();
            converter_list.push((param_name.to_string(), converter));

            remaining = &remaining[end + 1..];
        } else {
            // No more angle brackets, add the rest as literal
            regex_parts.push_str(&regex::escape(remaining));
            break;
        }
    }

    regex_parts.push('$');
    Ok((regex_parts, converter_list))
}

/// Creates a URL pattern using Django's `path()` syntax.
///
/// The route string may contain `<type:name>` placeholders that are converted
/// to regex capture groups. Supported types are `int`, `str`, `slug`, `uuid`,
/// and `path`.
///
/// # Examples
///
/// ```
/// use django_rs_http::urls::pattern::path;
/// use django_rs_http::{HttpRequest, HttpResponse};
/// use std::sync::Arc;
///
/// let handler = Arc::new(|_req: HttpRequest| -> django_rs_http::BoxFuture {
///     Box::pin(async { HttpResponse::ok("Hello") })
/// });
///
/// let pattern = path("articles/<int:year>/", handler, Some("article-detail")).unwrap();
/// assert_eq!(pattern.name(), Some("article-detail"));
/// ```
///
/// # Errors
///
/// Returns an error if the route contains unknown converter types or invalid syntax.
pub fn path(route: &str, callback: RouteHandler, name: Option<&str>) -> DjangoResult<URLPattern> {
    let (regex_str, converter_list) = parse_django_pattern(route)?;
    let regex = Regex::new(&regex_str)
        .map_err(|e| DjangoError::ImproperlyConfigured(format!("Invalid pattern regex: {e}")))?;

    Ok(URLPattern {
        route: route.to_string(),
        regex,
        name: name.map(String::from),
        converters: converter_list,
        callback,
    })
}

/// Creates a URL pattern using a raw regex (Django's `re_path()` syntax).
///
/// Named groups in the regex (e.g., `(?P<year>[0-9]{4})`) are treated as
/// string parameters. No type-based converters are applied.
///
/// # Errors
///
/// Returns an error if the regex is invalid.
pub fn re_path(
    regex_str: &str,
    callback: RouteHandler,
    name: Option<&str>,
) -> DjangoResult<URLPattern> {
    let full_regex = if regex_str.starts_with('^') {
        regex_str.to_string()
    } else {
        format!("^{regex_str}")
    };

    let full_regex = if full_regex.ends_with('$') {
        full_regex
    } else {
        format!("{full_regex}$")
    };

    let regex = Regex::new(&full_regex)
        .map_err(|e| DjangoError::ImproperlyConfigured(format!("Invalid regex pattern: {e}")))?;

    Ok(URLPattern {
        route: regex_str.to_string(),
        regex,
        name: name.map(String::from),
        converters: Vec::new(),
        callback,
    })
}

/// Creates a URL pattern for use as a prefix in a `URLResolver`.
///
/// Unlike [`path`], this pattern does not anchor to the end of the string,
/// allowing it to match a prefix of the URL path.
///
/// # Errors
///
/// Returns an error if the route contains unknown converter types or invalid syntax.
pub fn path_prefix(route: &str) -> DjangoResult<URLPattern> {
    let mut regex_str = String::from("^");
    let mut converter_list = Vec::new();
    let mut remaining = route;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find('<') {
            let prefix = &remaining[..start];
            regex_str.push_str(&regex::escape(prefix));

            let end = remaining[start..].find('>').ok_or_else(|| {
                DjangoError::ImproperlyConfigured(format!(
                    "Unclosed angle bracket in route: {route}"
                ))
            })? + start;

            let inner = &remaining[start + 1..end];
            let (type_name, param_name) = parse_type_and_name(inner);

            let converter = converters::get_converter(type_name)?;
            let converter_regex = converter.regex();
            write!(regex_str, "(?P<{param_name}>{converter_regex})").ok();
            converter_list.push((param_name.to_string(), converter));

            remaining = &remaining[end + 1..];
        } else {
            regex_str.push_str(&regex::escape(remaining));
            break;
        }
    }

    // No trailing $ for prefix patterns
    let regex = Regex::new(&regex_str).map_err(|e| {
        DjangoError::ImproperlyConfigured(format!("Invalid prefix pattern regex: {e}"))
    })?;

    // Use a dummy handler for prefix patterns
    let dummy_handler: RouteHandler =
        Arc::new(|_req| Box::pin(async { crate::HttpResponse::not_found("Not found") }));

    Ok(URLPattern {
        route: route.to_string(),
        regex,
        name: None,
        converters: converter_list,
        callback: dummy_handler,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_handler() -> RouteHandler {
        Arc::new(|_req| Box::pin(async { crate::HttpResponse::ok("ok") }))
    }

    #[test]
    fn test_path_simple_no_params() {
        let p = path("articles/", dummy_handler(), Some("articles")).unwrap();
        assert_eq!(p.name(), Some("articles"));
        assert!(p.full_match("articles/").is_some());
        assert!(p.full_match("other/").is_none());
    }

    #[test]
    fn test_path_with_int_param() {
        let p = path("articles/<int:year>/", dummy_handler(), None).unwrap();
        let kwargs = p.full_match("articles/2024/").unwrap();
        assert_eq!(kwargs.get("year").unwrap(), "2024");
    }

    #[test]
    fn test_path_with_str_param() {
        let p = path("users/<str:username>/", dummy_handler(), None).unwrap();
        let kwargs = p.full_match("users/alice/").unwrap();
        assert_eq!(kwargs.get("username").unwrap(), "alice");
    }

    #[test]
    fn test_path_with_slug_param() {
        let p = path("posts/<slug:title>/", dummy_handler(), None).unwrap();
        let kwargs = p.full_match("posts/my-first-post/").unwrap();
        assert_eq!(kwargs.get("title").unwrap(), "my-first-post");
    }

    #[test]
    fn test_path_with_uuid_param() {
        let p = path("items/<uuid:id>/", dummy_handler(), None).unwrap();
        let kwargs = p
            .full_match("items/550e8400-e29b-41d4-a716-446655440000/")
            .unwrap();
        assert_eq!(
            kwargs.get("id").unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_path_with_path_param() {
        let p = path("files/<path:filepath>", dummy_handler(), None).unwrap();
        let kwargs = p.full_match("files/docs/readme.md").unwrap();
        assert_eq!(kwargs.get("filepath").unwrap(), "docs/readme.md");
    }

    #[test]
    fn test_path_multiple_params() {
        let p = path(
            "articles/<int:year>/<slug:title>/",
            dummy_handler(),
            Some("article-detail"),
        )
        .unwrap();

        let kwargs = p.full_match("articles/2024/hello-world/").unwrap();
        assert_eq!(kwargs.get("year").unwrap(), "2024");
        assert_eq!(kwargs.get("title").unwrap(), "hello-world");
    }

    #[test]
    fn test_path_no_match() {
        let p = path("articles/<int:year>/", dummy_handler(), None).unwrap();
        assert!(p.full_match("articles/abc/").is_none());
        assert!(p.full_match("posts/2024/").is_none());
    }

    #[test]
    fn test_path_partial_match() {
        let p = path("articles/<int:year>/", dummy_handler(), None).unwrap();
        // full_match should fail if there's trailing content
        assert!(p.full_match("articles/2024/extra").is_none());
    }

    #[test]
    fn test_path_default_str_converter() {
        // When no type is specified, defaults to str
        let p = path("users/<username>/", dummy_handler(), None).unwrap();
        let kwargs = p.full_match("users/alice/").unwrap();
        assert_eq!(kwargs.get("username").unwrap(), "alice");
    }

    #[test]
    fn test_re_path_basic() {
        let p = re_path(
            r"^articles/(?P<year>[0-9]{4})/$",
            dummy_handler(),
            Some("article-year"),
        )
        .unwrap();
        let kwargs = p.full_match("articles/2024/").unwrap();
        assert_eq!(kwargs.get("year").unwrap(), "2024");
        // Only 4-digit years match
        assert!(p.full_match("articles/99/").is_none());
    }

    #[test]
    fn test_re_path_auto_anchors() {
        // Without explicit ^ and $, they should be added
        let p = re_path(r"articles/(?P<id>[0-9]+)/", dummy_handler(), None).unwrap();
        assert!(p.full_match("articles/42/").is_some());
    }

    #[test]
    fn test_path_unknown_converter() {
        let result = path("articles/<custom:year>/", dummy_handler(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_path_unclosed_bracket() {
        let result = path("articles/<int:year/", dummy_handler(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_path_prefix() {
        let p = path_prefix("api/v1/").unwrap();
        let result = p.match_path("api/v1/users/");
        assert!(result.is_some());
        let (kwargs, remaining) = result.unwrap();
        assert!(kwargs.is_empty());
        assert_eq!(remaining, "users/");
    }

    #[test]
    fn test_path_prefix_with_param() {
        let p = path_prefix("api/<str:version>/").unwrap();
        let result = p.match_path("api/v2/users/");
        assert!(result.is_some());
        let (kwargs, remaining) = result.unwrap();
        assert_eq!(kwargs.get("version").unwrap(), "v2");
        assert_eq!(remaining, "users/");
    }

    #[test]
    fn test_match_path_returns_remaining() {
        let p = path("articles/", dummy_handler(), None).unwrap();
        assert!(p.full_match("articles/").is_some());
        let (kwargs, remaining) = p.match_path("articles/").unwrap();
        assert!(kwargs.is_empty());
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_path_route_accessor() {
        let p = path("articles/<int:year>/", dummy_handler(), None).unwrap();
        assert_eq!(p.route(), "articles/<int:year>/");
    }

    #[test]
    fn test_path_debug() {
        let p = path("test/", dummy_handler(), Some("test")).unwrap();
        let debug = format!("{p:?}");
        assert!(debug.contains("test"));
    }
}
