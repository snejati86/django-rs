//! Model administration configuration.
//!
//! This module provides [`ModelAdmin`] and related types for configuring how
//! models are displayed and managed in the admin panel. It mirrors Django's
//! `ModelAdmin` class with a builder pattern for ergonomic configuration.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Configuration for how a model is displayed and managed in the admin panel.
///
/// Mirrors Django's `ModelAdmin` class. Each registered model gets a `ModelAdmin`
/// that controls list display, filtering, searching, fieldsets, inline editing,
/// and available actions.
///
/// # Examples
///
/// ```
/// use django_rs_admin::model_admin::ModelAdmin;
///
/// let admin = ModelAdmin::new("blog", "article")
///     .list_display(vec!["title", "author", "published_date"])
///     .list_filter_fields(vec!["published_date", "author"])
///     .search_fields(vec!["title", "body"])
///     .ordering(vec!["-published_date"])
///     .list_per_page(25);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAdmin {
    /// The application label (e.g., "blog").
    pub app_label: String,
    /// The model name in lowercase (e.g., "article").
    pub model_name: String,
    /// The human-readable verbose name.
    pub verbose_name: String,
    /// The human-readable plural verbose name.
    pub verbose_name_plural: String,
    /// Fields to display in the list view.
    pub list_display: Vec<String>,
    /// Fields that link to the detail/change view.
    pub list_display_links: Vec<String>,
    /// Filters available in the list view.
    pub list_filter: Vec<ListFilter>,
    /// Fields searched when using the search box.
    pub search_fields: Vec<String>,
    /// Default ordering for the list view (prefix with "-" for descending).
    pub ordering: Vec<String>,
    /// Number of items per page in list view.
    pub list_per_page: usize,
    /// Maximum number of items to show with "Show all".
    pub list_max_show_all: usize,
    /// Fields that are read-only in forms.
    pub readonly_fields: Vec<String>,
    /// Fields to exclude from forms.
    pub exclude: Vec<String>,
    /// Fieldset groupings for the detail/change view.
    pub fieldsets: Vec<Fieldset>,
    /// Inline model editors.
    pub inlines: Vec<InlineAdmin>,
    /// Available admin actions (stored as action names).
    pub action_names: Vec<String>,
    /// Whether to show save buttons at the top of the form.
    pub save_on_top: bool,
    /// Fields editable directly in the list view.
    pub list_editable: Vec<String>,
    /// A date field for hierarchical date-based drilling.
    pub date_hierarchy: Option<String>,
    /// Fields that auto-populate from other fields.
    pub prepopulated_fields: HashMap<String, Vec<String>>,
    /// Schema information about model fields (for React frontend introspection).
    pub fields_schema: Vec<FieldSchema>,
}

impl ModelAdmin {
    /// Creates a new `ModelAdmin` with default configuration.
    pub fn new(app_label: impl Into<String>, model_name: impl Into<String>) -> Self {
        let model = model_name.into();
        let app = app_label.into();
        let verbose = model.replace('_', " ");
        let verbose_plural = format!("{verbose}s");
        Self {
            app_label: app,
            model_name: model,
            verbose_name: verbose,
            verbose_name_plural: verbose_plural,
            list_display: vec!["__str__".to_string()],
            list_display_links: Vec::new(),
            list_filter: Vec::new(),
            search_fields: Vec::new(),
            ordering: Vec::new(),
            list_per_page: 100,
            list_max_show_all: 200,
            readonly_fields: Vec::new(),
            exclude: Vec::new(),
            fieldsets: Vec::new(),
            inlines: Vec::new(),
            action_names: vec!["delete_selected".to_string()],
            save_on_top: false,
            list_editable: Vec::new(),
            date_hierarchy: None,
            prepopulated_fields: HashMap::new(),
            fields_schema: Vec::new(),
        }
    }

    /// Sets the verbose name.
    #[must_use]
    pub fn verbose_name(mut self, name: impl Into<String>) -> Self {
        self.verbose_name = name.into();
        self
    }

    /// Sets the plural verbose name.
    #[must_use]
    pub fn verbose_name_plural(mut self, name: impl Into<String>) -> Self {
        self.verbose_name_plural = name.into();
        self
    }

    /// Sets the fields to display in the list view.
    #[must_use]
    pub fn list_display(mut self, fields: Vec<&str>) -> Self {
        self.list_display = fields.into_iter().map(String::from).collect();
        self
    }

    /// Sets the fields that link to the detail view.
    #[must_use]
    pub fn list_display_links(mut self, fields: Vec<&str>) -> Self {
        self.list_display_links = fields.into_iter().map(String::from).collect();
        self
    }

    /// Sets the list filters from field names.
    #[must_use]
    pub fn list_filter_fields(mut self, fields: Vec<&str>) -> Self {
        self.list_filter = fields
            .into_iter()
            .map(|f| ListFilter::Field(f.to_string()))
            .collect();
        self
    }

    /// Sets the list filters from `ListFilter` values.
    #[must_use]
    pub fn list_filter(mut self, filters: Vec<ListFilter>) -> Self {
        self.list_filter = filters;
        self
    }

    /// Sets the fields to search over.
    #[must_use]
    pub fn search_fields(mut self, fields: Vec<&str>) -> Self {
        self.search_fields = fields.into_iter().map(String::from).collect();
        self
    }

    /// Sets the default ordering.
    #[must_use]
    pub fn ordering(mut self, fields: Vec<&str>) -> Self {
        self.ordering = fields.into_iter().map(String::from).collect();
        self
    }

    /// Sets the number of items per page.
    #[must_use]
    pub const fn list_per_page(mut self, count: usize) -> Self {
        self.list_per_page = count;
        self
    }

    /// Sets the maximum number of items for "Show all".
    #[must_use]
    pub const fn list_max_show_all(mut self, count: usize) -> Self {
        self.list_max_show_all = count;
        self
    }

    /// Sets the read-only fields.
    #[must_use]
    pub fn readonly_fields(mut self, fields: Vec<&str>) -> Self {
        self.readonly_fields = fields.into_iter().map(String::from).collect();
        self
    }

    /// Sets the excluded fields.
    #[must_use]
    pub fn exclude(mut self, fields: Vec<&str>) -> Self {
        self.exclude = fields.into_iter().map(String::from).collect();
        self
    }

    /// Sets the fieldsets for the detail/change view.
    #[must_use]
    pub fn fieldsets(mut self, fieldsets: Vec<Fieldset>) -> Self {
        self.fieldsets = fieldsets;
        self
    }

    /// Sets the inline model editors.
    #[must_use]
    pub fn inlines(mut self, inlines: Vec<InlineAdmin>) -> Self {
        self.inlines = inlines;
        self
    }

    /// Enables the save-on-top button.
    #[must_use]
    pub const fn save_on_top(mut self, enabled: bool) -> Self {
        self.save_on_top = enabled;
        self
    }

    /// Sets fields editable in the list view.
    #[must_use]
    pub fn list_editable(mut self, fields: Vec<&str>) -> Self {
        self.list_editable = fields.into_iter().map(String::from).collect();
        self
    }

    /// Sets the date hierarchy field.
    #[must_use]
    pub fn date_hierarchy(mut self, field: &str) -> Self {
        self.date_hierarchy = Some(field.to_string());
        self
    }

    /// Sets prepopulated fields mapping.
    #[must_use]
    pub fn prepopulated_fields(mut self, fields: HashMap<String, Vec<String>>) -> Self {
        self.prepopulated_fields = fields;
        self
    }

    /// Sets the field schema for introspection.
    #[must_use]
    pub fn fields_schema(mut self, schema: Vec<FieldSchema>) -> Self {
        self.fields_schema = schema;
        self
    }

    /// Returns the model key in `"app_label.model_name"` format.
    pub fn model_key(&self) -> String {
        format!("{}.{}", self.app_label, self.model_name)
    }
}

/// A grouping of fields in the admin detail/change view.
///
/// Mirrors Django's fieldset tuple `(name, {"fields": [...], "classes": [...], "description": "..."})`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fieldset {
    /// Optional display name for this fieldset group.
    pub name: Option<String>,
    /// The fields included in this fieldset.
    pub fields: Vec<String>,
    /// CSS classes to apply to this fieldset (e.g., `"collapse"`, `"wide"`).
    pub classes: Vec<String>,
    /// Optional description text displayed below the fieldset title.
    pub description: Option<String>,
}

impl Fieldset {
    /// Creates a new fieldset with the given fields and no title.
    pub fn new(fields: Vec<&str>) -> Self {
        Self {
            name: None,
            fields: fields.into_iter().map(String::from).collect(),
            classes: Vec::new(),
            description: None,
        }
    }

    /// Sets the fieldset title.
    #[must_use]
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// Sets the CSS classes.
    #[must_use]
    pub fn classes(mut self, classes: Vec<&str>) -> Self {
        self.classes = classes.into_iter().map(String::from).collect();
        self
    }

    /// Sets the description.
    #[must_use]
    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
}

/// Configuration for inline model editing within a parent model's admin page.
///
/// Mirrors Django's `TabularInline` and `StackedInline`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineAdmin {
    /// The app label of the inline model.
    pub app_label: String,
    /// The model name of the inline model.
    pub model_name: String,
    /// The display layout for the inline editor.
    pub inline_type: InlineType,
    /// Number of extra empty forms to display.
    pub extra: usize,
    /// Minimum number of inline forms.
    pub min_num: usize,
    /// Maximum number of inline forms, if any.
    pub max_num: Option<usize>,
    /// Fields to display in the inline form.
    pub fields: Vec<String>,
}

impl InlineAdmin {
    /// Creates a new inline admin configuration.
    pub fn new(
        app_label: impl Into<String>,
        model_name: impl Into<String>,
        inline_type: InlineType,
    ) -> Self {
        Self {
            app_label: app_label.into(),
            model_name: model_name.into(),
            inline_type,
            extra: 3,
            min_num: 0,
            max_num: None,
            fields: Vec::new(),
        }
    }

    /// Sets the number of extra empty forms.
    #[must_use]
    pub const fn extra(mut self, n: usize) -> Self {
        self.extra = n;
        self
    }

    /// Sets the minimum number of forms.
    #[must_use]
    pub const fn min_num(mut self, n: usize) -> Self {
        self.min_num = n;
        self
    }

    /// Sets the maximum number of forms.
    #[must_use]
    pub const fn max_num(mut self, n: Option<usize>) -> Self {
        self.max_num = n;
        self
    }

    /// Sets the fields to display.
    #[must_use]
    pub fn fields(mut self, fields: Vec<&str>) -> Self {
        self.fields = fields.into_iter().map(String::from).collect();
        self
    }
}

/// The visual layout style for inline model editors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineType {
    /// Table-based layout with each inline as a row.
    Tabular,
    /// Form-based layout with each inline as a full form block.
    Stacked,
}

/// A filter configuration for the admin list view.
///
/// Supports automatic filters based on field values, date-based drill-down,
/// and custom filter definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ListFilter {
    /// Auto-filter by distinct values of a field.
    Field(String),
    /// Date-based hierarchical drill-down on a date/datetime field.
    DateHierarchy(String),
    /// Custom filter with explicit choices.
    Custom {
        /// Display name for the filter.
        name: String,
        /// Available filter choices.
        choices: Vec<FilterChoice>,
    },
}

/// A single choice within a filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterChoice {
    /// The display label shown to the user.
    pub display: String,
    /// The value sent as a query parameter.
    pub value: String,
}

impl FilterChoice {
    /// Creates a new filter choice.
    pub fn new(display: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            display: display.into(),
            value: value.into(),
        }
    }
}

/// Schema information about a single model field, used for React frontend introspection.
///
/// This is sent to the frontend so it knows how to render forms and list columns
/// without needing separate schema definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct FieldSchema {
    /// The field name.
    pub name: String,
    /// The field type as a string (e.g., "`CharField`", "`IntegerField`", "`ForeignKey`").
    pub field_type: String,
    /// Whether this field is required (non-null, non-blank).
    pub required: bool,
    /// Whether this field is read-only.
    pub read_only: bool,
    /// Whether this field is the primary key.
    pub primary_key: bool,
    /// Maximum character length, if applicable.
    pub max_length: Option<usize>,
    /// Human-readable label.
    pub label: String,
    /// Help text for the field.
    pub help_text: String,
    /// Allowed choices as (value, label) pairs, if any.
    pub choices: Option<Vec<(String, String)>>,
    /// Whether this is a relational field.
    pub is_relation: bool,
    /// The target model for relational fields (e.g., "auth.user").
    pub related_model: Option<String>,
}

impl FieldSchema {
    /// Creates a new field schema entry.
    pub fn new(name: impl Into<String>, field_type: impl Into<String>) -> Self {
        let n = name.into();
        let label = n.replace('_', " ");
        Self {
            name: n,
            field_type: field_type.into(),
            required: true,
            read_only: false,
            primary_key: false,
            max_length: None,
            label,
            help_text: String::new(),
            choices: None,
            is_relation: false,
            related_model: None,
        }
    }

    /// Marks this field as optional (not required).
    #[must_use]
    pub const fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// Marks this field as read-only.
    #[must_use]
    pub const fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// Marks this field as the primary key.
    #[must_use]
    pub const fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self.read_only = true;
        self
    }

    /// Sets the maximum length.
    #[must_use]
    pub const fn max_length(mut self, len: usize) -> Self {
        self.max_length = Some(len);
        self
    }

    /// Sets the human-readable label.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Sets the help text.
    #[must_use]
    pub fn help_text(mut self, text: impl Into<String>) -> Self {
        self.help_text = text.into();
        self
    }

    /// Sets the field as relational with the given target model.
    #[must_use]
    pub fn relation(mut self, related_model: impl Into<String>) -> Self {
        self.is_relation = true;
        self.related_model = Some(related_model.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_admin_new_defaults() {
        let admin = ModelAdmin::new("blog", "article");
        assert_eq!(admin.app_label, "blog");
        assert_eq!(admin.model_name, "article");
        assert_eq!(admin.verbose_name, "article");
        assert_eq!(admin.verbose_name_plural, "articles");
        assert_eq!(admin.list_display, vec!["__str__"]);
        assert!(admin.list_display_links.is_empty());
        assert!(admin.list_filter.is_empty());
        assert!(admin.search_fields.is_empty());
        assert!(admin.ordering.is_empty());
        assert_eq!(admin.list_per_page, 100);
        assert_eq!(admin.list_max_show_all, 200);
        assert!(!admin.save_on_top);
        assert!(admin.date_hierarchy.is_none());
        assert_eq!(admin.action_names, vec!["delete_selected"]);
    }

    #[test]
    fn test_model_admin_builder() {
        let admin = ModelAdmin::new("blog", "article")
            .list_display(vec!["title", "author", "date"])
            .list_display_links(vec!["title"])
            .search_fields(vec!["title", "body"])
            .ordering(vec!["-date"])
            .list_per_page(25)
            .list_max_show_all(500)
            .readonly_fields(vec!["created_at"])
            .exclude(vec!["internal_field"])
            .save_on_top(true)
            .list_editable(vec!["author"])
            .date_hierarchy("date");

        assert_eq!(admin.list_display, vec!["title", "author", "date"]);
        assert_eq!(admin.list_display_links, vec!["title"]);
        assert_eq!(admin.search_fields, vec!["title", "body"]);
        assert_eq!(admin.ordering, vec!["-date"]);
        assert_eq!(admin.list_per_page, 25);
        assert_eq!(admin.list_max_show_all, 500);
        assert_eq!(admin.readonly_fields, vec!["created_at"]);
        assert_eq!(admin.exclude, vec!["internal_field"]);
        assert!(admin.save_on_top);
        assert_eq!(admin.list_editable, vec!["author"]);
        assert_eq!(admin.date_hierarchy, Some("date".to_string()));
    }

    #[test]
    fn test_model_admin_model_key() {
        let admin = ModelAdmin::new("blog", "article");
        assert_eq!(admin.model_key(), "blog.article");
    }

    #[test]
    fn test_model_admin_list_filter_fields() {
        let admin = ModelAdmin::new("blog", "article")
            .list_filter_fields(vec!["status", "author"]);
        assert_eq!(admin.list_filter.len(), 2);
        match &admin.list_filter[0] {
            ListFilter::Field(f) => assert_eq!(f, "status"),
            _ => panic!("Expected Field filter"),
        }
    }

    #[test]
    fn test_model_admin_custom_list_filter() {
        let admin = ModelAdmin::new("blog", "article")
            .list_filter(vec![
                ListFilter::Field("status".to_string()),
                ListFilter::DateHierarchy("published_date".to_string()),
                ListFilter::Custom {
                    name: "Has Image".to_string(),
                    choices: vec![
                        FilterChoice::new("Yes", "true"),
                        FilterChoice::new("No", "false"),
                    ],
                },
            ]);
        assert_eq!(admin.list_filter.len(), 3);
    }

    #[test]
    fn test_model_admin_fieldsets() {
        let admin = ModelAdmin::new("blog", "article")
            .fieldsets(vec![
                Fieldset::new(vec!["title", "slug"])
                    .name("Basic Info"),
                Fieldset::new(vec!["body", "summary"])
                    .name("Content")
                    .classes(vec!["wide"])
                    .description("Main content fields"),
                Fieldset::new(vec!["author", "published_date"])
                    .name("Metadata")
                    .classes(vec!["collapse"]),
            ]);
        assert_eq!(admin.fieldsets.len(), 3);
        assert_eq!(admin.fieldsets[0].name, Some("Basic Info".to_string()));
        assert_eq!(admin.fieldsets[1].classes, vec!["wide"]);
        assert_eq!(
            admin.fieldsets[1].description,
            Some("Main content fields".to_string())
        );
    }

    #[test]
    fn test_model_admin_inlines() {
        let admin = ModelAdmin::new("blog", "article")
            .inlines(vec![
                InlineAdmin::new("blog", "comment", InlineType::Tabular)
                    .extra(1)
                    .min_num(0)
                    .max_num(Some(10))
                    .fields(vec!["author", "text"]),
            ]);
        assert_eq!(admin.inlines.len(), 1);
        assert_eq!(admin.inlines[0].model_name, "comment");
        assert_eq!(admin.inlines[0].inline_type, InlineType::Tabular);
        assert_eq!(admin.inlines[0].extra, 1);
        assert_eq!(admin.inlines[0].max_num, Some(10));
    }

    #[test]
    fn test_model_admin_prepopulated_fields() {
        let mut prepopulated = HashMap::new();
        prepopulated.insert("slug".to_string(), vec!["title".to_string()]);
        let admin = ModelAdmin::new("blog", "article")
            .prepopulated_fields(prepopulated);
        assert_eq!(
            admin.prepopulated_fields.get("slug"),
            Some(&vec!["title".to_string()])
        );
    }

    #[test]
    fn test_fieldset_new() {
        let fs = Fieldset::new(vec!["name", "email"]);
        assert!(fs.name.is_none());
        assert_eq!(fs.fields, vec!["name", "email"]);
        assert!(fs.classes.is_empty());
        assert!(fs.description.is_none());
    }

    #[test]
    fn test_fieldset_builder() {
        let fs = Fieldset::new(vec!["a", "b"])
            .name("Test")
            .classes(vec!["wide", "collapse"])
            .description("desc");
        assert_eq!(fs.name, Some("Test".to_string()));
        assert_eq!(fs.classes, vec!["wide", "collapse"]);
        assert_eq!(fs.description, Some("desc".to_string()));
    }

    #[test]
    fn test_inline_admin_new_defaults() {
        let inline = InlineAdmin::new("blog", "comment", InlineType::Stacked);
        assert_eq!(inline.app_label, "blog");
        assert_eq!(inline.model_name, "comment");
        assert_eq!(inline.inline_type, InlineType::Stacked);
        assert_eq!(inline.extra, 3);
        assert_eq!(inline.min_num, 0);
        assert!(inline.max_num.is_none());
        assert!(inline.fields.is_empty());
    }

    #[test]
    fn test_inline_admin_builder() {
        let inline = InlineAdmin::new("blog", "tag", InlineType::Tabular)
            .extra(0)
            .min_num(1)
            .max_num(Some(5))
            .fields(vec!["name"]);
        assert_eq!(inline.extra, 0);
        assert_eq!(inline.min_num, 1);
        assert_eq!(inline.max_num, Some(5));
        assert_eq!(inline.fields, vec!["name"]);
    }

    #[test]
    fn test_inline_type_equality() {
        assert_eq!(InlineType::Tabular, InlineType::Tabular);
        assert_ne!(InlineType::Tabular, InlineType::Stacked);
    }

    #[test]
    fn test_filter_choice_new() {
        let choice = FilterChoice::new("Published", "published");
        assert_eq!(choice.display, "Published");
        assert_eq!(choice.value, "published");
    }

    #[test]
    fn test_field_schema_new() {
        let schema = FieldSchema::new("username", "CharField");
        assert_eq!(schema.name, "username");
        assert_eq!(schema.field_type, "CharField");
        assert!(schema.required);
        assert!(!schema.read_only);
        assert!(!schema.primary_key);
        assert_eq!(schema.label, "username");
    }

    #[test]
    fn test_field_schema_builder() {
        let schema = FieldSchema::new("id", "BigAutoField")
            .primary_key()
            .label("ID")
            .help_text("Primary key");
        assert!(schema.primary_key);
        assert!(schema.read_only);
        assert_eq!(schema.label, "ID");
        assert_eq!(schema.help_text, "Primary key");
    }

    #[test]
    fn test_field_schema_relation() {
        let schema = FieldSchema::new("author", "ForeignKey")
            .relation("auth.user");
        assert!(schema.is_relation);
        assert_eq!(schema.related_model, Some("auth.user".to_string()));
    }

    #[test]
    fn test_field_schema_optional() {
        let schema = FieldSchema::new("bio", "TextField").optional();
        assert!(!schema.required);
    }

    #[test]
    fn test_field_schema_max_length() {
        let schema = FieldSchema::new("name", "CharField").max_length(100);
        assert_eq!(schema.max_length, Some(100));
    }

    #[test]
    fn test_model_admin_fields_schema() {
        let admin = ModelAdmin::new("auth", "user")
            .fields_schema(vec![
                FieldSchema::new("id", "BigAutoField").primary_key(),
                FieldSchema::new("username", "CharField").max_length(150),
                FieldSchema::new("email", "EmailField").optional(),
            ]);
        assert_eq!(admin.fields_schema.len(), 3);
        assert!(admin.fields_schema[0].primary_key);
    }

    #[test]
    fn test_model_admin_verbose_name_with_underscore() {
        let admin = ModelAdmin::new("blog", "blog_post");
        assert_eq!(admin.verbose_name, "blog post");
        assert_eq!(admin.verbose_name_plural, "blog posts");
    }

    #[test]
    fn test_model_admin_serialization() {
        let admin = ModelAdmin::new("blog", "article")
            .list_display(vec!["title"])
            .list_per_page(10);
        let json = serde_json::to_string(&admin).unwrap();
        assert!(json.contains("\"app_label\":\"blog\""));
        assert!(json.contains("\"list_per_page\":10"));
    }
}
