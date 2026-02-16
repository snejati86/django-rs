//! `#[derive(Form)]` implementation.
//!
//! Generates a function that returns a `Vec<FormFieldDef>` describing the
//! form's fields, plus a convenience constructor that creates a `BaseForm`.

use darling::{FromDeriveInput, FromField};
use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::model::{is_option_type, type_to_string, unwrap_option_type};
use crate::string_list::StringList;

/// Struct-level form attributes from `#[form(...)]`.
#[derive(Debug, FromDeriveInput)]
#[darling(attributes(form), supports(struct_named))]
pub struct FormOpts {
    pub ident: syn::Ident,
    pub data: darling::ast::Data<(), FormFieldOpts>,

    /// HTML form action URL.
    #[darling(default)]
    pub action: Option<String>,

    /// HTTP method (GET or POST).
    #[darling(default)]
    pub method: Option<String>,
}

/// Per-field form attributes from `#[form_field(...)]`.
#[derive(Debug, FromField)]
#[darling(attributes(form_field))]
pub struct FormFieldOpts {
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,

    /// Whether the field is required.
    #[darling(default)]
    pub required: Option<bool>,

    /// Override the field type (e.g. "email", "choice", "integer").
    #[darling(default)]
    pub field_type: Option<String>,

    /// Max length for char fields.
    #[darling(default)]
    pub max_length: Option<usize>,

    /// Min length for char fields.
    #[darling(default)]
    pub min_length: Option<usize>,

    /// Human-readable label.
    #[darling(default)]
    pub label: Option<String>,

    /// Help text.
    #[darling(default)]
    pub help_text: Option<String>,

    /// Widget type override (e.g. "textarea", "select").
    #[darling(default)]
    pub widget: Option<String>,

    /// Choices for choice fields as (value, label) pairs.
    #[darling(default)]
    pub choices: Option<StringList>,

    /// Min value for numeric fields.
    #[darling(default)]
    pub min_value: Option<i64>,

    /// Max value for numeric fields.
    #[darling(default)]
    pub max_value: Option<i64>,
}

/// Generates the Form-related implementations for the struct.
pub fn derive_form_impl(input: DeriveInput) -> TokenStream {
    let opts = match FormOpts::from_derive_input(&input) {
        Ok(o) => o,
        Err(e) => return e.write_errors(),
    };

    let struct_name = &opts.ident;
    let _action = opts.action.as_deref().unwrap_or("/");
    let _method = opts.method.as_deref().unwrap_or("post");

    let fields = opts
        .data
        .as_ref()
        .take_struct()
        .expect("#[derive(Form)] only supports named structs")
        .fields;

    // Generate FormFieldDef entries
    let field_def_tokens: Vec<TokenStream> =
        fields.iter().map(|f| generate_form_field_def(f)).collect();

    let expanded = quote! {
        impl #struct_name {
            /// Returns the form field definitions for this form.
            pub fn form_fields() -> Vec<django_rs_forms::fields::FormFieldDef> {
                vec![
                    #(#field_def_tokens),*
                ]
            }

            /// Creates a new `BaseForm` with this form's field definitions.
            pub fn as_base_form() -> django_rs_forms::form::BaseForm {
                django_rs_forms::form::BaseForm::new(Self::form_fields())
            }
        }
    };

    expanded
}

/// Generates a single `FormFieldDef` construction expression.
fn generate_form_field_def(f: &FormFieldOpts) -> TokenStream {
    let name_str = f.ident.as_ref().unwrap().to_string();
    let field_type_token = infer_form_field_type(f);

    let mut chain = Vec::new();

    // Required: defaults to true unless the type is Option<T> or explicitly set
    let is_optional = is_option_type(&f.ty);
    let required = f.required.unwrap_or(!is_optional);
    if !required {
        chain.push(quote! { .required(false) });
    }

    if let Some(ref label) = f.label {
        chain.push(quote! { .label(#label) });
    }
    if let Some(ref ht) = f.help_text {
        chain.push(quote! { .help_text(#ht) });
    }
    if let Some(ref widget_name) = f.widget {
        let widget_token = match widget_name.as_str() {
            "textarea" => quote! { django_rs_forms::widgets::WidgetType::Textarea },
            "select" => quote! { django_rs_forms::widgets::WidgetType::Select },
            "password" => quote! { django_rs_forms::widgets::WidgetType::PasswordInput },
            "hidden" => quote! { django_rs_forms::widgets::WidgetType::HiddenInput },
            "checkbox" => quote! { django_rs_forms::widgets::WidgetType::CheckboxInput },
            "radio" => quote! { django_rs_forms::widgets::WidgetType::RadioSelect },
            "email" => quote! { django_rs_forms::widgets::WidgetType::EmailInput },
            "number" => quote! { django_rs_forms::widgets::WidgetType::NumberInput },
            "date" => quote! { django_rs_forms::widgets::WidgetType::DateInput },
            "datetime" => quote! { django_rs_forms::widgets::WidgetType::DateTimeInput },
            "time" => quote! { django_rs_forms::widgets::WidgetType::TimeInput },
            "file" => quote! { django_rs_forms::widgets::WidgetType::FileInput },
            "url" => quote! { django_rs_forms::widgets::WidgetType::UrlInput },
            _ => quote! { django_rs_forms::widgets::WidgetType::TextInput },
        };
        chain.push(quote! { .widget(#widget_token) });
    }

    quote! {
        django_rs_forms::fields::FormFieldDef::new(#name_str, #field_type_token)
            #(#chain)*
    }
}

/// Infers the `FormFieldType` from a form field's attributes and Rust type.
fn infer_form_field_type(f: &FormFieldOpts) -> TokenStream {
    // Explicit field_type override
    if let Some(ref ft) = f.field_type {
        match ft.as_str() {
            "email" => return quote! { django_rs_forms::fields::FormFieldType::Email },
            "url" => return quote! { django_rs_forms::fields::FormFieldType::Url },
            "integer" => {
                let min = f
                    .min_value
                    .map(|v| quote! { Some(#v) })
                    .unwrap_or(quote! { None });
                let max = f
                    .max_value
                    .map(|v| quote! { Some(#v) })
                    .unwrap_or(quote! { None });
                return quote! {
                    django_rs_forms::fields::FormFieldType::Integer {
                        min_value: #min,
                        max_value: #max,
                    }
                };
            }
            "float" => {
                return quote! {
                    django_rs_forms::fields::FormFieldType::Float {
                        min_value: None,
                        max_value: None,
                    }
                };
            }
            "boolean" => return quote! { django_rs_forms::fields::FormFieldType::Boolean },
            "date" => return quote! { django_rs_forms::fields::FormFieldType::Date },
            "datetime" => return quote! { django_rs_forms::fields::FormFieldType::DateTime },
            "time" => return quote! { django_rs_forms::fields::FormFieldType::Time },
            "uuid" => return quote! { django_rs_forms::fields::FormFieldType::Uuid },
            "slug" => return quote! { django_rs_forms::fields::FormFieldType::Slug },
            "json" => return quote! { django_rs_forms::fields::FormFieldType::Json },
            "choice" => {
                let choices = generate_choices(f);
                return quote! {
                    django_rs_forms::fields::FormFieldType::Choice {
                        choices: #choices,
                    }
                };
            }
            _ => {}
        }
    }

    // Infer from Rust type
    let inner = unwrap_option_type(&f.ty).unwrap_or(&f.ty);
    let type_str = type_to_string(inner);

    match type_str.as_str() {
        "String" => {
            let min_len = f
                .min_length
                .map(|v| quote! { Some(#v) })
                .unwrap_or(quote! { None });
            let max_len = f
                .max_length
                .map(|v| quote! { Some(#v) })
                .unwrap_or(quote! { None });
            quote! {
                django_rs_forms::fields::FormFieldType::Char {
                    min_length: #min_len,
                    max_length: #max_len,
                    strip: true,
                }
            }
        }
        "i32" | "i64" | "i16" => {
            let min = f
                .min_value
                .map(|v| quote! { Some(#v) })
                .unwrap_or(quote! { None });
            let max = f
                .max_value
                .map(|v| quote! { Some(#v) })
                .unwrap_or(quote! { None });
            quote! {
                django_rs_forms::fields::FormFieldType::Integer {
                    min_value: #min,
                    max_value: #max,
                }
            }
        }
        "f64" | "f32" => {
            quote! {
                django_rs_forms::fields::FormFieldType::Float {
                    min_value: None,
                    max_value: None,
                }
            }
        }
        "bool" => quote! { django_rs_forms::fields::FormFieldType::Boolean },
        _ => {
            if type_str.contains("NaiveDate") && !type_str.contains("Time") {
                quote! { django_rs_forms::fields::FormFieldType::Date }
            } else if type_str.contains("NaiveDateTime") {
                quote! { django_rs_forms::fields::FormFieldType::DateTime }
            } else if type_str.contains("NaiveTime") {
                quote! { django_rs_forms::fields::FormFieldType::Time }
            } else if type_str.contains("Uuid") {
                quote! { django_rs_forms::fields::FormFieldType::Uuid }
            } else {
                let min_len = f
                    .min_length
                    .map(|v| quote! { Some(#v) })
                    .unwrap_or(quote! { None });
                let max_len = f
                    .max_length
                    .map(|v| quote! { Some(#v) })
                    .unwrap_or(quote! { None });
                quote! {
                    django_rs_forms::fields::FormFieldType::Char {
                        min_length: #min_len,
                        max_length: #max_len,
                        strip: true,
                    }
                }
            }
        }
    }
}

/// Generates a `Vec<(String, String)>` for choices.
///
/// The `choices` attribute is expected as a list of strings like
/// `["value1:Label 1", "value2:Label 2"]` where the colon separates
/// value from display label.
fn generate_choices(f: &FormFieldOpts) -> TokenStream {
    match &f.choices {
        Some(choices) => {
            let pairs: Vec<TokenStream> = choices
                .0
                .iter()
                .map(|c| {
                    let parts: Vec<&str> = c.splitn(2, ':').collect();
                    let (val, label) = if parts.len() == 2 {
                        (parts[0], parts[1])
                    } else {
                        (parts[0], parts[0])
                    };
                    quote! { (#val.to_string(), #label.to_string()) }
                })
                .collect();
            quote! { vec![#(#pairs),*] }
        }
        None => quote! { vec![] },
    }
}
