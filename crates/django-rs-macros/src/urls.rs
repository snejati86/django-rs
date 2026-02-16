//! `urls!` macro implementation.
//!
//! Parses a Django-like URL pattern DSL and generates code that creates
//! a vector of `UrlPattern` structs.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, LitStr, Token};

/// Represents one URL pattern entry: `"pattern" => handler`.
struct UrlEntry {
    pattern: LitStr,
    handler: Expr,
}

impl Parse for UrlEntry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let pattern: LitStr = input.parse()?;
        input.parse::<Token![=>]>()?;
        let handler: Expr = input.parse()?;
        Ok(UrlEntry { pattern, handler })
    }
}

/// Represents the entire `urls! { ... }` body.
pub struct UrlEntries {
    entries: Vec<UrlEntry>,
}

impl Parse for UrlEntries {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            entries.push(input.parse::<UrlEntry>()?);
            // Consume optional trailing comma
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(UrlEntries { entries })
    }
}

/// Converts a Django-style URL pattern into a regex pattern.
///
/// Converts patterns like:
/// - `"articles/<int:year>/"` -> `"^articles/(?P<year>[0-9]+)/$"`
/// - `"articles/<slug:slug>/"` -> `"^articles/(?P<slug>[-a-zA-Z0-9_]+)/$"`
/// - `"articles/<str:title>/"` -> `"^articles/(?P<title>[^/]+)/$"`
/// - `"articles/<uuid:id>/"` -> `"^articles/(?P<id>[0-9a-f-]+)/$"`
/// - `"articles/<path:rest>/"` -> `"^articles/(?P<rest>.+)/$"`
fn convert_pattern(django_pattern: &str) -> String {
    let mut regex = String::from("^");
    let mut remaining = django_pattern;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find('<') {
            // Add literal text before the capture group
            regex.push_str(&regex::escape(&remaining[..start]));
            let after_start = &remaining[start + 1..];

            if let Some(end) = after_start.find('>') {
                let capture = &after_start[..end];
                let (capture_type, capture_name) = if let Some(colon_pos) = capture.find(':') {
                    (&capture[..colon_pos], &capture[colon_pos + 1..])
                } else {
                    ("str", capture)
                };

                let pattern = match capture_type {
                    "int" => "[0-9]+",
                    "str" => "[^/]+",
                    "slug" => "[-a-zA-Z0-9_]+",
                    "uuid" => "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
                    "path" => ".+",
                    _ => "[^/]+",
                };

                regex.push_str(&format!("(?P<{capture_name}>{pattern})"));
                remaining = &after_start[end + 1..];
            } else {
                // Malformed, just add the rest literally
                regex.push_str(&regex::escape(remaining));
                break;
            }
        } else {
            regex.push_str(&regex::escape(remaining));
            break;
        }
    }

    regex.push('$');
    regex
}

/// Escapes regex special characters in a string.
mod regex {
    pub fn escape(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '\\' | '^' | '$'
                | '|' => {
                    result.push('\\');
                    result.push(c);
                }
                _ => result.push(c),
            }
        }
        result
    }
}

/// Generates the `urls!` macro output.
pub fn expand_urls(entries: UrlEntries) -> TokenStream {
    let patterns: Vec<TokenStream> = entries
        .entries
        .iter()
        .map(|entry| {
            let raw_pattern = entry.pattern.value();
            let regex_pattern = convert_pattern(&raw_pattern);
            let handler = &entry.handler;

            quote! {
                django_rs_macros::UrlPattern {
                    pattern: #raw_pattern,
                    regex: #regex_pattern,
                    handler: #handler,
                }
            }
        })
        .collect();

    quote! {
        {
            vec![
                #(#patterns),*
            ]
        }
    }
}

/// A URL pattern struct emitted by the `urls!` macro.
///
/// This is a simple data carrier that downstream code (e.g. a router) can
/// use to build its routing table.
///
/// Note: this struct is defined in the macro crate's output namespace via
/// the `django_rs_macros::UrlPattern` path, but the actual struct must
/// exist in a crate the user depends on. For now, we generate a local
/// struct definition as part of the macro expansion.
///
/// In practice, the framework crate re-exports this.
#[allow(dead_code)]
pub fn generate_url_pattern_struct() -> TokenStream {
    quote! {
        /// A URL pattern mapping a regex to a handler function.
        #[derive(Debug, Clone)]
        pub struct UrlPattern<F> {
            /// The original Django-style pattern string.
            pub pattern: &'static str,
            /// The compiled regex string.
            pub regex: &'static str,
            /// The handler function.
            pub handler: F,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_empty_pattern() {
        assert_eq!(convert_pattern(""), "^$");
    }

    #[test]
    fn test_convert_literal_pattern() {
        assert_eq!(convert_pattern("articles/"), "^articles/$");
    }

    #[test]
    fn test_convert_int_capture() {
        assert_eq!(
            convert_pattern("articles/<int:year>/"),
            "^articles/(?P<year>[0-9]+)/$"
        );
    }

    #[test]
    fn test_convert_slug_capture() {
        assert_eq!(
            convert_pattern("articles/<slug:slug>/"),
            "^articles/(?P<slug>[-a-zA-Z0-9_]+)/$"
        );
    }

    #[test]
    fn test_convert_str_capture() {
        assert_eq!(
            convert_pattern("articles/<str:title>/"),
            "^articles/(?P<title>[^/]+)/$"
        );
    }

    #[test]
    fn test_convert_uuid_capture() {
        let result = convert_pattern("items/<uuid:id>/");
        assert!(result.contains("(?P<id>"));
    }

    #[test]
    fn test_convert_path_capture() {
        assert_eq!(convert_pattern("files/<path:rest>"), "^files/(?P<rest>.+)$");
    }

    #[test]
    fn test_convert_multiple_captures() {
        let result = convert_pattern("articles/<int:year>/<slug:slug>/");
        assert!(result.contains("(?P<year>[0-9]+)"));
        assert!(result.contains("(?P<slug>[-a-zA-Z0-9_]+)"));
    }

    #[test]
    fn test_convert_no_type_defaults_to_str() {
        assert_eq!(convert_pattern("user/<name>/"), "^user/(?P<name>[^/]+)/$");
    }

    #[test]
    fn test_regex_escape() {
        assert_eq!(regex::escape("hello.world"), "hello\\.world");
        assert_eq!(regex::escape("a+b*c"), "a\\+b\\*c");
        assert_eq!(regex::escape("simple"), "simple");
    }
}
