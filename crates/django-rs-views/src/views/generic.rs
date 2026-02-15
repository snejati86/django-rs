//! Generic CRUD views for django-rs.
//!
//! This module provides trait-based generic views that mirror Django's
//! `django.views.generic` module. These views provide standard patterns for
//! listing, viewing, creating, updating, and deleting objects.
//!
//! The views use `serde_json::Value` as the object representation, allowing
//! them to work with any data source through trait implementations.
//!
//! ## View Traits
//!
//! - [`ListView`] - Displays a list of objects with optional pagination
//! - [`DetailView`] - Displays a single object
//! - [`CreateView`] - Creates a new object from form data
//! - [`UpdateView`] - Updates an existing object
//! - [`DeleteView`] - Deletes an existing object

use std::collections::HashMap;

use async_trait::async_trait;

use django_rs_core::DjangoError;
use django_rs_http::{HttpRequest, HttpResponse, HttpResponseRedirect};

use super::class_based::{ContextMixin, View};

/// A view for displaying a list of objects. Equivalent to Django's `ListView`.
///
/// Implementors provide the model name, queryset retrieval logic, and optional
/// pagination settings. The view renders the object list as a template.
#[async_trait]
pub trait ListView: View + ContextMixin + Send + Sync {
    /// Returns the model name for this list view.
    fn model_name(&self) -> &str;

    /// Returns the template name suffix for list views.
    fn template_name_suffix(&self) -> &str {
        "_list"
    }

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}{}.html", self.model_name(), self.template_name_suffix())
    }

    /// Returns the number of objects per page, or `None` for no pagination.
    fn paginate_by(&self) -> Option<usize> {
        None
    }

    /// Returns the ordering fields for the queryset.
    fn ordering(&self) -> Option<Vec<String>> {
        None
    }

    /// Retrieves the list of objects to display.
    async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError>;

    /// Handles GET requests for the list view.
    async fn list(&self, _request: HttpRequest) -> HttpResponse {
        match self.get_queryset().await {
            Ok(objects) => {
                let paginated = if let Some(per_page) = self.paginate_by() {
                    objects.into_iter().take(per_page).collect()
                } else {
                    objects
                };

                let mut context = self.get_context_data(&HashMap::new());
                context.insert(
                    "object_list".to_string(),
                    serde_json::Value::Array(paginated),
                );

                let body = serde_json::to_string_pretty(&context).unwrap_or_default();
                let template = self.template_name();
                let html = format!(
                    "<!-- Template: {template} -->\n<html><body><pre>{body}</pre></body></html>"
                );
                let mut response = HttpResponse::ok(html);
                response.set_content_type("text/html");
                response
            }
            Err(e) => HttpResponse::server_error(format!("Error fetching objects: {e}")),
        }
    }
}

/// A view for displaying a single object. Equivalent to Django's `DetailView`.
///
/// Implementors provide object retrieval logic and the view renders the
/// object detail as a template.
#[async_trait]
pub trait DetailView: View + ContextMixin + Send + Sync {
    /// Returns the model name for this detail view.
    fn model_name(&self) -> &str;

    /// Returns the template name suffix for detail views.
    fn template_name_suffix(&self) -> &str {
        "_detail"
    }

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}{}.html", self.model_name(), self.template_name_suffix())
    }

    /// Returns the field name used for slug-based lookups.
    fn slug_field(&self) -> &str {
        "slug"
    }

    /// Returns the URL keyword argument name for the primary key.
    fn pk_url_kwarg(&self) -> &str {
        "pk"
    }

    /// Retrieves the object to display.
    async fn get_object(
        &self,
        kwargs: &HashMap<String, String>,
    ) -> Result<serde_json::Value, DjangoError>;

    /// Handles GET requests for the detail view.
    async fn detail(&self, _request: HttpRequest, kwargs: &HashMap<String, String>) -> HttpResponse {
        match self.get_object(kwargs).await {
            Ok(object) => {
                let mut context = self.get_context_data(kwargs);
                context.insert("object".to_string(), object);

                let body = serde_json::to_string_pretty(&context).unwrap_or_default();
                let template = self.template_name();
                let html = format!(
                    "<!-- Template: {template} -->\n<html><body><pre>{body}</pre></body></html>"
                );
                let mut response = HttpResponse::ok(html);
                response.set_content_type("text/html");
                response
            }
            Err(DjangoError::NotFound(msg) | DjangoError::DoesNotExist(msg)) => {
                HttpResponse::not_found(msg)
            }
            Err(e) => HttpResponse::server_error(format!("Error fetching object: {e}")),
        }
    }
}

/// A view for creating a new object from form data. Equivalent to Django's `CreateView`.
///
/// Implementors provide validation logic and the success URL for after creation.
#[async_trait]
pub trait CreateView: View + ContextMixin + Send + Sync {
    /// Returns the model name for this create view.
    fn model_name(&self) -> &str;

    /// Returns the template name suffix for form views.
    fn template_name_suffix(&self) -> &str {
        "_form"
    }

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}{}.html", self.model_name(), self.template_name_suffix())
    }

    /// Returns the list of fields to include in the form.
    fn fields(&self) -> Vec<String>;

    /// Returns the URL to redirect to after successful creation.
    fn success_url(&self) -> &str;

    /// Handles valid form data by creating the object.
    async fn form_valid(&self, data: HashMap<String, String>) -> HttpResponse;

    /// Handles invalid form data by returning error information.
    async fn form_invalid(&self, errors: HashMap<String, Vec<String>>) -> HttpResponse;

    /// Handles GET requests by rendering the empty form.
    async fn render_form(&self) -> HttpResponse {
        let context = self.get_context_data(&HashMap::new());
        let body = serde_json::to_string_pretty(&context).unwrap_or_default();
        let template = self.template_name();
        let html = format!(
            "<!-- Template: {template} -->\n<html><body><pre>{body}</pre></body></html>"
        );
        let mut response = HttpResponse::ok(html);
        response.set_content_type("text/html");
        response
    }
}

/// A view for updating an existing object. Equivalent to Django's `UpdateView`.
///
/// Similar to `CreateView` but loads an existing object for editing.
#[async_trait]
pub trait UpdateView: View + ContextMixin + Send + Sync {
    /// Returns the model name for this update view.
    fn model_name(&self) -> &str;

    /// Returns the template name suffix for form views.
    fn template_name_suffix(&self) -> &str {
        "_form"
    }

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}{}.html", self.model_name(), self.template_name_suffix())
    }

    /// Returns the list of fields to include in the form.
    fn fields(&self) -> Vec<String>;

    /// Returns the URL to redirect to after successful update.
    fn success_url(&self) -> &str;

    /// Returns the URL keyword argument name for the primary key.
    fn pk_url_kwarg(&self) -> &str {
        "pk"
    }

    /// Retrieves the object to update.
    async fn get_object(
        &self,
        kwargs: &HashMap<String, String>,
    ) -> Result<serde_json::Value, DjangoError>;

    /// Handles valid form data by updating the object.
    async fn form_valid(&self, data: HashMap<String, String>) -> HttpResponse;

    /// Handles invalid form data by returning error information.
    async fn form_invalid(&self, errors: HashMap<String, Vec<String>>) -> HttpResponse;
}

/// A view for deleting an existing object. Equivalent to Django's `DeleteView`.
///
/// Displays a confirmation page on GET and performs deletion on POST.
#[async_trait]
pub trait DeleteView: View + ContextMixin + Send + Sync {
    /// Returns the model name for this delete view.
    fn model_name(&self) -> &str;

    /// Returns the template name suffix for confirm-delete views.
    fn template_name_suffix(&self) -> &str {
        "_confirm_delete"
    }

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}{}.html", self.model_name(), self.template_name_suffix())
    }

    /// Returns the URL to redirect to after successful deletion.
    fn success_url(&self) -> &str;

    /// Returns the URL keyword argument name for the primary key.
    fn pk_url_kwarg(&self) -> &str {
        "pk"
    }

    /// Retrieves the object to delete.
    async fn get_object(
        &self,
        kwargs: &HashMap<String, String>,
    ) -> Result<serde_json::Value, DjangoError>;

    /// Performs the actual deletion.
    async fn perform_delete(
        &self,
        kwargs: &HashMap<String, String>,
    ) -> Result<(), DjangoError>;

    /// Handles POST requests by deleting the object and redirecting.
    async fn delete_and_redirect(
        &self,
        kwargs: &HashMap<String, String>,
    ) -> HttpResponse {
        match self.perform_delete(kwargs).await {
            Ok(()) => HttpResponseRedirect::new(self.success_url()),
            Err(e) => HttpResponse::server_error(format!("Error deleting object: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ListView tests ──────────────────────────────────────────────

    struct TestListView {
        items: Vec<serde_json::Value>,
    }

    impl ContextMixin for TestListView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            let mut context = HashMap::new();
            context.insert(
                "model".to_string(),
                serde_json::json!(self.model_name()),
            );
            context
        }
    }

    #[async_trait]
    impl View for TestListView {
        async fn get(&self, request: HttpRequest) -> HttpResponse {
            self.list(request).await
        }
    }

    #[async_trait]
    impl ListView for TestListView {
        fn model_name(&self) -> &str {
            "article"
        }

        fn paginate_by(&self) -> Option<usize> {
            Some(2)
        }

        async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError> {
            Ok(self.items.clone())
        }
    }

    #[tokio::test]
    async fn test_list_view_template_name() {
        let view = TestListView { items: vec![] };
        assert_eq!(view.template_name(), "article_list.html");
    }

    #[tokio::test]
    async fn test_list_view_model_name() {
        let view = TestListView { items: vec![] };
        assert_eq!(view.model_name(), "article");
    }

    #[tokio::test]
    async fn test_list_view_get() {
        let view = TestListView {
            items: vec![
                serde_json::json!({"title": "First"}),
                serde_json::json!({"title": "Second"}),
            ],
        };
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("First"));
        assert!(body.contains("Second"));
    }

    #[tokio::test]
    async fn test_list_view_pagination() {
        let view = TestListView {
            items: vec![
                serde_json::json!({"title": "First"}),
                serde_json::json!({"title": "Second"}),
                serde_json::json!({"title": "Third"}),
            ],
        };
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view.dispatch(request).await;
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("First"));
        assert!(body.contains("Second"));
        assert!(!body.contains("Third")); // Paginated to 2
    }

    #[tokio::test]
    async fn test_list_view_template_name_suffix() {
        let view = TestListView { items: vec![] };
        assert_eq!(view.template_name_suffix(), "_list");
    }

    // ── DetailView tests ────────────────────────────────────────────

    struct TestDetailView {
        object: Option<serde_json::Value>,
    }

    impl ContextMixin for TestDetailView {
        fn get_context_data(
            &self,
            kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            let mut context = HashMap::new();
            for (k, v) in kwargs {
                context.insert(k.clone(), serde_json::json!(v));
            }
            context
        }
    }

    #[async_trait]
    impl View for TestDetailView {
        async fn get(&self, request: HttpRequest) -> HttpResponse {
            let kwargs = HashMap::new();
            self.detail(request, &kwargs).await
        }
    }

    #[async_trait]
    impl DetailView for TestDetailView {
        fn model_name(&self) -> &str {
            "article"
        }

        async fn get_object(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> Result<serde_json::Value, DjangoError> {
            self.object
                .clone()
                .ok_or_else(|| DjangoError::NotFound("Article not found".to_string()))
        }
    }

    #[tokio::test]
    async fn test_detail_view_template_name() {
        let view = TestDetailView { object: None };
        assert_eq!(view.template_name(), "article_detail.html");
    }

    #[tokio::test]
    async fn test_detail_view_found() {
        let view = TestDetailView {
            object: Some(serde_json::json!({"title": "My Article", "pk": 1})),
        };
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("My Article"));
    }

    #[tokio::test]
    async fn test_detail_view_not_found() {
        let view = TestDetailView { object: None };
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_detail_view_slug_field() {
        let view = TestDetailView { object: None };
        assert_eq!(view.slug_field(), "slug");
    }

    #[tokio::test]
    async fn test_detail_view_pk_url_kwarg() {
        let view = TestDetailView { object: None };
        assert_eq!(view.pk_url_kwarg(), "pk");
    }

    // ── CreateView tests ────────────────────────────────────────────

    struct TestCreateView;

    impl ContextMixin for TestCreateView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            let mut context = HashMap::new();
            context.insert("form_fields".to_string(), serde_json::json!(self.fields()));
            context
        }
    }

    #[async_trait]
    impl View for TestCreateView {
        async fn get(&self, _request: HttpRequest) -> HttpResponse {
            self.render_form().await
        }

        async fn post(&self, _request: HttpRequest) -> HttpResponse {
            let data = HashMap::new();
            self.form_valid(data).await
        }
    }

    #[async_trait]
    impl CreateView for TestCreateView {
        fn model_name(&self) -> &str {
            "article"
        }

        fn fields(&self) -> Vec<String> {
            vec!["title".to_string(), "body".to_string()]
        }

        fn success_url(&self) -> &str {
            "/articles/"
        }

        async fn form_valid(&self, _data: HashMap<String, String>) -> HttpResponse {
            HttpResponseRedirect::new(self.success_url())
        }

        async fn form_invalid(&self, errors: HashMap<String, Vec<String>>) -> HttpResponse {
            let body = serde_json::to_string(&errors).unwrap_or_default();
            HttpResponse::bad_request(body)
        }
    }

    #[tokio::test]
    async fn test_create_view_template_name() {
        let view = TestCreateView;
        assert_eq!(view.template_name(), "article_form.html");
    }

    #[tokio::test]
    async fn test_create_view_fields() {
        let view = TestCreateView;
        assert_eq!(view.fields(), vec!["title", "body"]);
    }

    #[tokio::test]
    async fn test_create_view_success_url() {
        let view = TestCreateView;
        assert_eq!(view.success_url(), "/articles/");
    }

    #[tokio::test]
    async fn test_create_view_get_renders_form() {
        let view = TestCreateView;
        let request = HttpRequest::builder()
            .method(http::Method::GET)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_view_post_redirects() {
        let view = TestCreateView;
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::FOUND);
        assert_eq!(
            response
                .headers()
                .get(http::header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "/articles/"
        );
    }

    #[tokio::test]
    async fn test_create_view_form_invalid() {
        let view = TestCreateView;
        let mut errors = HashMap::new();
        errors.insert("title".to_string(), vec!["Required".to_string()]);
        let response = view.form_invalid(errors).await;
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    }

    // ── DeleteView tests ────────────────────────────────────────────

    struct TestDeleteView {
        should_fail: bool,
    }

    impl ContextMixin for TestDeleteView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
    }

    #[async_trait]
    impl View for TestDeleteView {
        async fn post(&self, _request: HttpRequest) -> HttpResponse {
            let kwargs = HashMap::new();
            self.delete_and_redirect(&kwargs).await
        }
    }

    #[async_trait]
    impl DeleteView for TestDeleteView {
        fn model_name(&self) -> &str {
            "article"
        }

        fn success_url(&self) -> &str {
            "/articles/"
        }

        async fn get_object(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> Result<serde_json::Value, DjangoError> {
            Ok(serde_json::json!({"pk": 1}))
        }

        async fn perform_delete(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> Result<(), DjangoError> {
            if self.should_fail {
                Err(DjangoError::DatabaseError("Delete failed".to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn test_delete_view_template_name() {
        let view = TestDeleteView { should_fail: false };
        assert_eq!(view.template_name(), "article_confirm_delete.html");
    }

    #[tokio::test]
    async fn test_delete_view_success() {
        let view = TestDeleteView { should_fail: false };
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::FOUND);
    }

    #[tokio::test]
    async fn test_delete_view_failure() {
        let view = TestDeleteView { should_fail: true };
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_delete_view_pk_url_kwarg() {
        let view = TestDeleteView { should_fail: false };
        assert_eq!(view.pk_url_kwarg(), "pk");
    }

    // ── UpdateView tests ────────────────────────────────────────────

    struct TestUpdateView;

    impl ContextMixin for TestUpdateView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
    }

    #[async_trait]
    impl View for TestUpdateView {
        async fn post(&self, _request: HttpRequest) -> HttpResponse {
            let data = HashMap::new();
            self.form_valid(data).await
        }
    }

    #[async_trait]
    impl UpdateView for TestUpdateView {
        fn model_name(&self) -> &str {
            "article"
        }

        fn fields(&self) -> Vec<String> {
            vec!["title".to_string()]
        }

        fn success_url(&self) -> &str {
            "/articles/"
        }

        async fn get_object(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> Result<serde_json::Value, DjangoError> {
            Ok(serde_json::json!({"pk": 1, "title": "Old Title"}))
        }

        async fn form_valid(&self, _data: HashMap<String, String>) -> HttpResponse {
            HttpResponseRedirect::new(self.success_url())
        }

        async fn form_invalid(&self, errors: HashMap<String, Vec<String>>) -> HttpResponse {
            let body = serde_json::to_string(&errors).unwrap_or_default();
            HttpResponse::bad_request(body)
        }
    }

    #[tokio::test]
    async fn test_update_view_template_name() {
        let view = TestUpdateView;
        assert_eq!(view.template_name(), "article_form.html");
    }

    #[tokio::test]
    async fn test_update_view_fields() {
        let view = TestUpdateView;
        assert_eq!(view.fields(), vec!["title"]);
    }

    #[tokio::test]
    async fn test_update_view_success_url() {
        let view = TestUpdateView;
        assert_eq!(view.success_url(), "/articles/");
    }

    #[tokio::test]
    async fn test_update_view_post_redirects() {
        let view = TestUpdateView;
        let request = HttpRequest::builder()
            .method(http::Method::POST)
            .build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::FOUND);
    }

    #[tokio::test]
    async fn test_update_view_pk_url_kwarg() {
        let view = TestUpdateView;
        assert_eq!(view.pk_url_kwarg(), "pk");
    }

    #[tokio::test]
    async fn test_update_view_form_invalid() {
        let view = TestUpdateView;
        let mut errors = HashMap::new();
        errors.insert("title".to_string(), vec!["Too long".to_string()]);
        let response = view.form_invalid(errors).await;
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    }
}
