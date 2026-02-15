//! REST API endpoints for the admin panel.
//!
//! This module provides the JSON API that the React admin dashboard consumes.
//! All endpoints are async and designed for concurrent operation. The API
//! covers model listing, schema introspection, CRUD operations, bulk actions,
//! and authentication.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::filters::{apply_filters, apply_search};
use crate::model_admin::{FieldSchema, ModelAdmin};

/// Query parameters for the list endpoint.
///
/// Supports pagination, searching, ordering, and field-based filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListParams {
    /// The page number (1-indexed).
    pub page: usize,
    /// The number of items per page.
    pub page_size: usize,
    /// Optional search query applied across `search_fields`.
    pub search: Option<String>,
    /// Optional ordering field (prefix with "-" for descending).
    pub ordering: Option<String>,
    /// Field-value filters to apply.
    pub filters: HashMap<String, String>,
}

impl Default for ListParams {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 100,
            search: None,
            ordering: None,
            filters: HashMap::new(),
        }
    }
}

impl ListParams {
    /// Creates default list parameters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the page number.
    #[must_use]
    pub const fn page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    /// Sets the page size.
    #[must_use]
    pub const fn page_size(mut self, size: usize) -> Self {
        self.page_size = size;
        self
    }

    /// Sets the search query.
    #[must_use]
    pub fn search(mut self, query: impl Into<String>) -> Self {
        self.search = Some(query.into());
        self
    }

    /// Sets the ordering field.
    #[must_use]
    pub fn ordering(mut self, field: impl Into<String>) -> Self {
        self.ordering = Some(field.into());
        self
    }

    /// Adds a filter.
    #[must_use]
    pub fn filter(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.filters.insert(field.into(), value.into());
        self
    }
}

/// A paginated JSON response for list views.
///
/// Contains the result set along with pagination metadata that the React
/// frontend uses to render page controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonListResponse {
    /// The model objects for the current page.
    pub results: Vec<serde_json::Value>,
    /// Total number of matching objects (across all pages).
    pub count: usize,
    /// The current page number (1-indexed).
    pub page: usize,
    /// The number of items per page.
    pub page_size: usize,
    /// Total number of pages.
    pub total_pages: usize,
    /// Whether there is a next page.
    pub has_next: bool,
    /// Whether there is a previous page.
    pub has_previous: bool,
}

impl JsonListResponse {
    /// Creates a paginated response from a full result set and pagination parameters.
    ///
    /// This method handles computing pagination metadata and slicing the results
    /// to the requested page.
    pub fn paginate(
        all_results: &[serde_json::Value],
        page: usize,
        page_size: usize,
    ) -> Self {
        let count = all_results.len();
        let page_size = if page_size == 0 { 1 } else { page_size };
        let total_pages = count.div_ceil(page_size).max(1);
        let page = page.clamp(1, total_pages);

        let start = (page - 1) * page_size;
        let end = (start + page_size).min(count);
        let results = if start < count {
            all_results[start..end].to_vec()
        } else {
            Vec::new()
        };

        Self {
            results,
            count,
            page,
            page_size,
            total_pages,
            has_next: page < total_pages,
            has_previous: page > 1,
        }
    }

    /// Creates an empty paginated response.
    pub fn empty(page: usize, page_size: usize) -> Self {
        Self {
            results: Vec::new(),
            count: 0,
            page: page.max(1),
            page_size: if page_size == 0 { 1 } else { page_size },
            total_pages: 1,
            has_next: false,
            has_previous: false,
        }
    }
}

/// Response for the model list/index endpoint.
///
/// Lists all registered models with their app labels, names, and admin URLs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelIndexResponse {
    /// The registered models grouped by app label.
    pub apps: Vec<AppModels>,
}

/// Models grouped under an application label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppModels {
    /// The application label.
    pub app_label: String,
    /// The models registered under this app.
    pub models: Vec<ModelInfo>,
}

/// Summary information about a registered model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// The model name.
    pub name: String,
    /// The human-readable name.
    pub verbose_name: String,
    /// The plural human-readable name.
    pub verbose_name_plural: String,
    /// The API URL for this model's list view.
    pub url: String,
}

/// Schema response for a model, used by the React frontend for form rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSchemaResponse {
    /// The application label.
    pub app_label: String,
    /// The model name.
    pub model_name: String,
    /// Human-readable name.
    pub verbose_name: String,
    /// Plural human-readable name.
    pub verbose_name_plural: String,
    /// Field schema definitions.
    pub fields: Vec<FieldSchema>,
    /// Fields displayed in the list view.
    pub list_display: Vec<String>,
    /// Fields that are searchable.
    pub search_fields: Vec<String>,
    /// Default ordering.
    pub ordering: Vec<String>,
    /// Available action names.
    pub actions: Vec<String>,
    /// Number of items per page.
    pub list_per_page: usize,
}

impl ModelSchemaResponse {
    /// Creates a schema response from a `ModelAdmin`.
    pub fn from_model_admin(admin: &ModelAdmin) -> Self {
        Self {
            app_label: admin.app_label.clone(),
            model_name: admin.model_name.clone(),
            verbose_name: admin.verbose_name.clone(),
            verbose_name_plural: admin.verbose_name_plural.clone(),
            fields: admin.fields_schema.clone(),
            list_display: admin.list_display.clone(),
            search_fields: admin.search_fields.clone(),
            ordering: admin.ordering.clone(),
            actions: admin.action_names.clone(),
            list_per_page: admin.list_per_page,
        }
    }
}

/// Current user info response for the `/api/admin/me/` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUserResponse {
    /// The username.
    pub username: String,
    /// The user's email.
    pub email: String,
    /// Whether the user is a staff member.
    pub is_staff: bool,
    /// Whether the user is a superuser.
    pub is_superuser: bool,
    /// The user's full name.
    pub full_name: String,
}

/// Processes a list request: applies filtering, searching, and pagination.
///
/// This is the core async function that powers the list API endpoint. It applies
/// filters and search concurrently where possible. The function is async because
/// in a production implementation it will perform concurrent database queries.
#[allow(clippy::unused_async)]
pub async fn process_list_request(
    admin: &ModelAdmin,
    objects: Vec<serde_json::Value>,
    params: &ListParams,
) -> JsonListResponse {
    // Apply filters
    let filtered = apply_filters(&objects, &params.filters);

    // Apply search
    let searched = if let Some(ref query) = params.search {
        apply_search(&filtered, &admin.search_fields, query)
    } else {
        filtered
    };

    // Apply ordering
    let ordered = apply_ordering(searched, params.ordering.as_deref());

    // Paginate
    let page_size = if params.page_size > 0 {
        params.page_size
    } else {
        admin.list_per_page
    };

    JsonListResponse::paginate(&ordered, params.page, page_size)
}

/// Applies ordering to a list of JSON objects.
///
/// Supports ascending and descending order. Prefix the field name with "-" for descending.
fn apply_ordering(
    mut objects: Vec<serde_json::Value>,
    ordering: Option<&str>,
) -> Vec<serde_json::Value> {
    let Some(ordering) = ordering else {
        return objects;
    };

    let (field, descending) = ordering
        .strip_prefix('-')
        .map_or((ordering, false), |stripped| (stripped, true));

    objects.sort_by(|a, b| {
        let va = a.get(field);
        let vb = b.get(field);
        let cmp = compare_json_values(va, vb);
        if descending {
            cmp.reverse()
        } else {
            cmp
        }
    });

    objects
}

/// Compares two optional JSON values for ordering.
fn compare_json_values(
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(a), Some(b)) => {
            if let (Some(a_str), Some(b_str)) = (a.as_str(), b.as_str()) {
                a_str.cmp(b_str)
            } else if let (Some(a_num), Some(b_num)) = (a.as_f64(), b.as_f64()) {
                a_num.partial_cmp(&b_num).unwrap_or(std::cmp::Ordering::Equal)
            } else if let (Some(a_bool), Some(b_bool)) = (a.as_bool(), b.as_bool()) {
                a_bool.cmp(&b_bool)
            } else {
                a.to_string().cmp(&b.to_string())
            }
        }
    }
}

/// Builds the model index response from registered model admins.
pub fn build_model_index(admins: &[&ModelAdmin], url_prefix: &str) -> ModelIndexResponse {
    let mut apps_map: HashMap<String, Vec<ModelInfo>> = HashMap::new();

    for admin in admins {
        let info = ModelInfo {
            name: admin.model_name.clone(),
            verbose_name: admin.verbose_name.clone(),
            verbose_name_plural: admin.verbose_name_plural.clone(),
            url: format!("{}/{}/{}/", url_prefix, admin.app_label, admin.model_name),
        };
        apps_map
            .entry(admin.app_label.clone())
            .or_default()
            .push(info);
    }

    let mut apps: Vec<AppModels> = apps_map
        .into_iter()
        .map(|(app_label, models)| AppModels { app_label, models })
        .collect();
    apps.sort_by(|a, b| a.app_label.cmp(&b.app_label));

    ModelIndexResponse { apps }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_params_default() {
        let params = ListParams::default();
        assert_eq!(params.page, 1);
        assert_eq!(params.page_size, 100);
        assert!(params.search.is_none());
        assert!(params.ordering.is_none());
        assert!(params.filters.is_empty());
    }

    #[test]
    fn test_list_params_builder() {
        let params = ListParams::new()
            .page(2)
            .page_size(25)
            .search("hello")
            .ordering("-name")
            .filter("status", "active");
        assert_eq!(params.page, 2);
        assert_eq!(params.page_size, 25);
        assert_eq!(params.search, Some("hello".to_string()));
        assert_eq!(params.ordering, Some("-name".to_string()));
        assert_eq!(params.filters.get("status"), Some(&"active".to_string()));
    }

    #[test]
    fn test_json_list_response_paginate_basic() {
        let items: Vec<serde_json::Value> = (1..=25)
            .map(|i| serde_json::json!({"id": i}))
            .collect();
        let response = JsonListResponse::paginate(&items, 1, 10);
        assert_eq!(response.count, 25);
        assert_eq!(response.page, 1);
        assert_eq!(response.page_size, 10);
        assert_eq!(response.total_pages, 3);
        assert_eq!(response.results.len(), 10);
        assert!(response.has_next);
        assert!(!response.has_previous);
    }

    #[test]
    fn test_json_list_response_paginate_middle_page() {
        let items: Vec<serde_json::Value> = (1..=25)
            .map(|i| serde_json::json!({"id": i}))
            .collect();
        let response = JsonListResponse::paginate(&items, 2, 10);
        assert_eq!(response.page, 2);
        assert_eq!(response.results.len(), 10);
        assert!(response.has_next);
        assert!(response.has_previous);
    }

    #[test]
    fn test_json_list_response_paginate_last_page() {
        let items: Vec<serde_json::Value> = (1..=25)
            .map(|i| serde_json::json!({"id": i}))
            .collect();
        let response = JsonListResponse::paginate(&items, 3, 10);
        assert_eq!(response.page, 3);
        assert_eq!(response.results.len(), 5);
        assert!(!response.has_next);
        assert!(response.has_previous);
    }

    #[test]
    fn test_json_list_response_paginate_single_page() {
        let items: Vec<serde_json::Value> = (1..=5)
            .map(|i| serde_json::json!({"id": i}))
            .collect();
        let response = JsonListResponse::paginate(&items, 1, 10);
        assert_eq!(response.count, 5);
        assert_eq!(response.total_pages, 1);
        assert_eq!(response.results.len(), 5);
        assert!(!response.has_next);
        assert!(!response.has_previous);
    }

    #[test]
    fn test_json_list_response_paginate_empty() {
        let items: Vec<serde_json::Value> = Vec::new();
        let response = JsonListResponse::paginate(&items, 1, 10);
        assert_eq!(response.count, 0);
        assert_eq!(response.total_pages, 1);
        assert!(response.results.is_empty());
        assert!(!response.has_next);
        assert!(!response.has_previous);
    }

    #[test]
    fn test_json_list_response_paginate_page_beyond_range() {
        let items: Vec<serde_json::Value> = (1..=5)
            .map(|i| serde_json::json!({"id": i}))
            .collect();
        let response = JsonListResponse::paginate(&items, 100, 10);
        // Should clamp to last page
        assert_eq!(response.page, 1);
    }

    #[test]
    fn test_json_list_response_paginate_zero_page_size() {
        let items: Vec<serde_json::Value> = (1..=3)
            .map(|i| serde_json::json!({"id": i}))
            .collect();
        let response = JsonListResponse::paginate(&items, 1, 0);
        assert_eq!(response.page_size, 1);
    }

    #[test]
    fn test_json_list_response_empty() {
        let response = JsonListResponse::empty(1, 10);
        assert!(response.results.is_empty());
        assert_eq!(response.count, 0);
        assert_eq!(response.page, 1);
        assert_eq!(response.total_pages, 1);
    }

    #[test]
    fn test_json_list_response_serialization() {
        let items = vec![serde_json::json!({"id": 1, "name": "Test"})];
        let response = JsonListResponse::paginate(
            &items,
            1,
            10,
        );
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"count\":1"));
        assert!(json.contains("\"page\":1"));
        assert!(json.contains("\"total_pages\":1"));
        assert!(json.contains("\"has_next\":false"));
        assert!(json.contains("\"has_previous\":false"));
    }

    #[test]
    fn test_model_schema_response() {
        let admin = ModelAdmin::new("blog", "article")
            .list_display(vec!["title", "author"])
            .search_fields(vec!["title"])
            .ordering(vec!["-created"])
            .fields_schema(vec![
                FieldSchema::new("id", "BigAutoField").primary_key(),
                FieldSchema::new("title", "CharField").max_length(200),
            ]);
        let schema = ModelSchemaResponse::from_model_admin(&admin);
        assert_eq!(schema.app_label, "blog");
        assert_eq!(schema.model_name, "article");
        assert_eq!(schema.fields.len(), 2);
        assert_eq!(schema.list_display, vec!["title", "author"]);
        assert_eq!(schema.search_fields, vec!["title"]);
    }

    #[test]
    fn test_model_index_response() {
        let blog_article = ModelAdmin::new("blog", "article");
        let blog_comment = ModelAdmin::new("blog", "comment");
        let auth_user = ModelAdmin::new("auth", "user");

        let admins: Vec<&ModelAdmin> = vec![&blog_article, &blog_comment, &auth_user];
        let index = build_model_index(&admins, "/api/admin");

        assert_eq!(index.apps.len(), 2);
        // Sorted by app_label
        assert_eq!(index.apps[0].app_label, "auth");
        assert_eq!(index.apps[0].models.len(), 1);
        assert_eq!(index.apps[1].app_label, "blog");
        assert_eq!(index.apps[1].models.len(), 2);
    }

    #[test]
    fn test_model_info_url() {
        let admin = ModelAdmin::new("blog", "article");
        let admins: Vec<&ModelAdmin> = vec![&admin];
        let index = build_model_index(&admins, "/api/admin");
        assert_eq!(
            index.apps[0].models[0].url,
            "/api/admin/blog/article/"
        );
    }

    #[test]
    fn test_current_user_response() {
        let user_resp = CurrentUserResponse {
            username: "admin".to_string(),
            email: "admin@example.com".to_string(),
            is_staff: true,
            is_superuser: true,
            full_name: "Admin User".to_string(),
        };
        let json = serde_json::to_string(&user_resp).unwrap();
        assert!(json.contains("\"username\":\"admin\""));
        assert!(json.contains("\"is_superuser\":true"));
    }

    #[test]
    fn test_apply_ordering_ascending() {
        let objects = vec![
            serde_json::json!({"name": "Charlie"}),
            serde_json::json!({"name": "Alice"}),
            serde_json::json!({"name": "Bob"}),
        ];
        let result = apply_ordering(objects, Some("name"));
        assert_eq!(result[0]["name"], "Alice");
        assert_eq!(result[1]["name"], "Bob");
        assert_eq!(result[2]["name"], "Charlie");
    }

    #[test]
    fn test_apply_ordering_descending() {
        let objects = vec![
            serde_json::json!({"name": "Alice"}),
            serde_json::json!({"name": "Charlie"}),
            serde_json::json!({"name": "Bob"}),
        ];
        let result = apply_ordering(objects, Some("-name"));
        assert_eq!(result[0]["name"], "Charlie");
        assert_eq!(result[1]["name"], "Bob");
        assert_eq!(result[2]["name"], "Alice");
    }

    #[test]
    fn test_apply_ordering_numeric() {
        let objects = vec![
            serde_json::json!({"age": 30}),
            serde_json::json!({"age": 20}),
            serde_json::json!({"age": 25}),
        ];
        let result = apply_ordering(objects, Some("age"));
        assert_eq!(result[0]["age"], 20);
        assert_eq!(result[1]["age"], 25);
        assert_eq!(result[2]["age"], 30);
    }

    #[test]
    fn test_apply_ordering_none() {
        let objects = vec![
            serde_json::json!({"name": "B"}),
            serde_json::json!({"name": "A"}),
        ];
        let result = apply_ordering(objects.clone(), None);
        assert_eq!(result, objects);
    }

    #[tokio::test]
    async fn test_process_list_request_basic() {
        let admin = ModelAdmin::new("blog", "article")
            .search_fields(vec!["title"]);
        let objects: Vec<serde_json::Value> = (1..=30)
            .map(|i| serde_json::json!({"id": i, "title": format!("Article {i}")}))
            .collect();
        let params = ListParams::new().page(1).page_size(10);
        let response = process_list_request(&admin, objects, &params).await;
        assert_eq!(response.count, 30);
        assert_eq!(response.results.len(), 10);
        assert_eq!(response.total_pages, 3);
    }

    #[tokio::test]
    async fn test_process_list_request_with_search() {
        let admin = ModelAdmin::new("blog", "article")
            .search_fields(vec!["title"]);
        let objects = vec![
            serde_json::json!({"id": 1, "title": "Rust Guide"}),
            serde_json::json!({"id": 2, "title": "Python Guide"}),
            serde_json::json!({"id": 3, "title": "Rust Tips"}),
        ];
        let params = ListParams::new().search("rust");
        let response = process_list_request(&admin, objects, &params).await;
        assert_eq!(response.count, 2);
    }

    #[tokio::test]
    async fn test_process_list_request_with_filter() {
        let admin = ModelAdmin::new("blog", "article");
        let objects = vec![
            serde_json::json!({"id": 1, "status": "published"}),
            serde_json::json!({"id": 2, "status": "draft"}),
            serde_json::json!({"id": 3, "status": "published"}),
        ];
        let params = ListParams::new().filter("status", "published");
        let response = process_list_request(&admin, objects, &params).await;
        assert_eq!(response.count, 2);
    }

    #[tokio::test]
    async fn test_process_list_request_with_ordering() {
        let admin = ModelAdmin::new("blog", "article");
        let objects = vec![
            serde_json::json!({"id": 3, "title": "C"}),
            serde_json::json!({"id": 1, "title": "A"}),
            serde_json::json!({"id": 2, "title": "B"}),
        ];
        let params = ListParams::new().ordering("title");
        let response = process_list_request(&admin, objects, &params).await;
        assert_eq!(response.results[0]["title"], "A");
        assert_eq!(response.results[1]["title"], "B");
        assert_eq!(response.results[2]["title"], "C");
    }

    #[test]
    fn test_pagination_math_exact_division() {
        let items: Vec<serde_json::Value> = (1..=20)
            .map(|i| serde_json::json!({"id": i}))
            .collect();
        let response = JsonListResponse::paginate(&items, 2, 10);
        assert_eq!(response.total_pages, 2);
        assert_eq!(response.results.len(), 10);
        assert!(!response.has_next);
        assert!(response.has_previous);
    }

    #[test]
    fn test_pagination_math_one_item() {
        let items = vec![serde_json::json!({"id": 1})];
        let response = JsonListResponse::paginate(&items, 1, 10);
        assert_eq!(response.total_pages, 1);
        assert_eq!(response.results.len(), 1);
        assert!(!response.has_next);
        assert!(!response.has_previous);
    }

    #[test]
    fn test_pagination_math_page_zero() {
        let items: Vec<serde_json::Value> = (1..=5)
            .map(|i| serde_json::json!({"id": i}))
            .collect();
        // Page 0 should clamp to 1
        let response = JsonListResponse::paginate(&items, 0, 10);
        assert_eq!(response.page, 1);
    }
}
