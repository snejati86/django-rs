//! URL resolver and namespace support.
//!
//! This module provides [`URLResolver`] for hierarchical URL resolution, including
//! namespace support. It mirrors Django's `django.urls.URLResolver` and the
//! `include()` function.

use std::collections::HashMap;
use std::fmt;

use django_rs_core::{DjangoError, DjangoResult};

use super::converters::PathConverter;
use super::pattern::{self, ConverterEntry, RouteHandler, URLPattern};

/// An entry in the named-pattern collection: `(qualified_name, route_template, converters)`.
pub type NamedPatternEntry = (String, String, Vec<ConverterEntry>);

/// The result of successfully resolving a URL path to a handler.
///
/// Contains the matched handler function, captured arguments, and metadata
/// about the matched route. This mirrors Django's `ResolverMatch`.
#[derive(Clone)]
pub struct ResolverMatch {
    /// The handler function to call.
    pub func: RouteHandler,
    /// Positional arguments extracted from the URL (for `re_path` unnamed groups).
    pub args: Vec<String>,
    /// Named keyword arguments extracted from the URL path.
    pub kwargs: HashMap<String, String>,
    /// The name of the matched URL pattern, if any.
    pub url_name: Option<String>,
    /// The application names in the resolution chain (outermost first).
    pub app_names: Vec<String>,
    /// The instance namespaces in the resolution chain (outermost first).
    pub namespaces: Vec<String>,
    /// The matched route string.
    pub route: String,
}

impl fmt::Debug for ResolverMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolverMatch")
            .field("args", &self.args)
            .field("kwargs", &self.kwargs)
            .field("url_name", &self.url_name)
            .field("app_names", &self.app_names)
            .field("namespaces", &self.namespaces)
            .field("route", &self.route)
            .finish_non_exhaustive()
    }
}

impl ResolverMatch {
    /// Returns the fully-qualified view name, including namespaces.
    ///
    /// For example, if the namespaces are `["api", "v1"]` and the URL name is
    /// `"user-detail"`, this returns `"api:v1:user-detail"`.
    pub fn view_name(&self) -> String {
        let mut parts: Vec<&str> = self.namespaces.iter().map(String::as_str).collect();
        if let Some(name) = &self.url_name {
            parts.push(name);
        }
        parts.join(":")
    }
}

/// An entry in a URL configuration, either a leaf pattern or a nested resolver.
///
/// This mirrors the two types of entries that can appear in Django's `urlpatterns` list.
pub enum URLEntry {
    /// A leaf URL pattern that directly maps to a handler.
    Pattern(URLPattern),
    /// A nested resolver, typically created via `include()`.
    Resolver(URLResolver),
}

impl fmt::Debug for URLEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pattern(p) => f.debug_tuple("Pattern").field(p).finish(),
            Self::Resolver(r) => f.debug_tuple("Resolver").field(r).finish(),
        }
    }
}

/// A URL resolver that matches a prefix and delegates to child patterns.
///
/// Resolvers form a tree structure where each level matches a portion of the
/// URL path and passes the remainder to its children. This is the mechanism
/// behind Django's `include()`.
pub struct URLResolver {
    /// The prefix pattern for this resolver
    pattern: URLPattern,
    /// Child URL patterns and sub-resolvers
    url_patterns: Vec<URLEntry>,
    /// The instance namespace for this resolver
    namespace: Option<String>,
    /// The application namespace for this resolver
    app_name: Option<String>,
}

impl fmt::Debug for URLResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("URLResolver")
            .field("pattern", &self.pattern)
            .field("url_patterns", &self.url_patterns)
            .field("namespace", &self.namespace)
            .field("app_name", &self.app_name)
            .finish()
    }
}

impl URLResolver {
    /// Creates a new resolver with the given prefix pattern and child entries.
    pub fn new(
        pattern: URLPattern,
        url_patterns: Vec<URLEntry>,
        namespace: Option<&str>,
        app_name: Option<&str>,
    ) -> Self {
        Self {
            pattern,
            url_patterns,
            namespace: namespace.map(String::from),
            app_name: app_name.map(String::from),
        }
    }

    /// Returns the prefix pattern.
    pub const fn pattern(&self) -> &URLPattern {
        &self.pattern
    }

    /// Returns the child URL entries.
    pub fn url_patterns(&self) -> &[URLEntry] {
        &self.url_patterns
    }

    /// Returns the instance namespace, if set.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// Returns the application namespace, if set.
    pub fn app_name(&self) -> Option<&str> {
        self.app_name.as_deref()
    }

    /// Resolves a URL path to a [`ResolverMatch`].
    ///
    /// Tries each child pattern/resolver in order. For nested resolvers,
    /// the matched prefix is stripped and the remainder is passed to the child.
    ///
    /// # Errors
    ///
    /// Returns [`DjangoError::NotFound`] if no pattern matches the path.
    pub fn resolve(&self, path: &str) -> DjangoResult<ResolverMatch> {
        // First, match the prefix
        let (prefix_kwargs, remaining) =
            self.pattern.match_path(path).ok_or_else(|| {
                DjangoError::NotFound(format!("No URL pattern matches '{path}'"))
            })?;

        // Try each child pattern
        for entry in &self.url_patterns {
            match entry {
                URLEntry::Pattern(child_pattern) => {
                    if let Some(mut kwargs) = child_pattern.full_match(&remaining) {
                        // Merge prefix kwargs into child kwargs
                        for (k, v) in &prefix_kwargs {
                            kwargs.entry(k.clone()).or_insert_with(|| v.clone());
                        }

                        let mut namespaces = Vec::new();
                        let mut app_names = Vec::new();
                        if let Some(ns) = &self.namespace {
                            namespaces.push(ns.clone());
                        }
                        if let Some(app) = &self.app_name {
                            app_names.push(app.clone());
                        }

                        let route = format!(
                            "{}{}",
                            self.pattern.route(),
                            child_pattern.route()
                        );

                        return Ok(ResolverMatch {
                            func: child_pattern.callback().clone(),
                            args: Vec::new(),
                            kwargs,
                            url_name: child_pattern.name().map(String::from),
                            app_names,
                            namespaces,
                            route,
                        });
                    }
                }
                URLEntry::Resolver(child_resolver) => {
                    if let Ok(mut resolver_match) = child_resolver.resolve(&remaining)
                    {
                        // Merge prefix kwargs
                        for (k, v) in &prefix_kwargs {
                            resolver_match
                                .kwargs
                                .entry(k.clone())
                                .or_insert_with(|| v.clone());
                        }

                        // Prepend our namespace and app_name
                        if let Some(ns) = &self.namespace {
                            resolver_match.namespaces.insert(0, ns.clone());
                        }
                        if let Some(app) = &self.app_name {
                            resolver_match.app_names.insert(0, app.clone());
                        }

                        // Prepend our route
                        resolver_match.route = format!(
                            "{}{}",
                            self.pattern.route(),
                            resolver_match.route
                        );

                        return Ok(resolver_match);
                    }
                }
            }
        }

        Err(DjangoError::NotFound(format!(
            "No URL pattern matches '{path}'"
        )))
    }

    /// Collects all named patterns in this resolver tree, with their fully-qualified names.
    ///
    /// Used internally by [`reverse`](super::reverse::reverse) to find patterns by name.
    pub fn collect_named_patterns(&self) -> Vec<NamedPatternEntry> {
        let mut result = Vec::new();
        self.collect_named_patterns_inner(&mut result, &[], &[]);
        result
    }

    fn collect_named_patterns_inner(
        &self,
        result: &mut Vec<NamedPatternEntry>,
        parent_namespaces: &[String],
        parent_routes: &[String],
    ) {
        let mut namespaces: Vec<String> = parent_namespaces.to_vec();
        if let Some(ns) = &self.namespace {
            namespaces.push(ns.clone());
        }

        let mut routes: Vec<String> = parent_routes.to_vec();
        routes.push(self.pattern.route().to_string());

        for entry in &self.url_patterns {
            match entry {
                URLEntry::Pattern(child_pattern) => {
                    if let Some(name) = child_pattern.name() {
                        let qualified_name = if namespaces.is_empty() {
                            name.to_string()
                        } else {
                            format!("{}:{name}", namespaces.join(":"))
                        };

                        let full_route = {
                            let mut r = routes.join("");
                            r.push_str(child_pattern.route());
                            r
                        };

                        // Collect converters from the child pattern
                        let all_converters = rebuild_converters(child_pattern.converters());

                        result.push((qualified_name, full_route, all_converters));
                    }
                }
                URLEntry::Resolver(child_resolver) => {
                    child_resolver
                        .collect_named_patterns_inner(result, &namespaces, &routes);
                }
            }
        }
    }
}

/// Rebuilds converter entries by cloning the converter type based on its regex pattern.
fn rebuild_converters(converters: &[ConverterEntry]) -> Vec<ConverterEntry> {
    converters
        .iter()
        .map(|(cname, conv)| {
            let converter: Box<dyn PathConverter> = match conv.regex() {
                "[0-9]+" => Box::new(super::converters::IntConverter),
                "[^/]+" => Box::new(super::converters::StrConverter),
                "[-a-zA-Z0-9_]+" => Box::new(super::converters::SlugConverter),
                ".+" => Box::new(super::converters::PathSegmentConverter),
                r if r.contains("0-9a-f") => Box::new(super::converters::UuidConverter),
                _ => Box::new(super::converters::StrConverter),
            };
            (cname.clone(), converter)
        })
        .collect()
}

/// Creates a `URLResolver` from a prefix path and a set of child patterns.
///
/// This mirrors Django's `include()` function.
///
/// # Examples
///
/// ```
/// use django_rs_http::urls::resolver::include;
/// use django_rs_http::urls::pattern::path;
/// use django_rs_http::urls::resolver::URLEntry;
/// use django_rs_http::{HttpRequest, HttpResponse};
/// use std::sync::Arc;
///
/// let handler = Arc::new(|_req: HttpRequest| -> django_rs_http::BoxFuture {
///     Box::pin(async { HttpResponse::ok("list") })
/// });
/// let patterns = vec![
///     URLEntry::Pattern(path("", handler, Some("user-list")).unwrap()),
/// ];
/// let resolver = include("users/", patterns, Some("users"), Some("users")).unwrap();
/// ```
///
/// # Errors
///
/// Returns an error if the prefix route is invalid.
pub fn include(
    prefix: &str,
    patterns: Vec<URLEntry>,
    namespace: Option<&str>,
    app_name: Option<&str>,
) -> DjangoResult<URLResolver> {
    let prefix_pattern = pattern::path_prefix(prefix)?;
    Ok(URLResolver::new(prefix_pattern, patterns, namespace, app_name))
}

/// Creates a root resolver (matches empty prefix) with the given URL entries.
///
/// This is typically used at the top level of the URL configuration.
///
/// # Errors
///
/// Returns an error if pattern creation fails.
pub fn root(patterns: Vec<URLEntry>) -> DjangoResult<URLResolver> {
    let prefix_pattern = pattern::path_prefix("")?;
    Ok(URLResolver::new(prefix_pattern, patterns, None, None))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::urls::pattern::path;

    fn dummy_handler() -> RouteHandler {
        Arc::new(|_req| Box::pin(async { crate::HttpResponse::ok("ok") }))
    }

    #[test]
    fn test_resolve_simple_pattern() {
        let patterns = vec![URLEntry::Pattern(
            path("articles/", dummy_handler(), Some("articles")).unwrap(),
        )];

        let resolver = root(patterns).unwrap();
        let m = resolver.resolve("articles/").unwrap();
        assert_eq!(m.url_name.as_deref(), Some("articles"));
        assert!(m.kwargs.is_empty());
    }

    #[test]
    fn test_resolve_pattern_with_params() {
        let patterns = vec![URLEntry::Pattern(
            path("articles/<int:year>/", dummy_handler(), Some("article-year")).unwrap(),
        )];

        let resolver = root(patterns).unwrap();
        let m = resolver.resolve("articles/2024/").unwrap();
        assert_eq!(m.kwargs.get("year").unwrap(), "2024");
        assert_eq!(m.url_name.as_deref(), Some("article-year"));
    }

    #[test]
    fn test_resolve_nested_include() {
        let user_patterns = vec![
            URLEntry::Pattern(
                path("", dummy_handler(), Some("user-list")).unwrap(),
            ),
            URLEntry::Pattern(
                path("<int:id>/", dummy_handler(), Some("user-detail")).unwrap(),
            ),
        ];

        let patterns = vec![URLEntry::Resolver(
            include("users/", user_patterns, Some("users"), Some("users")).unwrap(),
        )];

        let resolver = root(patterns).unwrap();

        let m = resolver.resolve("users/").unwrap();
        assert_eq!(m.url_name.as_deref(), Some("user-list"));
        assert_eq!(m.namespaces, vec!["users"]);

        let m = resolver.resolve("users/42/").unwrap();
        assert_eq!(m.url_name.as_deref(), Some("user-detail"));
        assert_eq!(m.kwargs.get("id").unwrap(), "42");
        assert_eq!(m.namespaces, vec!["users"]);
    }

    #[test]
    fn test_resolve_deeply_nested() {
        let detail_patterns = vec![URLEntry::Pattern(
            path("info/", dummy_handler(), Some("info")).unwrap(),
        )];

        let user_patterns = vec![URLEntry::Resolver(
            include("<int:id>/", detail_patterns, Some("detail"), Some("detail")).unwrap(),
        )];

        let patterns = vec![URLEntry::Resolver(
            include("users/", user_patterns, Some("users"), Some("users")).unwrap(),
        )];

        let resolver = root(patterns).unwrap();
        let m = resolver.resolve("users/42/info/").unwrap();
        assert_eq!(m.url_name.as_deref(), Some("info"));
        assert_eq!(m.kwargs.get("id").unwrap(), "42");
        assert_eq!(m.namespaces, vec!["users", "detail"]);
        assert_eq!(m.app_names, vec!["users", "detail"]);
    }

    #[test]
    fn test_resolve_not_found() {
        let patterns = vec![URLEntry::Pattern(
            path("articles/", dummy_handler(), Some("articles")).unwrap(),
        )];

        let resolver = root(patterns).unwrap();
        assert!(resolver.resolve("nonexistent/").is_err());
    }

    #[test]
    fn test_resolve_multiple_patterns_first_match_wins() {
        let patterns = vec![
            URLEntry::Pattern(
                path("articles/", dummy_handler(), Some("first")).unwrap(),
            ),
            URLEntry::Pattern(
                path("articles/", dummy_handler(), Some("second")).unwrap(),
            ),
        ];

        let resolver = root(patterns).unwrap();
        let m = resolver.resolve("articles/").unwrap();
        assert_eq!(m.url_name.as_deref(), Some("first"));
    }

    #[test]
    fn test_resolver_match_view_name() {
        let user_patterns = vec![URLEntry::Pattern(
            path("<int:id>/", dummy_handler(), Some("detail")).unwrap(),
        )];

        let patterns = vec![URLEntry::Resolver(
            include("users/", user_patterns, Some("users"), Some("users")).unwrap(),
        )];

        let resolver = root(patterns).unwrap();
        let m = resolver.resolve("users/42/").unwrap();
        assert_eq!(m.view_name(), "users:detail");
    }

    #[test]
    fn test_resolver_match_view_name_no_namespace() {
        let patterns = vec![URLEntry::Pattern(
            path("articles/", dummy_handler(), Some("articles")).unwrap(),
        )];

        let resolver = root(patterns).unwrap();
        let m = resolver.resolve("articles/").unwrap();
        assert_eq!(m.view_name(), "articles");
    }

    #[test]
    fn test_resolver_match_debug() {
        let patterns = vec![URLEntry::Pattern(
            path("test/", dummy_handler(), Some("test")).unwrap(),
        )];
        let resolver = root(patterns).unwrap();
        let m = resolver.resolve("test/").unwrap();
        let debug = format!("{m:?}");
        assert!(debug.contains("test"));
    }

    #[test]
    fn test_resolver_accessors() {
        let resolver = include("api/", vec![], Some("api"), Some("api")).unwrap();
        assert_eq!(resolver.namespace(), Some("api"));
        assert_eq!(resolver.app_name(), Some("api"));
        assert!(resolver.url_patterns().is_empty());
    }

    #[test]
    fn test_resolve_route_tracking() {
        let patterns = vec![URLEntry::Pattern(
            path("articles/<int:year>/", dummy_handler(), Some("articles")).unwrap(),
        )];

        let resolver = root(patterns).unwrap();
        let m = resolver.resolve("articles/2024/").unwrap();
        assert_eq!(m.route, "articles/<int:year>/");
    }

    #[test]
    fn test_include_with_prefix_params() {
        let patterns = vec![URLEntry::Pattern(
            path("posts/", dummy_handler(), Some("posts")).unwrap(),
        )];

        let resolver_entry = include(
            "api/<str:version>/",
            patterns,
            Some("api"),
            Some("api"),
        )
        .unwrap();

        let root_resolver = root(vec![URLEntry::Resolver(resolver_entry)]).unwrap();
        let m = root_resolver.resolve("api/v2/posts/").unwrap();
        assert_eq!(m.kwargs.get("version").unwrap(), "v2");
        assert_eq!(m.url_name.as_deref(), Some("posts"));
    }
}
