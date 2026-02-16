//! Admin site registry and router generation.
//!
//! The [`AdminSite`] is the central registry where models are registered with
//! their [`ModelAdmin`] configurations. It generates an Axum router with all
//! the REST API endpoints that the React admin frontend consumes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use crate::actions::ActionRegistry;
use crate::api::{
    build_model_index, CurrentUserResponse, LoginRequest, LoginResponse, ModelSchemaResponse,
};
use crate::db::{AdminDbExecutor, AdminListParams, InMemoryAdminDb};
use crate::log_entry::{InMemoryLogEntryStore, LogEntryStore};
use crate::model_admin::ModelAdmin;

/// The admin site, responsible for model registration and route generation.
///
/// This is the equivalent of Django's `AdminSite`. It holds all registered
/// models, their admin configurations, and produces an Axum router that
/// serves the REST API for the React admin dashboard.
///
/// # Examples
///
/// ```
/// use django_rs_admin::site::AdminSite;
/// use django_rs_admin::model_admin::ModelAdmin;
///
/// let mut site = AdminSite::new("admin");
/// site.register("blog.article", ModelAdmin::new("blog", "article"));
/// let router = site.into_axum_router();
/// ```
pub struct AdminSite {
    /// The site name.
    name: String,
    /// The URL prefix for all admin API routes.
    url_prefix: String,
    /// Registered model admin configurations, keyed by `"app.model"`.
    registered_models: HashMap<String, ModelAdmin>,
    /// Optional directory for React build static assets.
    static_dir: Option<PathBuf>,
    /// Action registries per model key.
    action_registries: HashMap<String, ActionRegistry>,
    /// Optional database executor for CRUD operations.
    db: Option<Arc<dyn AdminDbExecutor>>,
    /// Optional log entry store for audit trail.
    log_store: Option<Arc<dyn LogEntryStore>>,
}

impl AdminSite {
    /// Creates a new admin site with the given name.
    ///
    /// The URL prefix defaults to `/api/admin`.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            url_prefix: "/api/admin".to_string(),
            registered_models: HashMap::new(),
            static_dir: None,
            action_registries: HashMap::new(),
            db: None,
            log_store: None,
        }
    }

    /// Sets the URL prefix for admin API routes.
    #[must_use]
    pub fn url_prefix(mut self, prefix: &str) -> Self {
        self.url_prefix = prefix.to_string();
        self
    }

    /// Sets the directory for React build static assets.
    #[must_use]
    pub fn static_dir(mut self, dir: PathBuf) -> Self {
        self.static_dir = Some(dir);
        self
    }

    /// Sets the database executor for CRUD operations.
    #[must_use]
    pub fn db(mut self, db: Arc<dyn AdminDbExecutor>) -> Self {
        self.db = Some(db);
        self
    }

    /// Sets the log entry store for the audit trail.
    #[must_use]
    pub fn log_store(mut self, store: Arc<dyn LogEntryStore>) -> Self {
        self.log_store = Some(store);
        self
    }

    /// Returns the site name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the URL prefix.
    pub fn url_prefix_str(&self) -> &str {
        &self.url_prefix
    }

    /// Returns the static directory, if set.
    pub const fn static_dir_path(&self) -> Option<&PathBuf> {
        self.static_dir.as_ref()
    }

    /// Registers a model with its admin configuration.
    ///
    /// The `model_key` should be in `"app_label.model_name"` format.
    pub fn register(&mut self, model_key: &str, admin: ModelAdmin) {
        self.registered_models.insert(model_key.to_string(), admin);
        self.action_registries
            .insert(model_key.to_string(), ActionRegistry::new());
    }

    /// Unregisters a model from the admin site.
    pub fn unregister(&mut self, model_key: &str) {
        self.registered_models.remove(model_key);
        self.action_registries.remove(model_key);
    }

    /// Returns the `ModelAdmin` for a registered model, if any.
    pub fn get_model_admin(&self, model_key: &str) -> Option<&ModelAdmin> {
        self.registered_models.get(model_key)
    }

    /// Returns the action registry for a registered model, if any.
    pub fn get_action_registry(&self, model_key: &str) -> Option<&ActionRegistry> {
        self.action_registries.get(model_key)
    }

    /// Returns the mutable action registry for a registered model.
    pub fn get_action_registry_mut(&mut self, model_key: &str) -> Option<&mut ActionRegistry> {
        self.action_registries.get_mut(model_key)
    }

    /// Returns a list of all registered model keys.
    pub fn registered_models(&self) -> Vec<&str> {
        self.registered_models.keys().map(String::as_str).collect()
    }

    /// Returns the number of registered models.
    pub fn model_count(&self) -> usize {
        self.registered_models.len()
    }

    /// Returns whether a model is registered.
    pub fn is_registered(&self, model_key: &str) -> bool {
        self.registered_models.contains_key(model_key)
    }

    /// Generates the Axum router with all admin API endpoints.
    ///
    /// The generated routes are:
    ///
    /// - `POST /login/` - Authenticate and get token
    /// - `POST /logout/` - Invalidate session
    /// - `GET /` - List all registered models
    /// - `GET /me/` - Current user info
    /// - `GET /log/` - Recent log entries
    /// - `GET /log/:ct/:id/` - Log entries for a specific object
    /// - `GET /:app/:model/schema` - Model schema/introspection
    /// - `GET /:app/:model/` - List objects (paginated)
    /// - `POST /:app/:model/` - Create a new object
    /// - `GET /:app/:model/:pk/` - Get single object
    /// - `PUT /:app/:model/:pk/` - Update an object
    /// - `DELETE /:app/:model/:pk/` - Delete an object
    /// - `POST /:app/:model/action/` - Execute bulk action
    pub fn into_axum_router(self) -> Router {
        let db: Arc<dyn AdminDbExecutor> =
            self.db.unwrap_or_else(|| Arc::new(InMemoryAdminDb::new()));
        let log_store: Arc<dyn LogEntryStore> = self
            .log_store
            .unwrap_or_else(|| Arc::new(InMemoryLogEntryStore::new()));

        let shared = Arc::new(AdminSiteState {
            registered_models: self.registered_models,
            url_prefix: self.url_prefix,
            name: self.name,
            db,
            log_store,
        });

        Router::new()
            .route("/login/", post(handle_login))
            .route("/logout/", post(handle_logout))
            .route("/", get(handle_index))
            .route("/me/", get(handle_me))
            .route("/log/", get(handle_log_recent))
            .route("/log/{ct}/{id}/", get(handle_log_object))
            .route("/{app}/{model}/schema", get(handle_schema))
            .route("/{app}/{model}/", get(handle_list).post(handle_create))
            .route(
                "/{app}/{model}/{pk}/",
                get(handle_detail).put(handle_update).delete(handle_delete),
            )
            .with_state(shared)
    }
}

impl std::fmt::Debug for AdminSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdminSite")
            .field("name", &self.name)
            .field("url_prefix", &self.url_prefix)
            .field("model_count", &self.registered_models.len())
            .field("models", &self.registered_models().join(", "))
            .finish_non_exhaustive()
    }
}

/// Shared state for Axum handlers.
struct AdminSiteState {
    registered_models: HashMap<String, ModelAdmin>,
    url_prefix: String,
    name: String,
    db: Arc<dyn AdminDbExecutor>,
    log_store: Arc<dyn LogEntryStore>,
}

// ── Authentication Handlers ────────────────────────────────────────

/// Handler for `POST /login/` - authenticate with username/password.
async fn handle_login(axum::Json(payload): axum::Json<LoginRequest>) -> impl IntoResponse {
    // Hardcoded admin/admin for development
    if payload.username == "admin" && payload.password == "admin" {
        let response = LoginResponse {
            token: "django-rs-dev-token-admin".to_string(),
            user: CurrentUserResponse {
                username: "admin".to_string(),
                email: "admin@example.com".to_string(),
                is_staff: true,
                is_superuser: true,
                full_name: "Admin User".to_string(),
            },
        };
        axum::Json(serde_json::to_value(response).unwrap_or_default()).into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": "Invalid credentials"
            })),
        )
            .into_response()
    }
}

/// Handler for `POST /logout/` - invalidate session.
async fn handle_logout() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

// ── Index / Me Handlers ────────────────────────────────────────────

/// Handler for `GET /` - list all registered models.
async fn handle_index(State(state): State<Arc<AdminSiteState>>) -> impl IntoResponse {
    let admins: Vec<&ModelAdmin> = state.registered_models.values().collect();
    let index = build_model_index(&admins, &state.url_prefix);
    axum::Json(serde_json::json!({
        "site_name": state.name,
        "apps": index.apps,
    }))
}

/// Handler for `GET /me/` - current user info placeholder.
async fn handle_me() -> impl IntoResponse {
    let user = CurrentUserResponse {
        username: "admin".to_string(),
        email: "admin@example.com".to_string(),
        is_staff: true,
        is_superuser: true,
        full_name: "Admin User".to_string(),
    };
    axum::Json(user)
}

// ── Log Entry Handlers ─────────────────────────────────────────────

/// Query parameters for the log endpoint.
#[derive(Debug, Deserialize)]
struct LogQueryParams {
    limit: Option<usize>,
}

/// Handler for `GET /log/` - recent log entries.
async fn handle_log_recent(
    State(state): State<Arc<AdminSiteState>>,
    Query(query): Query<LogQueryParams>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(10);
    let entries = state.log_store.recent(limit);
    axum::Json(serde_json::to_value(entries).unwrap_or_default())
}

/// Handler for `GET /log/:ct/:id/` - log entries for a specific object.
async fn handle_log_object(
    State(state): State<Arc<AdminSiteState>>,
    Path((ct, id)): Path<(String, String)>,
) -> impl IntoResponse {
    let entries = state.log_store.get_for_object(&ct, &id);
    axum::Json(serde_json::to_value(entries).unwrap_or_default())
}

// ── Schema / List / Detail / CRUD Handlers ─────────────────────────

/// Query parameters for the list endpoint.
#[derive(Debug, Deserialize)]
struct ListQueryParams {
    page: Option<usize>,
    page_size: Option<usize>,
    search: Option<String>,
    ordering: Option<String>,
}

/// Handler for `GET /:app/:model/schema` - model schema introspection.
async fn handle_schema(
    State(state): State<Arc<AdminSiteState>>,
    Path((app, model)): Path<(String, String)>,
) -> impl IntoResponse {
    let key = format!("{app}.{model}");
    state.registered_models.get(&key).map_or_else(
        || {
            (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({
                    "error": format!("Model '{key}' not found")
                })),
            )
                .into_response()
        },
        |admin| {
            let schema = ModelSchemaResponse::from_model_admin(admin);
            axum::Json(serde_json::to_value(schema).unwrap_or_default()).into_response()
        },
    )
}

/// Handler for `GET /:app/:model/` - list objects (paginated).
async fn handle_list(
    State(state): State<Arc<AdminSiteState>>,
    Path((app, model)): Path<(String, String)>,
    Query(query): Query<ListQueryParams>,
) -> impl IntoResponse {
    let key = format!("{app}.{model}");
    match state.registered_models.get(&key) {
        Some(admin) => {
            let params = AdminListParams {
                page: query.page.unwrap_or(1),
                page_size: query.page_size.unwrap_or(admin.list_per_page),
                search: query.search,
                ordering: query.ordering,
                filters: HashMap::new(),
            };
            match state.db.list_objects(admin, &params).await {
                Ok(result) => axum::Json(serde_json::to_value(result.response).unwrap_or_default())
                    .into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(serde_json::json!({"error": e})),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": format!("Model '{key}' not found")
            })),
        )
            .into_response(),
    }
}

/// Handler for `GET /:app/:model/:pk/` - get single object.
async fn handle_detail(
    State(state): State<Arc<AdminSiteState>>,
    Path((app, model, pk)): Path<(String, String, String)>,
) -> impl IntoResponse {
    let key = format!("{app}.{model}");
    match state.registered_models.get(&key) {
        Some(admin) => match state.db.get_object(admin, &pk).await {
            Ok(obj) => axum::Json(obj).into_response(),
            Err(e) => (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": format!("Model '{key}' not found")
            })),
        )
            .into_response(),
    }
}

/// Handler for `POST /:app/:model/` - create a new object.
async fn handle_create(
    State(state): State<Arc<AdminSiteState>>,
    Path((app, model)): Path<(String, String)>,
    axum::Json(body): axum::Json<HashMap<String, serde_json::Value>>,
) -> impl IntoResponse {
    let key = format!("{app}.{model}");
    match state.registered_models.get(&key) {
        Some(admin) => match state.db.create_object(admin, &body).await {
            Ok(obj) => {
                let pk = obj.get("id").map(|v| v.to_string()).unwrap_or_default();
                let repr = obj
                    .get("title")
                    .or_else(|| obj.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("object")
                    .to_string();
                state
                    .log_store
                    .log_addition(1, &key, &pk, &repr, "Created via admin");
                (StatusCode::CREATED, axum::Json(obj)).into_response()
            }
            Err(e) => (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": format!("Model '{key}' not found")
            })),
        )
            .into_response(),
    }
}

/// Handler for `PUT /:app/:model/:pk/` - update an object.
async fn handle_update(
    State(state): State<Arc<AdminSiteState>>,
    Path((app, model, pk)): Path<(String, String, String)>,
    axum::Json(body): axum::Json<HashMap<String, serde_json::Value>>,
) -> impl IntoResponse {
    let key = format!("{app}.{model}");
    match state.registered_models.get(&key) {
        Some(admin) => match state.db.update_object(admin, &pk, &body).await {
            Ok(obj) => {
                let repr = obj
                    .get("title")
                    .or_else(|| obj.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("object")
                    .to_string();
                let changed: Vec<String> = body.keys().cloned().collect();
                let msg = format!("Changed {}", changed.join(", "));
                state.log_store.log_change(1, &key, &pk, &repr, &msg);
                axum::Json(obj).into_response()
            }
            Err(e) => (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": format!("Model '{key}' not found")
            })),
        )
            .into_response(),
    }
}

/// Handler for `DELETE /:app/:model/:pk/` - delete an object.
async fn handle_delete(
    State(state): State<Arc<AdminSiteState>>,
    Path((app, model, pk)): Path<(String, String, String)>,
) -> impl IntoResponse {
    let key = format!("{app}.{model}");
    match state.registered_models.get(&key) {
        Some(admin) => {
            // Try to get the object repr before deleting
            let repr = state
                .db
                .get_object(admin, &pk)
                .await
                .ok()
                .and_then(|obj| {
                    obj.get("title")
                        .or_else(|| obj.get("name"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
                .unwrap_or_else(|| format!("{key} object"));

            match state.db.delete_object(admin, &pk).await {
                Ok(true) => {
                    state.log_store.log_deletion(1, &key, &pk, &repr, "");
                    StatusCode::NO_CONTENT.into_response()
                }
                Ok(false) => (
                    StatusCode::NOT_FOUND,
                    axum::Json(serde_json::json!({"error": "Object not found"})),
                )
                    .into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(serde_json::json!({"error": e})),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": format!("Model '{key}' not found")
            })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_admin::FieldSchema;

    #[test]
    fn test_admin_site_new() {
        let site = AdminSite::new("admin");
        assert_eq!(site.name(), "admin");
        assert_eq!(site.url_prefix_str(), "/api/admin");
        assert_eq!(site.model_count(), 0);
        assert!(site.static_dir_path().is_none());
    }

    #[test]
    fn test_admin_site_custom_prefix() {
        let site = AdminSite::new("admin").url_prefix("/custom/api");
        assert_eq!(site.url_prefix_str(), "/custom/api");
    }

    #[test]
    fn test_admin_site_static_dir() {
        let site = AdminSite::new("admin").static_dir(PathBuf::from("/static"));
        assert_eq!(site.static_dir_path(), Some(&PathBuf::from("/static")));
    }

    #[test]
    fn test_admin_site_register() {
        let mut site = AdminSite::new("admin");
        site.register("blog.article", ModelAdmin::new("blog", "article"));
        assert!(site.is_registered("blog.article"));
        assert!(!site.is_registered("blog.comment"));
        assert_eq!(site.model_count(), 1);
    }

    #[test]
    fn test_admin_site_unregister() {
        let mut site = AdminSite::new("admin");
        site.register("blog.article", ModelAdmin::new("blog", "article"));
        assert!(site.is_registered("blog.article"));
        site.unregister("blog.article");
        assert!(!site.is_registered("blog.article"));
        assert_eq!(site.model_count(), 0);
    }

    #[test]
    fn test_admin_site_get_model_admin() {
        let mut site = AdminSite::new("admin");
        site.register(
            "blog.article",
            ModelAdmin::new("blog", "article").list_per_page(25),
        );
        let admin = site.get_model_admin("blog.article").unwrap();
        assert_eq!(admin.list_per_page, 25);
    }

    #[test]
    fn test_admin_site_get_model_admin_not_found() {
        let site = AdminSite::new("admin");
        assert!(site.get_model_admin("blog.article").is_none());
    }

    #[test]
    fn test_admin_site_registered_models() {
        let mut site = AdminSite::new("admin");
        site.register("blog.article", ModelAdmin::new("blog", "article"));
        site.register("auth.user", ModelAdmin::new("auth", "user"));
        let mut models = site.registered_models();
        models.sort_unstable();
        assert_eq!(models, vec!["auth.user", "blog.article"]);
    }

    #[test]
    fn test_admin_site_action_registry() {
        let mut site = AdminSite::new("admin");
        site.register("blog.article", ModelAdmin::new("blog", "article"));
        let registry = site.get_action_registry("blog.article").unwrap();
        assert_eq!(registry.action_names(), vec!["delete_selected"]);
    }

    #[test]
    fn test_admin_site_action_registry_not_found() {
        let site = AdminSite::new("admin");
        assert!(site.get_action_registry("blog.article").is_none());
    }

    #[test]
    fn test_admin_site_unregister_removes_action_registry() {
        let mut site = AdminSite::new("admin");
        site.register("blog.article", ModelAdmin::new("blog", "article"));
        site.unregister("blog.article");
        assert!(site.get_action_registry("blog.article").is_none());
    }

    #[test]
    fn test_admin_site_debug() {
        let mut site = AdminSite::new("admin");
        site.register("blog.article", ModelAdmin::new("blog", "article"));
        let debug = format!("{site:?}");
        assert!(debug.contains("AdminSite"));
        assert!(debug.contains("admin"));
    }

    #[test]
    fn test_admin_site_into_router() {
        let mut site = AdminSite::new("admin");
        site.register(
            "blog.article",
            ModelAdmin::new("blog", "article").fields_schema(vec![
                FieldSchema::new("id", "BigAutoField").primary_key(),
                FieldSchema::new("title", "CharField").max_length(200),
            ]),
        );
        // Should not panic
        let _router = site.into_axum_router();
    }

    #[test]
    fn test_admin_site_register_overwrite() {
        let mut site = AdminSite::new("admin");
        site.register(
            "blog.article",
            ModelAdmin::new("blog", "article").list_per_page(10),
        );
        site.register(
            "blog.article",
            ModelAdmin::new("blog", "article").list_per_page(50),
        );
        assert_eq!(site.model_count(), 1);
        let admin = site.get_model_admin("blog.article").unwrap();
        assert_eq!(admin.list_per_page, 50);
    }

    #[test]
    fn test_admin_site_multiple_registrations() {
        let mut site = AdminSite::new("admin");
        site.register("blog.article", ModelAdmin::new("blog", "article"));
        site.register("blog.comment", ModelAdmin::new("blog", "comment"));
        site.register("auth.user", ModelAdmin::new("auth", "user"));
        assert_eq!(site.model_count(), 3);
    }

    #[test]
    fn test_admin_site_with_db_and_log_store() {
        let db = Arc::new(InMemoryAdminDb::new());
        let log_store = Arc::new(InMemoryLogEntryStore::new());
        let mut site = AdminSite::new("admin").db(db).log_store(log_store);
        site.register("blog.article", ModelAdmin::new("blog", "article"));
        let _router = site.into_axum_router();
    }
}
