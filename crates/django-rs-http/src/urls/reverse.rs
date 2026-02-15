//! Reverse URL resolution.
//!
//! This module provides the [`reverse`] function for generating URLs from
//! named URL patterns, mirroring Django's `django.urls.reverse()`.

use std::collections::HashMap;
use std::hash::BuildHasher;

use django_rs_core::{DjangoError, DjangoResult};

use super::resolver::URLResolver;

/// Generates a URL for a named view, substituting the given arguments.
///
/// Supports both positional args and keyword args. Namespaced lookups use
/// colon-separated names (e.g., `"app:view-name"`).
///
/// This mirrors Django's `reverse()` function.
///
/// # Arguments
///
/// * `viewname` - The name of the URL pattern to resolve. May include namespace
///   prefixes separated by colons (e.g., `"users:detail"`).
/// * `args` - Positional arguments to substitute into the URL pattern (in order).
/// * `kwargs` - Named keyword arguments to substitute into the URL pattern.
/// * `urlconf` - The root URL resolver to search in.
///
/// # Errors
///
/// Returns [`DjangoError::NotFound`] if no matching URL pattern is found, or if
/// the provided arguments do not match the pattern's expected parameters.
///
/// # Examples
///
/// ```
/// use django_rs_http::urls::reverse::reverse;
/// use django_rs_http::urls::resolver::{root, include, URLEntry, URLResolver};
/// use django_rs_http::urls::pattern::path;
/// use django_rs_http::{HttpRequest, HttpResponse};
/// use std::collections::HashMap;
/// use std::sync::Arc;
///
/// let handler = Arc::new(|_req: HttpRequest| -> django_rs_http::BoxFuture {
///     Box::pin(async { HttpResponse::ok("ok") })
/// });
///
/// let patterns = vec![
///     URLEntry::Pattern(path("articles/<int:year>/", handler, Some("article-year")).unwrap()),
/// ];
/// let resolver = root(patterns).unwrap();
///
/// let mut kwargs = HashMap::new();
/// kwargs.insert("year", "2024");
/// let url = reverse("article-year", &[], &kwargs, &resolver).unwrap();
/// assert_eq!(url, "/articles/2024/");
/// ```
pub fn reverse<S: BuildHasher>(
    viewname: &str,
    args: &[&str],
    kwargs: &HashMap<&str, &str, S>,
    urlconf: &URLResolver,
) -> DjangoResult<String> {
    let named_patterns = urlconf.collect_named_patterns();

    for (qualified_name, route_template, _converters) in &named_patterns {
        if qualified_name == viewname {
            let url = substitute_pattern(route_template, args, kwargs)?;
            // Ensure the URL starts with /
            let url = if url.starts_with('/') {
                url
            } else {
                format!("/{url}")
            };
            return Ok(url);
        }
    }

    Err(DjangoError::NotFound(format!(
        "Reverse for '{viewname}' not found"
    )))
}

/// Substitutes arguments into a route template string.
///
/// Replaces `<type:name>` placeholders with values from kwargs (by name)
/// or args (by position).
fn substitute_pattern<S: BuildHasher>(
    route: &str,
    args: &[&str],
    kwargs: &HashMap<&str, &str, S>,
) -> DjangoResult<String> {
    let mut result = String::new();
    let mut remaining = route;
    let mut arg_index = 0;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find('<') {
            // Add the literal prefix
            result.push_str(&remaining[..start]);

            let end = remaining[start..]
                .find('>')
                .ok_or_else(|| {
                    DjangoError::ImproperlyConfigured(format!(
                        "Unclosed angle bracket in route template: {route}"
                    ))
                })?
                + start;

            let inner = &remaining[start + 1..end];

            // Parse "type:name" or just "name"
            let param_name = inner
                .find(':')
                .map_or(inner, |pos| &inner[pos + 1..]);

            // Try kwargs first, then fall back to positional args
            if let Some(value) = kwargs.get(param_name) {
                result.push_str(value);
            } else if arg_index < args.len() {
                result.push_str(args[arg_index]);
                arg_index += 1;
            } else {
                return Err(DjangoError::NotFound(format!(
                    "No value provided for parameter '{param_name}' in URL pattern"
                )));
            }

            remaining = &remaining[end + 1..];
        } else {
            result.push_str(remaining);
            break;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::urls::pattern::{path, RouteHandler};
    use crate::urls::resolver::{include, root, URLEntry};

    fn dummy_handler() -> RouteHandler {
        Arc::new(|_req| Box::pin(async { crate::HttpResponse::ok("ok") }))
    }

    #[test]
    fn test_reverse_simple() {
        let patterns = vec![URLEntry::Pattern(
            path("articles/", dummy_handler(), Some("articles")).unwrap(),
        )];
        let resolver = root(patterns).unwrap();

        let url = reverse("articles", &[], &HashMap::new(), &resolver).unwrap();
        assert_eq!(url, "/articles/");
    }

    #[test]
    fn test_reverse_with_kwargs() {
        let patterns = vec![URLEntry::Pattern(
            path("articles/<int:year>/", dummy_handler(), Some("article-year")).unwrap(),
        )];
        let resolver = root(patterns).unwrap();

        let mut kwargs = HashMap::new();
        kwargs.insert("year", "2024");
        let url = reverse("article-year", &[], &kwargs, &resolver).unwrap();
        assert_eq!(url, "/articles/2024/");
    }

    #[test]
    fn test_reverse_with_args() {
        let patterns = vec![URLEntry::Pattern(
            path(
                "articles/<int:year>/<slug:title>/",
                dummy_handler(),
                Some("article-detail"),
            )
            .unwrap(),
        )];
        let resolver = root(patterns).unwrap();

        let url = reverse(
            "article-detail",
            &["2024", "hello-world"],
            &HashMap::new(),
            &resolver,
        )
        .unwrap();
        assert_eq!(url, "/articles/2024/hello-world/");
    }

    #[test]
    fn test_reverse_with_mixed_args_kwargs() {
        let patterns = vec![URLEntry::Pattern(
            path(
                "articles/<int:year>/<slug:title>/",
                dummy_handler(),
                Some("article-detail"),
            )
            .unwrap(),
        )];
        let resolver = root(patterns).unwrap();

        let mut kwargs = HashMap::new();
        kwargs.insert("title", "hello-world");
        // year will be taken from positional args
        let url = reverse("article-detail", &["2024"], &kwargs, &resolver).unwrap();
        assert_eq!(url, "/articles/2024/hello-world/");
    }

    #[test]
    fn test_reverse_namespaced() {
        let user_patterns = vec![URLEntry::Pattern(
            path("<int:id>/", dummy_handler(), Some("detail")).unwrap(),
        )];

        let patterns = vec![URLEntry::Resolver(
            include("users/", user_patterns, Some("users"), Some("users")).unwrap(),
        )];

        let resolver = root(patterns).unwrap();

        let mut kwargs = HashMap::new();
        kwargs.insert("id", "42");
        let url = reverse("users:detail", &[], &kwargs, &resolver).unwrap();
        assert_eq!(url, "/users/42/");
    }

    #[test]
    fn test_reverse_not_found() {
        let patterns = vec![URLEntry::Pattern(
            path("articles/", dummy_handler(), Some("articles")).unwrap(),
        )];
        let resolver = root(patterns).unwrap();

        let result = reverse("nonexistent", &[], &HashMap::new(), &resolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_reverse_missing_param() {
        let patterns = vec![URLEntry::Pattern(
            path("articles/<int:year>/", dummy_handler(), Some("article-year")).unwrap(),
        )];
        let resolver = root(patterns).unwrap();

        let result = reverse("article-year", &[], &HashMap::new(), &resolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_reverse_deeply_namespaced() {
        let info_patterns = vec![URLEntry::Pattern(
            path("info/", dummy_handler(), Some("info")).unwrap(),
        )];

        let detail_patterns = vec![URLEntry::Resolver(
            include("<int:id>/", info_patterns, Some("detail"), Some("detail")).unwrap(),
        )];

        let patterns = vec![URLEntry::Resolver(
            include("users/", detail_patterns, Some("users"), Some("users")).unwrap(),
        )];

        let resolver = root(patterns).unwrap();

        let mut kwargs = HashMap::new();
        kwargs.insert("id", "42");
        let url = reverse("users:detail:info", &[], &kwargs, &resolver).unwrap();
        assert_eq!(url, "/users/42/info/");
    }

    #[test]
    fn test_substitute_pattern_basic() {
        let mut kwargs = HashMap::new();
        kwargs.insert("year", "2024");
        let result = substitute_pattern("articles/<int:year>/", &[], &kwargs).unwrap();
        assert_eq!(result, "articles/2024/");
    }

    #[test]
    fn test_substitute_pattern_no_params() {
        let result =
            substitute_pattern("articles/", &[], &HashMap::<&str, &str>::new()).unwrap();
        assert_eq!(result, "articles/");
    }

    #[test]
    fn test_substitute_pattern_multiple_params() {
        let mut kwargs = HashMap::new();
        kwargs.insert("year", "2024");
        kwargs.insert("title", "hello");
        let result = substitute_pattern(
            "articles/<int:year>/<slug:title>/",
            &[],
            &kwargs,
        )
        .unwrap();
        assert_eq!(result, "articles/2024/hello/");
    }
}
