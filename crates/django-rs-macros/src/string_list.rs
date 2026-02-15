//! Helper type for parsing `Vec<String>` from darling attributes.
//!
//! Darling's `FromMeta` is implemented for `Vec<LitStr>` but not `Vec<String>`.
//! This module provides a newtype wrapper that accepts both:
//! - `#[attr(field("a", "b"))]` (parenthesized list / from_list)
//! - `#[attr(field = ["a", "b"])]` (array expression / from_expr)

use darling::FromMeta;

/// A newtype around `Vec<String>` that implements `FromMeta` via multiple
/// parsing strategies to support both `field("a", "b")` and `field = ["a", "b"]`.
#[derive(Debug, Clone, Default)]
pub struct StringList(pub Vec<String>);

impl FromMeta for StringList {
    /// Handles parenthesized list syntax: `field("a", "b", "c")`
    fn from_list(items: &[darling::ast::NestedMeta]) -> darling::Result<Self> {
        let strings: darling::Result<Vec<String>> = items
            .iter()
            .map(|item| match item {
                darling::ast::NestedMeta::Lit(syn::Lit::Str(lit)) => Ok(lit.value()),
                _ => Err(darling::Error::unexpected_type("non-string literal")),
            })
            .collect();
        strings.map(StringList)
    }

    /// Handles array expression syntax: `field = ["a", "b", "c"]`
    fn from_expr(expr: &syn::Expr) -> darling::Result<Self> {
        match expr {
            syn::Expr::Array(arr) => {
                let strings: darling::Result<Vec<String>> = arr
                    .elems
                    .iter()
                    .map(|elem| {
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(lit),
                            ..
                        }) = elem
                        {
                            Ok(lit.value())
                        } else {
                            Err(darling::Error::unexpected_type("non-string literal in array"))
                        }
                    })
                    .collect();
                strings.map(StringList)
            }
            _ => Err(darling::Error::unexpected_expr_type(expr)),
        }
    }
}
