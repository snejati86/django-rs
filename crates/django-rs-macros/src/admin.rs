//! `#[derive(Admin)]` implementation.
//!
//! Generates a `ModelAdmin`-style configuration struct with list display,
//! filtering, search, and ordering options.

use darling::FromDeriveInput;
use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::string_list::StringList;

/// Struct-level admin attributes from `#[admin(...)]`.
#[derive(Debug, FromDeriveInput)]
#[darling(attributes(admin), supports(struct_named, struct_unit))]
pub struct AdminOpts {
    pub ident: syn::Ident,

    /// Fields to display in the list view.
    #[darling(default)]
    pub list_display: Option<StringList>,

    /// Fields available for filtering in the list view.
    #[darling(default)]
    pub list_filter: Option<StringList>,

    /// Fields to search across.
    #[darling(default)]
    pub search_fields: Option<StringList>,

    /// Default ordering (prefix with `-` for descending).
    #[darling(default)]
    pub ordering: Option<StringList>,

    /// Number of items per page.
    #[darling(default)]
    pub list_per_page: Option<usize>,

    /// Read-only fields in edit view.
    #[darling(default)]
    pub readonly_fields: Option<StringList>,

    /// Fields to show links in list view.
    #[darling(default)]
    pub list_display_links: Option<StringList>,

    /// Fields editable directly in list view.
    #[darling(default)]
    pub list_editable: Option<StringList>,

    /// Date hierarchy field.
    #[darling(default)]
    pub date_hierarchy: Option<String>,
}

/// Generates the admin configuration implementation.
pub fn derive_admin_impl(input: DeriveInput) -> TokenStream {
    let opts = match AdminOpts::from_derive_input(&input) {
        Ok(o) => o,
        Err(e) => return e.write_errors(),
    };

    let struct_name = &opts.ident;

    let list_display = generate_str_vec(&opts.list_display);
    let list_filter = generate_str_vec(&opts.list_filter);
    let search_fields = generate_str_vec(&opts.search_fields);
    let readonly_fields = generate_str_vec(&opts.readonly_fields);
    let list_display_links = generate_str_vec(&opts.list_display_links);
    let list_editable = generate_str_vec(&opts.list_editable);
    let list_per_page = opts.list_per_page.unwrap_or(100);

    let ordering_tokens = match &opts.ordering {
        Some(ordering) => {
            let pairs: Vec<TokenStream> = ordering
                .0
                .iter()
                .map(|s| {
                    if let Some(col) = s.strip_prefix('-') {
                        quote! { (#col, true) }
                    } else {
                        quote! { (#s, false) }
                    }
                })
                .collect();
            quote! { vec![#(#pairs),*] }
        }
        None => quote! { vec![] },
    };

    let date_hierarchy = match &opts.date_hierarchy {
        Some(dh) => quote! { Some(#dh.to_string()) },
        None => quote! { None },
    };

    let expanded = quote! {
        impl #struct_name {
            /// Returns the admin configuration for list display fields.
            pub fn list_display() -> Vec<&'static str> {
                #list_display
            }

            /// Returns the admin configuration for list filter fields.
            pub fn list_filter() -> Vec<&'static str> {
                #list_filter
            }

            /// Returns the admin configuration for search fields.
            pub fn search_fields() -> Vec<&'static str> {
                #search_fields
            }

            /// Returns the admin ordering configuration.
            /// Each tuple is (field_name, is_descending).
            pub fn ordering() -> Vec<(&'static str, bool)> {
                #ordering_tokens
            }

            /// Returns the number of items per page.
            pub fn list_per_page() -> usize {
                #list_per_page
            }

            /// Returns the read-only fields.
            pub fn readonly_fields() -> Vec<&'static str> {
                #readonly_fields
            }

            /// Returns the list display links.
            pub fn list_display_links() -> Vec<&'static str> {
                #list_display_links
            }

            /// Returns the list editable fields.
            pub fn list_editable() -> Vec<&'static str> {
                #list_editable
            }

            /// Returns the date hierarchy field.
            pub fn date_hierarchy() -> Option<String> {
                #date_hierarchy
            }
        }
    };

    expanded
}

/// Generates a `Vec<&'static str>` from an optional `StringList`.
fn generate_str_vec(items: &Option<StringList>) -> TokenStream {
    match items {
        Some(items) => {
            let strs: Vec<&str> = items.0.iter().map(String::as_str).collect();
            quote! { vec![#(#strs),*] }
        }
        None => quote! { vec![] },
    }
}
