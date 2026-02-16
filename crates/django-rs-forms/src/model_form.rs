//! Model-backed forms that auto-generate fields from ORM model metadata.
//!
//! [`ModelFormConfig`] specifies how to generate form fields from a model's
//! [`ModelMeta`](django_rs_db::model::ModelMeta). The [`generate_form_fields`]
//! function creates [`FormFieldDef`] instances from the model's
//! [`FieldDef`](django_rs_db::fields::FieldDef) entries.
//!
//! This mirrors Django's `django.forms.ModelForm` and `ModelFormOptions`.

use std::collections::HashMap;

use django_rs_db::fields::{FieldDef, FieldType};
use django_rs_db::model::ModelMeta;

use crate::fields::{FormFieldDef, FormFieldType};
use crate::widgets::WidgetType;

/// Configuration for generating a model-backed form.
///
/// Specifies which model fields to include/exclude and allows overriding
/// widgets, labels, and help texts for the generated form fields.
pub struct ModelFormConfig {
    /// The model metadata to generate fields from.
    pub model_meta: &'static ModelMeta,
    /// Which model fields to include in the form.
    pub fields: ModelFormFields,
    /// Widget overrides keyed by field name.
    pub widgets: HashMap<String, WidgetType>,
    /// Label overrides keyed by field name.
    pub labels: HashMap<String, String>,
    /// Help text overrides keyed by field name.
    pub help_texts: HashMap<String, String>,
}

/// Specifies which model fields to include in a `ModelForm`.
#[derive(Debug, Clone)]
pub enum ModelFormFields {
    /// Include all editable fields.
    All,
    /// Include only the specified fields.
    Include(Vec<String>),
    /// Include all fields except the specified ones.
    Exclude(Vec<String>),
}

impl ModelFormConfig {
    /// Creates a new `ModelFormConfig` with all fields included.
    pub fn new(model_meta: &'static ModelMeta) -> Self {
        Self {
            model_meta,
            fields: ModelFormFields::All,
            widgets: HashMap::new(),
            labels: HashMap::new(),
            help_texts: HashMap::new(),
        }
    }

    /// Sets which fields to include.
    pub fn with_fields(mut self, fields: ModelFormFields) -> Self {
        self.fields = fields;
        self
    }

    /// Adds a widget override for a specific field.
    pub fn with_widget(mut self, field_name: impl Into<String>, widget: WidgetType) -> Self {
        self.widgets.insert(field_name.into(), widget);
        self
    }

    /// Adds a label override for a specific field.
    pub fn with_label(mut self, field_name: impl Into<String>, label: impl Into<String>) -> Self {
        self.labels.insert(field_name.into(), label.into());
        self
    }

    /// Adds a help text override for a specific field.
    pub fn with_help_text(
        mut self,
        field_name: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        self.help_texts.insert(field_name.into(), text.into());
        self
    }
}

/// Generates form field definitions from a model form configuration.
///
/// Iterates over the model's field definitions and creates corresponding
/// [`FormFieldDef`] instances, applying any overrides from the config.
///
/// Fields that are not editable, are primary keys, or are relational
/// (foreign keys, many-to-many) are excluded by default.
pub fn generate_form_fields(config: &ModelFormConfig) -> Vec<FormFieldDef> {
    let mut form_fields = Vec::new();

    for model_field in &config.model_meta.fields {
        // Skip non-editable fields
        if !model_field.editable {
            continue;
        }

        // Skip primary keys
        if model_field.primary_key {
            continue;
        }

        // Skip relational fields (handled separately in Django)
        if model_field.is_relation() {
            continue;
        }

        // Check field inclusion/exclusion
        let field_name = model_field.name.to_string();
        match &config.fields {
            ModelFormFields::All => {}
            ModelFormFields::Include(include) => {
                if !include.contains(&field_name) {
                    continue;
                }
            }
            ModelFormFields::Exclude(exclude) => {
                if exclude.contains(&field_name) {
                    continue;
                }
            }
        }

        // Convert model field type to form field type
        let form_field_type = model_field_to_form_field_type(model_field);

        let mut form_field = FormFieldDef::new(&field_name, form_field_type);

        // Required: a field is required if it's not nullable and has no default
        form_field.required =
            !model_field.null && !model_field.blank && model_field.default.is_none();

        // Apply overrides
        if let Some(widget) = config.widgets.get(&field_name) {
            form_field.widget = widget.clone();
        }
        if let Some(label) = config.labels.get(&field_name) {
            form_field.label = label.clone();
        } else {
            form_field.label = model_field.verbose_name.clone();
        }
        if let Some(help_text) = config.help_texts.get(&field_name) {
            form_field.help_text = help_text.clone();
        } else {
            form_field.help_text = model_field.help_text.clone();
        }

        // Set initial from default
        if let Some(default) = &model_field.default {
            form_field.initial = Some(default.clone());
        }

        form_fields.push(form_field);
    }

    form_fields
}

/// Converts an ORM field type to a form field type.
fn model_field_to_form_field_type(field_def: &FieldDef) -> FormFieldType {
    match &field_def.field_type {
        FieldType::CharField | FieldType::TextField => FormFieldType::Char {
            min_length: None,
            max_length: field_def.max_length,
            strip: true,
        },
        FieldType::IntegerField | FieldType::BigIntegerField | FieldType::SmallIntegerField => {
            FormFieldType::Integer {
                min_value: None,
                max_value: None,
            }
        }
        FieldType::FloatField => FormFieldType::Float {
            min_value: None,
            max_value: None,
        },
        FieldType::DecimalField {
            max_digits,
            decimal_places,
        } => FormFieldType::Decimal {
            max_digits: *max_digits,
            decimal_places: *decimal_places,
        },
        FieldType::BooleanField => FormFieldType::Boolean,
        FieldType::DateField => FormFieldType::Date,
        FieldType::DateTimeField => FormFieldType::DateTime,
        FieldType::TimeField => FormFieldType::Time,
        FieldType::DurationField => FormFieldType::Duration,
        FieldType::UuidField => FormFieldType::Uuid,
        FieldType::EmailField => FormFieldType::Email,
        FieldType::UrlField => FormFieldType::Url,
        FieldType::SlugField => FormFieldType::Slug,
        FieldType::IpAddressField => FormFieldType::IpAddress,
        FieldType::JsonField => FormFieldType::Json,
        FieldType::BinaryField | FieldType::FilePathField => FormFieldType::Char {
            min_length: None,
            max_length: None,
            strip: false,
        },
        // Auto fields should be excluded by the primary_key check above,
        // but handle gracefully
        FieldType::AutoField | FieldType::BigAutoField => FormFieldType::Integer {
            min_value: None,
            max_value: None,
        },
        // Relational fields should be excluded above
        FieldType::ForeignKey { .. }
        | FieldType::OneToOneField { .. }
        | FieldType::ManyToManyField { .. } => FormFieldType::Integer {
            min_value: None,
            max_value: None,
        },
        // PostgreSQL-specific fields: use JSON representation in forms
        FieldType::ArrayField { .. }
        | FieldType::HStoreField
        | FieldType::IntegerRangeField
        | FieldType::BigIntegerRangeField
        | FieldType::FloatRangeField
        | FieldType::DateRangeField
        | FieldType::DateTimeRangeField => FormFieldType::Json,
        // Generated fields should not appear in forms (they are computed)
        FieldType::GeneratedField { .. } => FormFieldType::Char {
            min_length: None,
            max_length: None,
            strip: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use django_rs_db::fields::{FieldDef, FieldType};
    use django_rs_db::model::ModelMeta;
    use django_rs_db::value::Value;
    use std::sync::LazyLock;

    static TEST_META: LazyLock<ModelMeta> = LazyLock::new(|| ModelMeta {
        app_label: "test",
        model_name: "article",
        db_table: "test_article".to_string(),
        verbose_name: "article".to_string(),
        verbose_name_plural: "articles".to_string(),
        ordering: vec![],
        unique_together: vec![],
        indexes: vec![],
        abstract_model: false,
        fields: vec![
            FieldDef::new("id", FieldType::BigAutoField).primary_key(),
            FieldDef::new("title", FieldType::CharField)
                .max_length(200)
                .verbose_name("Title"),
            FieldDef::new("content", FieldType::TextField)
                .verbose_name("Content")
                .help_text("Article body text"),
            FieldDef::new("email", FieldType::EmailField).verbose_name("Author Email"),
            FieldDef::new("published", FieldType::BooleanField).default(Value::Bool(false)),
            FieldDef::new("views", FieldType::IntegerField).default(Value::Int(0)),
            FieldDef::new("rating", FieldType::FloatField).nullable(),
            FieldDef::new(
                "price",
                FieldType::DecimalField {
                    max_digits: 10,
                    decimal_places: 2,
                },
            ),
            FieldDef::new("publish_date", FieldType::DateField).nullable(),
            FieldDef::new("slug", FieldType::SlugField),
        ],
        constraints: vec![],
        inheritance_type: django_rs_db::query::compiler::InheritanceType::None,
    });

    fn get_test_meta() -> &'static ModelMeta {
        &TEST_META
    }

    #[test]
    fn test_generate_all_fields() {
        let config = ModelFormConfig::new(get_test_meta());
        let fields = generate_form_fields(&config);

        // Should exclude 'id' (primary key)
        assert!(!fields.iter().any(|f| f.name == "id"));

        // Should include the rest
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"title"));
        assert!(names.contains(&"content"));
        assert!(names.contains(&"email"));
        assert!(names.contains(&"published"));
        assert!(names.contains(&"views"));
        assert!(names.contains(&"rating"));
        assert!(names.contains(&"price"));
        assert!(names.contains(&"publish_date"));
        assert!(names.contains(&"slug"));
    }

    #[test]
    fn test_generate_include_fields() {
        let config =
            ModelFormConfig::new(get_test_meta()).with_fields(ModelFormFields::Include(vec![
                "title".into(),
                "content".into(),
            ]));
        let fields = generate_form_fields(&config);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "title");
        assert_eq!(fields[1].name, "content");
    }

    #[test]
    fn test_generate_exclude_fields() {
        let config =
            ModelFormConfig::new(get_test_meta()).with_fields(ModelFormFields::Exclude(vec![
                "content".into(),
                "email".into(),
            ]));
        let fields = generate_form_fields(&config);
        assert!(!fields.iter().any(|f| f.name == "content"));
        assert!(!fields.iter().any(|f| f.name == "email"));
        assert!(fields.iter().any(|f| f.name == "title"));
    }

    #[test]
    fn test_field_types_match() {
        let config = ModelFormConfig::new(get_test_meta());
        let fields = generate_form_fields(&config);

        let title = fields.iter().find(|f| f.name == "title").unwrap();
        assert!(matches!(
            title.field_type,
            FormFieldType::Char {
                max_length: Some(200),
                ..
            }
        ));

        let email_field = fields.iter().find(|f| f.name == "email").unwrap();
        assert!(matches!(email_field.field_type, FormFieldType::Email));

        let published = fields.iter().find(|f| f.name == "published").unwrap();
        assert!(matches!(published.field_type, FormFieldType::Boolean));

        let views = fields.iter().find(|f| f.name == "views").unwrap();
        assert!(matches!(views.field_type, FormFieldType::Integer { .. }));

        let rating = fields.iter().find(|f| f.name == "rating").unwrap();
        assert!(matches!(rating.field_type, FormFieldType::Float { .. }));

        let price = fields.iter().find(|f| f.name == "price").unwrap();
        assert!(matches!(
            price.field_type,
            FormFieldType::Decimal {
                max_digits: 10,
                decimal_places: 2
            }
        ));

        let publish_date = fields.iter().find(|f| f.name == "publish_date").unwrap();
        assert!(matches!(publish_date.field_type, FormFieldType::Date));

        let slug_field = fields.iter().find(|f| f.name == "slug").unwrap();
        assert!(matches!(slug_field.field_type, FormFieldType::Slug));
    }

    #[test]
    fn test_required_from_model() {
        let config = ModelFormConfig::new(get_test_meta());
        let fields = generate_form_fields(&config);

        // title: not null, no default -> required
        let title = fields.iter().find(|f| f.name == "title").unwrap();
        assert!(title.required);

        // published: has default -> not required
        let published = fields.iter().find(|f| f.name == "published").unwrap();
        assert!(!published.required);

        // rating: nullable -> not required
        let rating = fields.iter().find(|f| f.name == "rating").unwrap();
        assert!(!rating.required);
    }

    #[test]
    fn test_widget_override() {
        let config =
            ModelFormConfig::new(get_test_meta()).with_widget("content", WidgetType::Textarea);
        let fields = generate_form_fields(&config);
        let content = fields.iter().find(|f| f.name == "content").unwrap();
        assert_eq!(content.widget, WidgetType::Textarea);
    }

    #[test]
    fn test_label_override() {
        let config = ModelFormConfig::new(get_test_meta()).with_label("title", "Article Title");
        let fields = generate_form_fields(&config);
        let title = fields.iter().find(|f| f.name == "title").unwrap();
        assert_eq!(title.label, "Article Title");
    }

    #[test]
    fn test_help_text_override() {
        let config = ModelFormConfig::new(get_test_meta())
            .with_help_text("title", "Enter a descriptive title");
        let fields = generate_form_fields(&config);
        let title = fields.iter().find(|f| f.name == "title").unwrap();
        assert_eq!(title.help_text, "Enter a descriptive title");
    }

    #[test]
    fn test_default_labels_from_model() {
        let config = ModelFormConfig::new(get_test_meta());
        let fields = generate_form_fields(&config);
        let title = fields.iter().find(|f| f.name == "title").unwrap();
        assert_eq!(title.label, "Title");
    }

    #[test]
    fn test_default_help_text_from_model() {
        let config = ModelFormConfig::new(get_test_meta());
        let fields = generate_form_fields(&config);
        let content = fields.iter().find(|f| f.name == "content").unwrap();
        assert_eq!(content.help_text, "Article body text");
    }

    #[test]
    fn test_initial_from_default() {
        let config = ModelFormConfig::new(get_test_meta());
        let fields = generate_form_fields(&config);
        let published = fields.iter().find(|f| f.name == "published").unwrap();
        assert_eq!(published.initial, Some(Value::Bool(false)));
        let views = fields.iter().find(|f| f.name == "views").unwrap();
        assert_eq!(views.initial, Some(Value::Int(0)));
    }

    #[test]
    fn test_config_builder_chain() {
        let config = ModelFormConfig::new(get_test_meta())
            .with_fields(ModelFormFields::Include(vec!["title".into()]))
            .with_widget("title", WidgetType::Textarea)
            .with_label("title", "Article Title")
            .with_help_text("title", "Enter a title");
        let fields = generate_form_fields(&config);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].widget, WidgetType::Textarea);
        assert_eq!(fields[0].label, "Article Title");
        assert_eq!(fields[0].help_text, "Enter a title");
    }
}
