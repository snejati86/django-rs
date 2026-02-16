//! `#[derive(Model)]` implementation.
//!
//! This module generates an implementation of the `django_rs_db::model::Model`
//! trait for a struct, including `ModelMeta`, field definitions, value
//! conversions, and row deserialization.

use darling::{FromDeriveInput, FromField};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Type};

use crate::string_list::StringList;

/// Top-level struct-level attributes parsed from `#[model(...)]`.
#[derive(Debug, FromDeriveInput)]
#[darling(attributes(model), supports(struct_named))]
pub struct ModelOpts {
    pub ident: syn::Ident,
    pub data: darling::ast::Data<(), FieldOpts>,

    /// The database table name; defaults to `"{app}_{model_name_lowercase}"`.
    #[darling(default)]
    pub table: Option<String>,

    /// The application label.
    #[darling(default)]
    pub app: Option<String>,

    /// Verbose singular name.
    #[darling(default)]
    pub verbose_name: Option<String>,

    /// Verbose plural name.
    #[darling(default)]
    pub verbose_name_plural: Option<String>,

    /// Whether the model is abstract (no table).
    #[darling(default)]
    pub abstract_model: bool,

    /// Default ordering (e.g., `["-created_at", "name"]`).
    #[darling(default)]
    pub ordering: Option<StringList>,
}

/// Per-field attributes parsed from `#[field(...)]`.
#[derive(Debug, FromField)]
#[darling(attributes(field))]
pub struct FieldOpts {
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,

    /// Marks as primary key.
    #[darling(default)]
    pub primary_key: bool,

    /// Auto-increment field.
    #[darling(default)]
    pub auto: bool,

    /// Maximum character length for CharField.
    #[darling(default)]
    pub max_length: Option<usize>,

    /// Allow blank values.
    #[darling(default)]
    #[allow(dead_code)]
    pub blank: bool,

    /// Allow NULL (nullable).
    #[darling(default)]
    pub null: bool,

    /// Default value (as a string literal).
    #[darling(default)]
    pub default: Option<String>,

    /// Create a database index.
    #[darling(default)]
    pub db_index: bool,

    /// Unique constraint.
    #[darling(default)]
    pub unique: bool,

    /// Human-readable name.
    #[darling(default)]
    pub verbose_name: Option<String>,

    /// Help text.
    #[darling(default)]
    pub help_text: Option<String>,

    /// Foreign key target table.
    #[darling(default)]
    pub foreign_key: Option<String>,

    /// ON DELETE behavior.
    #[darling(default)]
    pub on_delete: Option<String>,

    /// Auto-set timestamp on save.
    #[darling(default)]
    pub auto_now: bool,

    /// Auto-set timestamp on creation.
    #[darling(default)]
    pub auto_now_add: bool,

    /// Whether the field is editable.
    #[darling(default)]
    #[allow(dead_code)]
    pub editable: Option<bool>,

    /// Database column name override.
    #[darling(default)]
    pub db_column: Option<String>,
}

/// Generates the `Model` trait implementation for the given derive input.
pub fn derive_model_impl(input: DeriveInput) -> TokenStream {
    let opts = match ModelOpts::from_derive_input(&input) {
        Ok(o) => o,
        Err(e) => return e.write_errors(),
    };

    let struct_name = &opts.ident;
    let model_name_lower = struct_name.to_string().to_lowercase();

    let app_label = opts.app.as_deref().unwrap_or("app");
    let table_name = opts
        .table
        .clone()
        .unwrap_or_else(|| format!("{app_label}_{model_name_lower}"));
    let verbose = opts
        .verbose_name
        .clone()
        .unwrap_or_else(|| model_name_lower.replace('_', " "));
    let verbose_plural = opts
        .verbose_name_plural
        .clone()
        .unwrap_or_else(|| format!("{verbose}s"));
    let abstract_model = opts.abstract_model;

    let fields = opts
        .data
        .as_ref()
        .take_struct()
        .expect("#[derive(Model)] only supports named structs")
        .fields;

    // Generate ordering tokens
    let ordering_tokens = match &opts.ordering {
        Some(ordering) => {
            let order_items: Vec<TokenStream> = ordering
                .0
                .iter()
                .map(|s| {
                    if let Some(col) = s.strip_prefix('-') {
                        quote! { django_rs_db::query::compiler::OrderBy::desc(#col) }
                    } else {
                        quote! { django_rs_db::query::compiler::OrderBy::asc(#s) }
                    }
                })
                .collect();
            quote! { vec![#(#order_items),*] }
        }
        None => quote! { vec![] },
    };

    // Generate FieldDef entries
    let field_def_tokens: Vec<TokenStream> = fields.iter().map(|f| generate_field_def(f)).collect();

    // Generate field_values() entries
    let field_value_tokens: Vec<TokenStream> = fields
        .iter()
        .map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let name_str = ident.to_string();
            quote! {
                (#name_str, django_rs_db::value::Value::from(self.#ident.clone()))
            }
        })
        .collect();

    // Find the primary key field
    let pk_field = fields.iter().find(|f| f.primary_key || f.auto);

    // Generate pk(), set_pk(), and pk_field_name() methods
    let pk_token = if let Some(pk_f) = pk_field {
        let pk_ident = pk_f.ident.as_ref().unwrap();
        let inner_ty = unwrap_option_type(&pk_f.ty);
        let is_pk_option = inner_ty.is_some();
        let pk_name = pk_ident.to_string();

        if is_pk_option {
            // Option<T> pk: Some if the field is Some
            quote! {
                fn pk(&self) -> Option<&django_rs_db::value::Value> {
                    // For Option-typed PKs, we can't return a reference to a temporary.
                    // Use None to indicate unsaved.
                    None
                }

                fn set_pk(&mut self, value: django_rs_db::value::Value) {
                    if let django_rs_db::value::Value::Int(id) = value {
                        self.#pk_ident = Some(id);
                    }
                }

                fn pk_field_name() -> &'static str {
                    #pk_name
                }
            }
        } else {
            // Non-Option pk: treat 0 as unsaved
            quote! {
                fn pk(&self) -> Option<&django_rs_db::value::Value> {
                    None // Cannot return reference to temporary Value
                }

                fn set_pk(&mut self, value: django_rs_db::value::Value) {
                    if let django_rs_db::value::Value::Int(id) = value {
                        self.#pk_ident = id;
                    }
                }

                fn pk_field_name() -> &'static str {
                    #pk_name
                }
            }
        }
    } else {
        // No pk field annotated, use default
        quote! {
            fn pk(&self) -> Option<&django_rs_db::value::Value> {
                None
            }

            fn set_pk(&mut self, _value: django_rs_db::value::Value) {
                // No primary key field annotated
            }
        }
    };

    // Generate from_row() entries
    let from_row_tokens: Vec<TokenStream> = fields
        .iter()
        .map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let name_str = ident.to_string();
            generate_from_row_field(ident, &name_str, &f.ty)
        })
        .collect();

    // Generate index entries for fields with db_index
    let index_tokens: Vec<TokenStream> = fields
        .iter()
        .filter(|f| f.db_index)
        .map(|f| {
            let field_name = f.ident.as_ref().unwrap().to_string();
            let idx_name = format!("idx_{}_{}", table_name, field_name);
            quote! {
                django_rs_db::model::Index {
                    name: Some(#idx_name.to_string()),
                    fields: vec![#field_name.to_string()],
                    unique: false,
                    index_type: django_rs_db::model::IndexType::BTree,
                    concurrently: false,
                    expressions: Vec::new(),
                    include: Vec::new(),
                    condition: None,
                }
            }
        })
        .collect();

    // Add unique index entries
    let unique_index_tokens: Vec<TokenStream> = fields
        .iter()
        .filter(|f| f.unique && !f.primary_key)
        .map(|f| {
            let field_name = f.ident.as_ref().unwrap().to_string();
            let idx_name = format!("uniq_{}_{}", table_name, field_name);
            quote! {
                django_rs_db::model::Index {
                    name: Some(#idx_name.to_string()),
                    fields: vec![#field_name.to_string()],
                    unique: true,
                    index_type: django_rs_db::model::IndexType::BTree,
                    concurrently: false,
                    expressions: Vec::new(),
                    include: Vec::new(),
                    condition: None,
                }
            }
        })
        .collect();

    let all_indexes = [index_tokens, unique_index_tokens].concat();

    let expanded = quote! {
        impl django_rs_db::model::Model for #struct_name {
            fn meta() -> &'static django_rs_db::model::ModelMeta {
                use std::sync::LazyLock;
                static META: LazyLock<django_rs_db::model::ModelMeta> = LazyLock::new(|| {
                    django_rs_db::model::ModelMeta {
                        app_label: #app_label,
                        model_name: #model_name_lower,
                        db_table: #table_name.to_string(),
                        verbose_name: #verbose.to_string(),
                        verbose_name_plural: #verbose_plural.to_string(),
                        ordering: #ordering_tokens,
                        unique_together: vec![],
                        indexes: vec![#(#all_indexes),*],
                        abstract_model: #abstract_model,
                        fields: vec![#(#field_def_tokens),*],
                        constraints: vec![],
                        inheritance_type: django_rs_db::query::compiler::InheritanceType::None,
                    }
                });
                &META
            }

            fn table_name() -> &'static str {
                #table_name
            }

            fn app_label() -> &'static str {
                #app_label
            }

            #pk_token

            fn field_values(&self) -> Vec<(&'static str, django_rs_db::value::Value)> {
                vec![
                    #(#field_value_tokens),*
                ]
            }

            fn from_row(row: &django_rs_db::query::compiler::Row) -> Result<Self, django_rs_core::DjangoError>
            where
                Self: Sized,
            {
                Ok(Self {
                    #(#from_row_tokens),*
                })
            }
        }
    };

    expanded
}

/// Generates the code to extract a field value from a `Row`.
///
/// For types that implement `FromValue` (i64, i32, f64, bool, String, Uuid, Option<T>),
/// we use `row.get::<T>(name)`. For chrono types and other types that don't have
/// `FromValue`, we extract the raw `Value` and convert manually.
fn generate_from_row_field(ident: &syn::Ident, name_str: &str, ty: &Type) -> TokenStream {
    let inner = unwrap_option_type(ty);
    let effective_ty = inner.unwrap_or(ty);
    let type_str = type_to_string(effective_ty);
    let is_option = inner.is_some();

    // Types that need manual conversion from Value (no FromValue impl)
    let needs_manual = type_str.contains("NaiveDate")
        || type_str.contains("NaiveTime")
        || type_str.contains("Duration")
        || type_str.contains("serde_json");

    if needs_manual {
        let conversion = generate_value_conversion(effective_ty, &type_str);
        if is_option {
            quote! {
                #ident: {
                    let val = row.get::<django_rs_db::value::Value>(#name_str)?;
                    match val {
                        django_rs_db::value::Value::Null => None,
                        v => Some(#conversion),
                    }
                }
            }
        } else {
            quote! {
                #ident: {
                    let v = row.get::<django_rs_db::value::Value>(#name_str)?;
                    #conversion
                }
            }
        }
    } else {
        // Use row.get::<T> directly
        quote! {
            #ident: row.get(#name_str)?
        }
    }
}

/// Generates code to convert a `Value` (in variable `v`) to the target type.
fn generate_value_conversion(_ty: &Type, type_str: &str) -> TokenStream {
    if type_str.contains("NaiveDateTime") {
        quote! {
            match v {
                django_rs_db::value::Value::DateTime(dt) => dt,
                _ => return Err(django_rs_core::DjangoError::DatabaseError(
                    format!("Expected DateTime, got {:?}", v)
                )),
            }
        }
    } else if type_str.contains("NaiveDate") {
        quote! {
            match v {
                django_rs_db::value::Value::Date(d) => d,
                _ => return Err(django_rs_core::DjangoError::DatabaseError(
                    format!("Expected Date, got {:?}", v)
                )),
            }
        }
    } else if type_str.contains("NaiveTime") {
        quote! {
            match v {
                django_rs_db::value::Value::Time(t) => t,
                _ => return Err(django_rs_core::DjangoError::DatabaseError(
                    format!("Expected Time, got {:?}", v)
                )),
            }
        }
    } else if type_str.contains("serde_json") {
        quote! {
            match v {
                django_rs_db::value::Value::Json(j) => j,
                _ => return Err(django_rs_core::DjangoError::DatabaseError(
                    format!("Expected Json, got {:?}", v)
                )),
            }
        }
    } else {
        // Fallback - try to use From/Into
        quote! {
            return Err(django_rs_core::DjangoError::DatabaseError(
                format!("Unsupported type conversion from {:?}", v)
            ))
        }
    }
}

/// Generates a `FieldDef` construction expression for one field.
fn generate_field_def(f: &FieldOpts) -> TokenStream {
    let name_str = f.ident.as_ref().unwrap().to_string();
    let field_type = infer_field_type(f);

    let mut chain = Vec::new();

    if f.primary_key {
        chain.push(quote! { .primary_key() });
    }
    if f.null || is_option_type(&f.ty) {
        chain.push(quote! { .nullable() });
    }
    if let Some(ml) = f.max_length {
        chain.push(quote! { .max_length(#ml) });
    }
    if f.db_index {
        chain.push(quote! { .db_index() });
    }
    if f.unique {
        chain.push(quote! { .unique() });
    }
    if let Some(ref vn) = f.verbose_name {
        chain.push(quote! { .verbose_name(#vn) });
    }
    if let Some(ref ht) = f.help_text {
        chain.push(quote! { .help_text(#ht) });
    }
    if let Some(ref def) = f.default {
        chain.push(quote! { .default(django_rs_db::value::Value::String(#def.to_string())) });
    }
    if let Some(ref col) = f.db_column {
        chain.push(quote! { .column(#col) });
    }

    quote! {
        django_rs_db::fields::FieldDef::new(#name_str, #field_type)
            #(#chain)*
    }
}

/// Infers the `FieldType` variant from the Rust type and field attributes.
fn infer_field_type(f: &FieldOpts) -> TokenStream {
    // Check for foreign key first
    if let Some(ref fk_table) = f.foreign_key {
        let on_del = match f.on_delete.as_deref() {
            Some("cascade") => quote! { django_rs_db::fields::OnDelete::Cascade },
            Some("protect") => quote! { django_rs_db::fields::OnDelete::Protect },
            Some("set_null") => quote! { django_rs_db::fields::OnDelete::SetNull },
            Some("set_default") => quote! { django_rs_db::fields::OnDelete::SetDefault },
            Some("do_nothing") => quote! { django_rs_db::fields::OnDelete::DoNothing },
            _ => quote! { django_rs_db::fields::OnDelete::Cascade },
        };
        return quote! {
            django_rs_db::fields::FieldType::ForeignKey {
                to: #fk_table.to_string(),
                on_delete: #on_del,
                related_name: None,
            }
        };
    }

    let inner_type = unwrap_option_type(&f.ty).unwrap_or(&f.ty);
    let type_str = type_to_string(inner_type);

    // Auto fields
    if f.auto {
        if type_str == "i64" {
            return quote! { django_rs_db::fields::FieldType::BigAutoField };
        }
        return quote! { django_rs_db::fields::FieldType::AutoField };
    }

    // auto_now / auto_now_add -> DateTimeField
    if f.auto_now || f.auto_now_add {
        return quote! { django_rs_db::fields::FieldType::DateTimeField };
    }

    match type_str.as_str() {
        "i32" => quote! { django_rs_db::fields::FieldType::IntegerField },
        "i16" => quote! { django_rs_db::fields::FieldType::SmallIntegerField },
        "i64" => quote! { django_rs_db::fields::FieldType::BigIntegerField },
        "f64" | "f32" => quote! { django_rs_db::fields::FieldType::FloatField },
        "bool" => quote! { django_rs_db::fields::FieldType::BooleanField },
        "String" if f.max_length.is_some() => {
            quote! { django_rs_db::fields::FieldType::CharField }
        }
        "String" => quote! { django_rs_db::fields::FieldType::TextField },
        _ => {
            // Check for path-based types
            if type_str.contains("NaiveDateTime") {
                quote! { django_rs_db::fields::FieldType::DateTimeField }
            } else if type_str.contains("NaiveDate") {
                quote! { django_rs_db::fields::FieldType::DateField }
            } else if type_str.contains("NaiveTime") {
                quote! { django_rs_db::fields::FieldType::TimeField }
            } else if type_str.contains("Uuid") {
                quote! { django_rs_db::fields::FieldType::UuidField }
            } else if type_str.contains("serde_json") || type_str.contains("Value") {
                quote! { django_rs_db::fields::FieldType::JsonField }
            } else if type_str.contains("Vec") && type_str.contains("u8") {
                quote! { django_rs_db::fields::FieldType::BinaryField }
            } else {
                quote! { django_rs_db::fields::FieldType::TextField }
            }
        }
    }
}

/// Checks if a type is `Option<T>`.
pub(crate) fn is_option_type(ty: &Type) -> bool {
    unwrap_option_type(ty).is_some()
}

/// If the type is `Option<T>`, returns `Some(&T)`. Otherwise `None`.
pub(crate) fn unwrap_option_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "Option" {
            if let syn::PathArguments::AngleBracketed(ref args) = segment.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some(inner);
                }
            }
        }
    }
    None
}

/// Converts a `syn::Type` to a string for matching.
pub(crate) fn type_to_string(ty: &Type) -> String {
    quote!(#ty).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_is_option_type() {
        let ty: Type = parse_quote!(Option<String>);
        assert!(is_option_type(&ty));

        let ty: Type = parse_quote!(String);
        assert!(!is_option_type(&ty));
    }

    #[test]
    fn test_unwrap_option_type() {
        let ty: Type = parse_quote!(Option<i64>);
        let inner = unwrap_option_type(&ty).unwrap();
        assert_eq!(type_to_string(inner), "i64");
    }

    #[test]
    fn test_type_to_string_primitives() {
        let ty: Type = parse_quote!(i64);
        assert_eq!(type_to_string(&ty), "i64");

        let ty: Type = parse_quote!(bool);
        assert_eq!(type_to_string(&ty), "bool");

        let ty: Type = parse_quote!(String);
        assert_eq!(type_to_string(&ty), "String");
    }

    #[test]
    fn test_type_to_string_chrono() {
        let ty: Type = parse_quote!(chrono::NaiveDateTime);
        let s = type_to_string(&ty);
        assert!(s.contains("NaiveDateTime"));
    }

    #[test]
    fn test_type_to_string_vec_u8() {
        let ty: Type = parse_quote!(Vec<u8>);
        let s = type_to_string(&ty);
        assert!(s.contains("Vec"));
        assert!(s.contains("u8"));
    }
}
