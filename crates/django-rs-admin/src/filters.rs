//! List filter types and filter specification for the admin panel.
//!
//! This module provides the [`FilterSpec`] type used to describe the current
//! state of a filter in the admin list view, including available choices
//! and the currently selected value.

use serde::{Deserialize, Serialize};

use crate::model_admin::FilterChoice;

/// A resolved filter specification with its available choices and current selection.
///
/// This is generated at runtime based on the registered [`ListFilter`](crate::model_admin::ListFilter)
/// and the current query parameters. The React frontend uses this to render
/// filter sidebar controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSpec {
    /// The field name or filter identifier.
    pub field: String,
    /// Human-readable title for this filter.
    pub title: String,
    /// Available filter choices.
    pub choices: Vec<FilterChoice>,
    /// The currently selected value, if any.
    pub selected: Option<String>,
}

impl FilterSpec {
    /// Creates a new filter specification.
    pub fn new(field: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            title: title.into(),
            choices: Vec::new(),
            selected: None,
        }
    }

    /// Adds a choice to this filter.
    #[must_use]
    pub fn add_choice(mut self, choice: FilterChoice) -> Self {
        self.choices.push(choice);
        self
    }

    /// Sets the currently selected value.
    #[must_use]
    pub fn selected(mut self, value: impl Into<String>) -> Self {
        self.selected = Some(value.into());
        self
    }

    /// Creates a boolean filter specification with "Yes"/"No" choices.
    pub fn boolean(field: impl Into<String>, title: impl Into<String>) -> Self {
        Self::new(field, title)
            .add_choice(FilterChoice::new("All", ""))
            .add_choice(FilterChoice::new("Yes", "true"))
            .add_choice(FilterChoice::new("No", "false"))
    }
}

/// Applies filter parameters to a set of serialized objects.
///
/// This function filters a slice of JSON values based on the given field-value
/// filter map. Objects are included if all filter conditions match.
pub fn apply_filters<S: ::std::hash::BuildHasher>(
    objects: &[serde_json::Value],
    filters: &std::collections::HashMap<String, String, S>,
) -> Vec<serde_json::Value> {
    if filters.is_empty() {
        return objects.to_vec();
    }

    objects
        .iter()
        .filter(|obj| {
            filters.iter().all(|(field, value)| {
                obj.get(field)
                    .is_some_and(|v| match v {
                        serde_json::Value::String(s) => s == value,
                        serde_json::Value::Number(n) => n.to_string() == *value,
                        serde_json::Value::Bool(b) => b.to_string() == *value,
                        serde_json::Value::Null => value.is_empty() || value == "null",
                        _ => false,
                    })
            })
        })
        .cloned()
        .collect()
}

/// Applies a search query across the specified fields of serialized objects.
///
/// Objects are included if any of the search fields contain the query string
/// (case-insensitive).
pub fn apply_search(
    objects: &[serde_json::Value],
    search_fields: &[String],
    query: &str,
) -> Vec<serde_json::Value> {
    if query.is_empty() || search_fields.is_empty() {
        return objects.to_vec();
    }

    let query_lower = query.to_lowercase();

    objects
        .iter()
        .filter(|obj| {
            search_fields.iter().any(|field| {
                obj.get(field)
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s.to_lowercase().contains(&query_lower))
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_filter_spec_new() {
        let spec = FilterSpec::new("status", "Status");
        assert_eq!(spec.field, "status");
        assert_eq!(spec.title, "Status");
        assert!(spec.choices.is_empty());
        assert!(spec.selected.is_none());
    }

    #[test]
    fn test_filter_spec_add_choice() {
        let spec = FilterSpec::new("status", "Status")
            .add_choice(FilterChoice::new("Active", "active"))
            .add_choice(FilterChoice::new("Inactive", "inactive"));
        assert_eq!(spec.choices.len(), 2);
        assert_eq!(spec.choices[0].display, "Active");
        assert_eq!(spec.choices[1].value, "inactive");
    }

    #[test]
    fn test_filter_spec_selected() {
        let spec = FilterSpec::new("status", "Status")
            .selected("active");
        assert_eq!(spec.selected, Some("active".to_string()));
    }

    #[test]
    fn test_filter_spec_boolean() {
        let spec = FilterSpec::boolean("is_active", "Active");
        assert_eq!(spec.choices.len(), 3);
        assert_eq!(spec.choices[0].display, "All");
        assert_eq!(spec.choices[1].display, "Yes");
        assert_eq!(spec.choices[1].value, "true");
        assert_eq!(spec.choices[2].display, "No");
        assert_eq!(spec.choices[2].value, "false");
    }

    #[test]
    fn test_apply_filters_empty() {
        let objects = vec![
            serde_json::json!({"name": "Alice", "status": "active"}),
            serde_json::json!({"name": "Bob", "status": "inactive"}),
        ];
        let filters = HashMap::new();
        let result = apply_filters(&objects, &filters);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_apply_filters_string_match() {
        let objects = vec![
            serde_json::json!({"name": "Alice", "status": "active"}),
            serde_json::json!({"name": "Bob", "status": "inactive"}),
            serde_json::json!({"name": "Charlie", "status": "active"}),
        ];
        let mut filters = HashMap::new();
        filters.insert("status".to_string(), "active".to_string());
        let result = apply_filters(&objects, &filters);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_apply_filters_number_match() {
        let objects = vec![
            serde_json::json!({"name": "Alice", "age": 30}),
            serde_json::json!({"name": "Bob", "age": 25}),
        ];
        let mut filters = HashMap::new();
        filters.insert("age".to_string(), "30".to_string());
        let result = apply_filters(&objects, &filters);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_apply_filters_bool_match() {
        let objects = vec![
            serde_json::json!({"name": "Alice", "active": true}),
            serde_json::json!({"name": "Bob", "active": false}),
        ];
        let mut filters = HashMap::new();
        filters.insert("active".to_string(), "true".to_string());
        let result = apply_filters(&objects, &filters);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_apply_filters_multiple_conditions() {
        let objects = vec![
            serde_json::json!({"name": "Alice", "status": "active", "role": "admin"}),
            serde_json::json!({"name": "Bob", "status": "active", "role": "user"}),
            serde_json::json!({"name": "Charlie", "status": "inactive", "role": "admin"}),
        ];
        let mut filters = HashMap::new();
        filters.insert("status".to_string(), "active".to_string());
        filters.insert("role".to_string(), "admin".to_string());
        let result = apply_filters(&objects, &filters);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "Alice");
    }

    #[test]
    fn test_apply_filters_no_match() {
        let objects = vec![
            serde_json::json!({"name": "Alice", "status": "active"}),
        ];
        let mut filters = HashMap::new();
        filters.insert("status".to_string(), "deleted".to_string());
        let result = apply_filters(&objects, &filters);
        assert!(result.is_empty());
    }

    #[test]
    fn test_apply_search_empty_query() {
        let objects = vec![
            serde_json::json!({"title": "Hello World"}),
        ];
        let result = apply_search(&objects, &["title".to_string()], "");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_apply_search_empty_fields() {
        let objects = vec![
            serde_json::json!({"title": "Hello World"}),
        ];
        let result = apply_search(&objects, &[], "hello");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_apply_search_case_insensitive() {
        let objects = vec![
            serde_json::json!({"title": "Hello World", "body": "Some text"}),
            serde_json::json!({"title": "Goodbye", "body": "Other text"}),
        ];
        let fields = vec!["title".to_string()];
        let result = apply_search(&objects, &fields, "hello");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["title"], "Hello World");
    }

    #[test]
    fn test_apply_search_multiple_fields() {
        let objects = vec![
            serde_json::json!({"title": "Rust Guide", "body": "Learn Rust programming"}),
            serde_json::json!({"title": "Python Guide", "body": "Learn Python programming"}),
            serde_json::json!({"title": "Other", "body": "Nothing relevant"}),
        ];
        let fields = vec!["title".to_string(), "body".to_string()];
        let result = apply_search(&objects, &fields, "rust");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_apply_search_no_match() {
        let objects = vec![
            serde_json::json!({"title": "Hello"}),
        ];
        let fields = vec!["title".to_string()];
        let result = apply_search(&objects, &fields, "xyz");
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_spec_serialization() {
        let spec = FilterSpec::new("status", "Status")
            .add_choice(FilterChoice::new("Active", "active"))
            .selected("active");
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"field\":\"status\""));
        assert!(json.contains("\"selected\":\"active\""));
    }
}
