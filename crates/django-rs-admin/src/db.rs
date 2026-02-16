//! Admin database integration.
//!
//! This module provides the [`AdminDbExecutor`] trait that bridges the admin panel
//! with the database layer. It allows admin endpoints to perform actual CRUD
//! operations against the database using the `DbExecutor` trait from `django-rs-db`.
//!
//! # Architecture
//!
//! The admin database layer works with `serde_json::Value` objects rather than
//! typed models, because the admin panel is generic over any registered model.
//! The [`AdminDbExecutor`] converts between JSON representations and SQL
//! queries, using the [`ModelAdmin`] configuration for field information.
//!
//! # Example
//!
//! ```
//! use django_rs_admin::db::{AdminDbExecutor, InMemoryAdminDb, AdminListParams};
//!
//! let db = InMemoryAdminDb::new();
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::api::JsonListResponse;
use crate::model_admin::ModelAdmin;

/// Parameters for an admin list query.
///
/// Combines pagination, search, ordering, and filtering parameters
/// into a single struct passed to [`AdminDbExecutor::list_objects`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminListParams {
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

impl AdminListParams {
    /// Creates new admin list parameters with defaults.
    pub fn new() -> Self {
        Self {
            page: 1,
            page_size: 100,
            search: None,
            ordering: None,
            filters: HashMap::new(),
        }
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

/// The result of an admin list query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminListResult {
    /// The JSON list response with pagination metadata.
    pub response: JsonListResponse,
    /// Available filter choices, keyed by field name.
    pub filter_choices: HashMap<String, Vec<String>>,
}

/// Trait for admin database operations.
///
/// Provides CRUD operations on arbitrary models using JSON values.
/// This trait abstracts over the actual database backend, allowing
/// the admin to work with any data source.
///
/// The trait uses `serde_json::Value` as the universal object representation,
/// making it generic across all registered models.
#[async_trait]
pub trait AdminDbExecutor: Send + Sync {
    /// Lists objects for a model with pagination, search, ordering, and filtering.
    ///
    /// Returns an `AdminListResult` containing the paginated results and
    /// available filter choices for the sidebar.
    async fn list_objects(
        &self,
        admin: &ModelAdmin,
        params: &AdminListParams,
    ) -> Result<AdminListResult, String>;

    /// Fetches a single object by primary key.
    async fn get_object(&self, admin: &ModelAdmin, pk: &str) -> Result<serde_json::Value, String>;

    /// Creates a new object from the given field values.
    ///
    /// Returns the created object with its generated primary key.
    async fn create_object(
        &self,
        admin: &ModelAdmin,
        data: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String>;

    /// Updates an existing object identified by primary key.
    ///
    /// Returns the updated object.
    async fn update_object(
        &self,
        admin: &ModelAdmin,
        pk: &str,
        data: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String>;

    /// Deletes an object by primary key.
    ///
    /// Returns `true` if the object was found and deleted.
    async fn delete_object(&self, admin: &ModelAdmin, pk: &str) -> Result<bool, String>;
}

/// Storage entry for a model table in the in-memory database.
#[derive(Debug, Clone)]
struct ModelTable {
    /// Objects keyed by primary key string.
    objects: Vec<serde_json::Value>,
    /// Auto-incrementing ID counter.
    next_id: u64,
}

impl ModelTable {
    const fn new() -> Self {
        Self {
            objects: Vec::new(),
            next_id: 1,
        }
    }
}

/// In-memory implementation of [`AdminDbExecutor`].
///
/// This is useful for testing and development. It stores all objects
/// in memory using `serde_json::Value` maps. Each model gets its own
/// table backed by a `Vec<serde_json::Value>`.
///
/// Thread-safe via `Arc<RwLock<...>>`.
///
/// # Example
///
/// ```
/// use django_rs_admin::db::InMemoryAdminDb;
///
/// let db = InMemoryAdminDb::new();
/// ```
#[derive(Debug, Clone)]
pub struct InMemoryAdminDb {
    /// Tables keyed by model key (e.g., "blog.article").
    tables: Arc<RwLock<HashMap<String, ModelTable>>>,
}

impl InMemoryAdminDb {
    /// Creates a new empty in-memory database.
    pub fn new() -> Self {
        Self {
            tables: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Returns the number of objects in a model's table.
    pub fn count(&self, model_key: &str) -> usize {
        let tables = self.tables.read().unwrap();
        tables.get(model_key).map_or(0, |t| t.objects.len())
    }

    /// Clears all objects from all tables.
    pub fn clear(&self) {
        let mut tables = self.tables.write().unwrap();
        tables.clear();
    }

    /// Clears all objects from a specific model's table.
    pub fn clear_table(&self, model_key: &str) {
        let mut tables = self.tables.write().unwrap();
        if let Some(table) = tables.get_mut(model_key) {
            table.objects.clear();
            table.next_id = 1;
        }
    }

    /// Returns all objects for a model.
    pub fn all_objects(&self, model_key: &str) -> Vec<serde_json::Value> {
        let tables = self.tables.read().unwrap();
        tables
            .get(model_key)
            .map_or_else(Vec::new, |t| t.objects.clone())
    }

    /// Finds the PK field name from the admin configuration.
    fn pk_field(admin: &ModelAdmin) -> String {
        admin
            .fields_schema
            .iter()
            .find(|f| f.primary_key)
            .map_or_else(|| "id".to_string(), |f| f.name.clone())
    }
}

impl Default for InMemoryAdminDb {
    fn default() -> Self {
        Self::new()
    }
}

/// Applies search filtering to a list of objects.
///
/// Matches objects where any of the search fields contain the query (case-insensitive).
fn apply_search(
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

/// Applies field-value filtering to a list of objects.
fn apply_filters(
    objects: &[serde_json::Value],
    filters: &HashMap<String, String>,
) -> Vec<serde_json::Value> {
    if filters.is_empty() {
        return objects.to_vec();
    }
    objects
        .iter()
        .filter(|obj| {
            filters.iter().all(|(field, value)| {
                obj.get(field).is_some_and(|v| match v {
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

/// Applies ordering to a list of objects.
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
                a_num
                    .partial_cmp(&b_num)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else if let (Some(a_bool), Some(b_bool)) = (a.as_bool(), b.as_bool()) {
                a_bool.cmp(&b_bool)
            } else {
                a.to_string().cmp(&b.to_string())
            }
        }
    }
}

/// Collects distinct values for filter fields from a set of objects.
fn collect_filter_choices(
    objects: &[serde_json::Value],
    filter_fields: &[String],
) -> HashMap<String, Vec<String>> {
    let mut choices: HashMap<String, Vec<String>> = HashMap::new();
    for field in filter_fields {
        let mut values: Vec<String> = Vec::new();
        for obj in objects {
            if let Some(v) = obj.get(field) {
                let s = match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                if !values.contains(&s) {
                    values.push(s);
                }
            }
        }
        values.sort();
        choices.insert(field.clone(), values);
    }
    choices
}

/// Extracts list-filter field names from a `ModelAdmin`.
fn list_filter_field_names(admin: &ModelAdmin) -> Vec<String> {
    admin
        .list_filter
        .iter()
        .map(|f| match f {
            crate::model_admin::ListFilter::Field(name)
            | crate::model_admin::ListFilter::DateHierarchy(name)
            | crate::model_admin::ListFilter::Custom { name, .. } => name.clone(),
        })
        .collect()
}

#[async_trait]
impl AdminDbExecutor for InMemoryAdminDb {
    async fn list_objects(
        &self,
        admin: &ModelAdmin,
        params: &AdminListParams,
    ) -> Result<AdminListResult, String> {
        let model_key = admin.model_key();
        let all_objects = self.all_objects(&model_key);

        // Collect filter choices from the unfiltered set
        let filter_field_names = list_filter_field_names(admin);
        let filter_choices = collect_filter_choices(&all_objects, &filter_field_names);

        // Apply filters
        let filtered = apply_filters(&all_objects, &params.filters);

        // Apply search
        let searched = if let Some(ref query) = params.search {
            apply_search(&filtered, &admin.search_fields, query)
        } else {
            filtered
        };

        // Apply ordering
        let ordering = params
            .ordering
            .as_deref()
            .or_else(|| admin.ordering.first().map(String::as_str));
        let ordered = apply_ordering(searched, ordering);

        // Paginate
        let page_size = if params.page_size > 0 {
            params.page_size
        } else {
            admin.list_per_page
        };
        let response = JsonListResponse::paginate(&ordered, params.page, page_size);

        Ok(AdminListResult {
            response,
            filter_choices,
        })
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn get_object(&self, admin: &ModelAdmin, pk: &str) -> Result<serde_json::Value, String> {
        let model_key = admin.model_key();
        let pk_field = Self::pk_field(admin);
        let tables = self.tables.read().unwrap();
        let table = tables
            .get(&model_key)
            .ok_or_else(|| format!("Model '{model_key}' has no table"))?;

        table
            .objects
            .iter()
            .find(|obj| obj.get(&pk_field).is_some_and(|v| value_matches_pk(v, pk)))
            .cloned()
            .ok_or_else(|| format!("Object with pk '{pk}' not found in '{model_key}'"))
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn create_object(
        &self,
        admin: &ModelAdmin,
        data: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let model_key = admin.model_key();
        let pk_field = Self::pk_field(admin);
        let mut tables = self.tables.write().unwrap();
        let table = tables.entry(model_key).or_insert_with(ModelTable::new);

        let mut obj = serde_json::Map::new();
        // Auto-generate PK
        let id = table.next_id;
        table.next_id += 1;
        obj.insert(pk_field, serde_json::json!(id));

        // Insert provided fields
        for (key, value) in data {
            obj.insert(key.clone(), value.clone());
        }

        let value = serde_json::Value::Object(obj);
        table.objects.push(value.clone());
        Ok(value)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn update_object(
        &self,
        admin: &ModelAdmin,
        pk: &str,
        data: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let model_key = admin.model_key();
        let pk_field = Self::pk_field(admin);
        let mut tables = self.tables.write().unwrap();
        let table = tables
            .get_mut(&model_key)
            .ok_or_else(|| format!("Model '{model_key}' has no table"))?;

        let obj = table
            .objects
            .iter_mut()
            .find(|obj| obj.get(&pk_field).is_some_and(|v| value_matches_pk(v, pk)))
            .ok_or_else(|| format!("Object with pk '{pk}' not found in '{model_key}'"))?;

        // Update fields
        if let serde_json::Value::Object(map) = obj {
            for (key, value) in data {
                map.insert(key.clone(), value.clone());
            }
        }

        Ok(obj.clone())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn delete_object(&self, admin: &ModelAdmin, pk: &str) -> Result<bool, String> {
        let model_key = admin.model_key();
        let pk_field = Self::pk_field(admin);
        let mut tables = self.tables.write().unwrap();
        let table = tables
            .get_mut(&model_key)
            .ok_or_else(|| format!("Model '{model_key}' has no table"))?;

        let original_len = table.objects.len();
        table
            .objects
            .retain(|obj| !obj.get(&pk_field).is_some_and(|v| value_matches_pk(v, pk)));

        Ok(table.objects.len() < original_len)
    }
}

/// Checks if a JSON value matches a primary key string.
///
/// Handles both numeric and string PK comparisons.
fn value_matches_pk(value: &serde_json::Value, pk: &str) -> bool {
    match value {
        serde_json::Value::Number(n) => n.to_string() == pk,
        serde_json::Value::String(s) => s == pk,
        serde_json::Value::Bool(b) => b.to_string() == pk,
        serde_json::Value::Null => pk.is_empty() || pk == "null",
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_admin::{FieldSchema, ModelAdmin};

    fn test_admin() -> ModelAdmin {
        ModelAdmin::new("blog", "article")
            .fields_schema(vec![
                FieldSchema::new("id", "BigAutoField").primary_key(),
                FieldSchema::new("title", "CharField").max_length(200),
                FieldSchema::new("body", "TextField").optional(),
                FieldSchema::new("status", "CharField").max_length(20),
            ])
            .search_fields(vec!["title", "body"])
            .list_filter_fields(vec!["status"])
            .ordering(vec!["-id"])
            .list_per_page(10)
    }

    #[test]
    fn test_admin_list_params_defaults() {
        let params = AdminListParams::new();
        assert_eq!(params.page, 1);
        assert_eq!(params.page_size, 100);
        assert!(params.search.is_none());
        assert!(params.ordering.is_none());
        assert!(params.filters.is_empty());
    }

    #[test]
    fn test_admin_list_params_builder() {
        let params = AdminListParams::new()
            .page(2)
            .page_size(25)
            .search("hello")
            .ordering("-title")
            .filter("status", "published");
        assert_eq!(params.page, 2);
        assert_eq!(params.page_size, 25);
        assert_eq!(params.search, Some("hello".to_string()));
        assert_eq!(params.ordering, Some("-title".to_string()));
        assert_eq!(params.filters.get("status"), Some(&"published".to_string()));
    }

    #[test]
    fn test_in_memory_db_new() {
        let db = InMemoryAdminDb::new();
        assert_eq!(db.count("blog.article"), 0);
    }

    #[test]
    fn test_in_memory_db_default() {
        let db = InMemoryAdminDb::default();
        assert_eq!(db.count("blog.article"), 0);
    }

    #[tokio::test]
    async fn test_create_object() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();
        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::json!("First Post"));
        data.insert("status".to_string(), serde_json::json!("published"));

        let result = db.create_object(&admin, &data).await.unwrap();
        assert_eq!(result["id"], 1);
        assert_eq!(result["title"], "First Post");
        assert_eq!(result["status"], "published");
        assert_eq!(db.count("blog.article"), 1);
    }

    #[tokio::test]
    async fn test_create_multiple_objects_auto_increment() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        let mut data1 = HashMap::new();
        data1.insert("title".to_string(), serde_json::json!("First"));
        let obj1 = db.create_object(&admin, &data1).await.unwrap();

        let mut data2 = HashMap::new();
        data2.insert("title".to_string(), serde_json::json!("Second"));
        let obj2 = db.create_object(&admin, &data2).await.unwrap();

        assert_eq!(obj1["id"], 1);
        assert_eq!(obj2["id"], 2);
        assert_eq!(db.count("blog.article"), 2);
    }

    #[tokio::test]
    async fn test_get_object() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();
        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::json!("Test Article"));
        db.create_object(&admin, &data).await.unwrap();

        let obj = db.get_object(&admin, "1").await.unwrap();
        assert_eq!(obj["title"], "Test Article");
    }

    #[tokio::test]
    async fn test_get_object_not_found() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();
        // Create a table by inserting an object first
        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::json!("Existing"));
        db.create_object(&admin, &data).await.unwrap();

        let result = db.get_object(&admin, "999").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_get_object_no_table() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();
        let result = db.get_object(&admin, "1").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no table"));
    }

    #[tokio::test]
    async fn test_update_object() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();
        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::json!("Original"));
        data.insert("status".to_string(), serde_json::json!("draft"));
        db.create_object(&admin, &data).await.unwrap();

        let mut update = HashMap::new();
        update.insert("title".to_string(), serde_json::json!("Updated"));
        update.insert("status".to_string(), serde_json::json!("published"));

        let obj = db.update_object(&admin, "1", &update).await.unwrap();
        assert_eq!(obj["title"], "Updated");
        assert_eq!(obj["status"], "published");

        // Verify persisted
        let fetched = db.get_object(&admin, "1").await.unwrap();
        assert_eq!(fetched["title"], "Updated");
    }

    #[tokio::test]
    async fn test_update_object_not_found() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();
        // Create table first
        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::json!("Test"));
        db.create_object(&admin, &data).await.unwrap();

        let update = HashMap::new();
        let result = db.update_object(&admin, "999", &update).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_object() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();
        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::json!("To Delete"));
        db.create_object(&admin, &data).await.unwrap();
        assert_eq!(db.count("blog.article"), 1);

        let deleted = db.delete_object(&admin, "1").await.unwrap();
        assert!(deleted);
        assert_eq!(db.count("blog.article"), 0);
    }

    #[tokio::test]
    async fn test_delete_object_not_found() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();
        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::json!("Existing"));
        db.create_object(&admin, &data).await.unwrap();

        let deleted = db.delete_object(&admin, "999").await.unwrap();
        assert!(!deleted);
        assert_eq!(db.count("blog.article"), 1);
    }

    #[tokio::test]
    async fn test_list_objects_empty() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();
        let params = AdminListParams::new();
        let result = db.list_objects(&admin, &params).await.unwrap();
        assert_eq!(result.response.count, 0);
        assert!(result.response.results.is_empty());
    }

    #[tokio::test]
    async fn test_list_objects_basic() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        for i in 1..=5 {
            let mut data = HashMap::new();
            data.insert(
                "title".to_string(),
                serde_json::json!(format!("Article {i}")),
            );
            data.insert("status".to_string(), serde_json::json!("published"));
            db.create_object(&admin, &data).await.unwrap();
        }

        let params = AdminListParams::new().page_size(10);
        let result = db.list_objects(&admin, &params).await.unwrap();
        assert_eq!(result.response.count, 5);
        assert_eq!(result.response.results.len(), 5);
    }

    #[tokio::test]
    async fn test_list_objects_pagination() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        for i in 1..=25 {
            let mut data = HashMap::new();
            data.insert(
                "title".to_string(),
                serde_json::json!(format!("Article {i}")),
            );
            db.create_object(&admin, &data).await.unwrap();
        }

        let params = AdminListParams::new().page(1).page_size(10);
        let result = db.list_objects(&admin, &params).await.unwrap();
        assert_eq!(result.response.count, 25);
        assert_eq!(result.response.results.len(), 10);
        assert_eq!(result.response.total_pages, 3);
        assert!(result.response.has_next);
        assert!(!result.response.has_previous);

        let params = AdminListParams::new().page(3).page_size(10);
        let result = db.list_objects(&admin, &params).await.unwrap();
        assert_eq!(result.response.results.len(), 5);
        assert!(!result.response.has_next);
        assert!(result.response.has_previous);
    }

    #[tokio::test]
    async fn test_list_objects_search() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        let mut data1 = HashMap::new();
        data1.insert("title".to_string(), serde_json::json!("Rust Guide"));
        data1.insert("body".to_string(), serde_json::json!("Learn Rust"));
        db.create_object(&admin, &data1).await.unwrap();

        let mut data2 = HashMap::new();
        data2.insert("title".to_string(), serde_json::json!("Python Guide"));
        data2.insert("body".to_string(), serde_json::json!("Learn Python"));
        db.create_object(&admin, &data2).await.unwrap();

        let mut data3 = HashMap::new();
        data3.insert("title".to_string(), serde_json::json!("Rust Tips"));
        data3.insert("body".to_string(), serde_json::json!("Advanced Rust"));
        db.create_object(&admin, &data3).await.unwrap();

        let params = AdminListParams::new().search("rust");
        let result = db.list_objects(&admin, &params).await.unwrap();
        assert_eq!(result.response.count, 2);
    }

    #[tokio::test]
    async fn test_list_objects_filter() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        let mut d1 = HashMap::new();
        d1.insert("title".to_string(), serde_json::json!("A1"));
        d1.insert("status".to_string(), serde_json::json!("published"));
        db.create_object(&admin, &d1).await.unwrap();

        let mut d2 = HashMap::new();
        d2.insert("title".to_string(), serde_json::json!("A2"));
        d2.insert("status".to_string(), serde_json::json!("draft"));
        db.create_object(&admin, &d2).await.unwrap();

        let mut d3 = HashMap::new();
        d3.insert("title".to_string(), serde_json::json!("A3"));
        d3.insert("status".to_string(), serde_json::json!("published"));
        db.create_object(&admin, &d3).await.unwrap();

        let params = AdminListParams::new().filter("status", "published");
        let result = db.list_objects(&admin, &params).await.unwrap();
        assert_eq!(result.response.count, 2);
    }

    #[tokio::test]
    async fn test_list_objects_ordering() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        let mut d1 = HashMap::new();
        d1.insert("title".to_string(), serde_json::json!("Charlie"));
        db.create_object(&admin, &d1).await.unwrap();

        let mut d2 = HashMap::new();
        d2.insert("title".to_string(), serde_json::json!("Alice"));
        db.create_object(&admin, &d2).await.unwrap();

        let mut d3 = HashMap::new();
        d3.insert("title".to_string(), serde_json::json!("Bob"));
        db.create_object(&admin, &d3).await.unwrap();

        let params = AdminListParams::new().ordering("title");
        let result = db.list_objects(&admin, &params).await.unwrap();
        assert_eq!(result.response.results[0]["title"], "Alice");
        assert_eq!(result.response.results[1]["title"], "Bob");
        assert_eq!(result.response.results[2]["title"], "Charlie");
    }

    #[tokio::test]
    async fn test_list_objects_ordering_descending() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        let mut d1 = HashMap::new();
        d1.insert("title".to_string(), serde_json::json!("Alice"));
        db.create_object(&admin, &d1).await.unwrap();

        let mut d2 = HashMap::new();
        d2.insert("title".to_string(), serde_json::json!("Charlie"));
        db.create_object(&admin, &d2).await.unwrap();

        let mut d3 = HashMap::new();
        d3.insert("title".to_string(), serde_json::json!("Bob"));
        db.create_object(&admin, &d3).await.unwrap();

        let params = AdminListParams::new().ordering("-title");
        let result = db.list_objects(&admin, &params).await.unwrap();
        assert_eq!(result.response.results[0]["title"], "Charlie");
        assert_eq!(result.response.results[1]["title"], "Bob");
        assert_eq!(result.response.results[2]["title"], "Alice");
    }

    #[tokio::test]
    async fn test_list_objects_filter_choices() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        let mut d1 = HashMap::new();
        d1.insert("title".to_string(), serde_json::json!("A1"));
        d1.insert("status".to_string(), serde_json::json!("published"));
        db.create_object(&admin, &d1).await.unwrap();

        let mut d2 = HashMap::new();
        d2.insert("title".to_string(), serde_json::json!("A2"));
        d2.insert("status".to_string(), serde_json::json!("draft"));
        db.create_object(&admin, &d2).await.unwrap();

        let params = AdminListParams::new();
        let result = db.list_objects(&admin, &params).await.unwrap();
        let status_choices = result.filter_choices.get("status").unwrap();
        assert!(status_choices.contains(&"draft".to_string()));
        assert!(status_choices.contains(&"published".to_string()));
    }

    #[tokio::test]
    async fn test_list_objects_default_ordering() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin(); // ordering: ["-id"]

        for i in 1..=3 {
            let mut data = HashMap::new();
            data.insert("title".to_string(), serde_json::json!(format!("A{i}")));
            db.create_object(&admin, &data).await.unwrap();
        }

        // No explicit ordering - should use admin default "-id"
        let params = AdminListParams::new();
        let result = db.list_objects(&admin, &params).await.unwrap();
        assert_eq!(result.response.results[0]["id"], 3);
        assert_eq!(result.response.results[1]["id"], 2);
        assert_eq!(result.response.results[2]["id"], 1);
    }

    #[tokio::test]
    async fn test_list_objects_combined_search_and_filter() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        let mut d1 = HashMap::new();
        d1.insert("title".to_string(), serde_json::json!("Rust Guide"));
        d1.insert("status".to_string(), serde_json::json!("published"));
        db.create_object(&admin, &d1).await.unwrap();

        let mut d2 = HashMap::new();
        d2.insert("title".to_string(), serde_json::json!("Rust Tips"));
        d2.insert("status".to_string(), serde_json::json!("draft"));
        db.create_object(&admin, &d2).await.unwrap();

        let mut d3 = HashMap::new();
        d3.insert("title".to_string(), serde_json::json!("Python Guide"));
        d3.insert("status".to_string(), serde_json::json!("published"));
        db.create_object(&admin, &d3).await.unwrap();

        let params = AdminListParams::new()
            .search("rust")
            .filter("status", "published");
        let result = db.list_objects(&admin, &params).await.unwrap();
        assert_eq!(result.response.count, 1);
        assert_eq!(result.response.results[0]["title"], "Rust Guide");
    }

    #[tokio::test]
    async fn test_clear_and_clear_table() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::json!("Test"));
        db.create_object(&admin, &data).await.unwrap();
        assert_eq!(db.count("blog.article"), 1);

        db.clear_table("blog.article");
        assert_eq!(db.count("blog.article"), 0);

        db.create_object(&admin, &data).await.unwrap();
        assert_eq!(db.count("blog.article"), 1);

        db.clear();
        assert_eq!(db.count("blog.article"), 0);
    }

    #[tokio::test]
    async fn test_all_objects() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        for i in 1..=3 {
            let mut data = HashMap::new();
            data.insert(
                "title".to_string(),
                serde_json::json!(format!("Article {i}")),
            );
            db.create_object(&admin, &data).await.unwrap();
        }

        let objects = db.all_objects("blog.article");
        assert_eq!(objects.len(), 3);
    }

    #[test]
    fn test_all_objects_empty_table() {
        let db = InMemoryAdminDb::new();
        let objects = db.all_objects("nonexistent.model");
        assert!(objects.is_empty());
    }

    #[test]
    fn test_value_matches_pk_number() {
        assert!(value_matches_pk(&serde_json::json!(42), "42"));
        assert!(!value_matches_pk(&serde_json::json!(42), "43"));
    }

    #[test]
    fn test_value_matches_pk_string() {
        assert!(value_matches_pk(&serde_json::json!("abc"), "abc"));
        assert!(!value_matches_pk(&serde_json::json!("abc"), "xyz"));
    }

    #[test]
    fn test_apply_search_fn() {
        let objects = vec![
            serde_json::json!({"title": "Hello World"}),
            serde_json::json!({"title": "Goodbye"}),
        ];
        let fields = vec!["title".to_string()];
        let result = apply_search(&objects, &fields, "hello");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["title"], "Hello World");
    }

    #[test]
    fn test_apply_search_empty_query() {
        let objects = vec![serde_json::json!({"title": "Test"})];
        let fields = vec!["title".to_string()];
        let result = apply_search(&objects, &fields, "");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_apply_filters_fn() {
        let objects = vec![
            serde_json::json!({"status": "active"}),
            serde_json::json!({"status": "inactive"}),
        ];
        let mut filters = HashMap::new();
        filters.insert("status".to_string(), "active".to_string());
        let result = apply_filters(&objects, &filters);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_apply_ordering_fn() {
        let objects = vec![
            serde_json::json!({"name": "C"}),
            serde_json::json!({"name": "A"}),
            serde_json::json!({"name": "B"}),
        ];
        let result = apply_ordering(objects, Some("name"));
        assert_eq!(result[0]["name"], "A");
        assert_eq!(result[1]["name"], "B");
        assert_eq!(result[2]["name"], "C");
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

    #[test]
    fn test_collect_filter_choices() {
        let objects = vec![
            serde_json::json!({"status": "active", "role": "admin"}),
            serde_json::json!({"status": "inactive", "role": "user"}),
            serde_json::json!({"status": "active", "role": "admin"}),
        ];
        let fields = vec!["status".to_string()];
        let choices = collect_filter_choices(&objects, &fields);
        let status = choices.get("status").unwrap();
        assert_eq!(status.len(), 2);
        assert!(status.contains(&"active".to_string()));
        assert!(status.contains(&"inactive".to_string()));
    }

    #[tokio::test]
    async fn test_update_preserves_pk() {
        let db = InMemoryAdminDb::new();
        let admin = test_admin();

        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::json!("Original"));
        db.create_object(&admin, &data).await.unwrap();

        let mut update = HashMap::new();
        update.insert("title".to_string(), serde_json::json!("Updated"));
        let obj = db.update_object(&admin, "1", &update).await.unwrap();

        // PK should not change
        assert_eq!(obj["id"], 1);
        assert_eq!(obj["title"], "Updated");
    }

    #[tokio::test]
    async fn test_pk_field_default() {
        let admin = ModelAdmin::new("blog", "article"); // no fields_schema
        assert_eq!(InMemoryAdminDb::pk_field(&admin), "id");
    }

    #[tokio::test]
    async fn test_pk_field_from_schema() {
        let admin = ModelAdmin::new("blog", "article").fields_schema(vec![
            FieldSchema::new("article_id", "BigAutoField").primary_key(),
            FieldSchema::new("title", "CharField"),
        ]);
        assert_eq!(InMemoryAdminDb::pk_field(&admin), "article_id");
    }

    #[test]
    fn test_compare_json_values_strings() {
        let a = serde_json::json!("alpha");
        let b = serde_json::json!("beta");
        assert_eq!(
            compare_json_values(Some(&a), Some(&b)),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn test_compare_json_values_numbers() {
        let a = serde_json::json!(10);
        let b = serde_json::json!(20);
        assert_eq!(
            compare_json_values(Some(&a), Some(&b)),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn test_compare_json_values_none() {
        assert_eq!(compare_json_values(None, None), std::cmp::Ordering::Equal);
        assert_eq!(
            compare_json_values(None, Some(&serde_json::json!("a"))),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_json_values(Some(&serde_json::json!("a")), None),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn test_admin_db_executor_is_object_safe() {
        fn _assert_object_safe(_: &dyn AdminDbExecutor) {}
    }

    #[test]
    fn test_admin_list_result_serialization() {
        let result = AdminListResult {
            response: JsonListResponse::empty(1, 10),
            filter_choices: HashMap::new(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"count\":0"));
    }
}
