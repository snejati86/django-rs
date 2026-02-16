//! Date-based archive views for django-rs.
//!
//! This module provides trait-based archive views that mirror Django's
//! `django.views.generic.dates` module. These views filter and group
//! objects by a date field, supporting year, month, day, and "today"
//! archive patterns.
//!
//! ## View Traits
//!
//! - [`ArchiveIndexView`] - Lists objects by date, newest first
//! - [`YearArchiveView`] - Lists objects for a given year
//! - [`MonthArchiveView`] - Lists objects for a given year/month
//! - [`DayArchiveView`] - Lists objects for a specific date
//! - [`TodayArchiveView`] - Like `DayArchiveView` but for today's date
//! - [`DateDetailView`] - Detail view that also validates the date
//!
//! ## Usage
//!
//! Each view accepts a `date_field` parameter that names the JSON field
//! to use for date-based filtering. Dates in the queryset are expected
//! to be ISO 8601 format strings (e.g., `"2026-02-15"` or
//! `"2026-02-15T10:30:00"`).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{Datelike, NaiveDate};

use django_rs_core::DjangoError;
use django_rs_http::{HttpRequest, HttpResponse};
use django_rs_template::context::{Context, ContextValue};
use django_rs_template::engine::Engine;

use super::class_based::{ContextMixin, View};
use crate::pagination::Paginator;

/// Renders a template with the given name and serde_json context using the engine.
///
/// If no engine is provided, falls back to a JSON representation.
fn render_with_engine(
    template_name: &str,
    context: &HashMap<String, serde_json::Value>,
    engine: Option<&Engine>,
) -> HttpResponse {
    if let Some(engine) = engine {
        let mut template_context = Context::new();
        for (key, value) in context {
            template_context.set(key.clone(), ContextValue::from(value.clone()));
        }
        match engine.render_to_string(template_name, &mut template_context) {
            Ok(html) => {
                let mut response = HttpResponse::ok(html);
                response.set_content_type("text/html");
                response
            }
            Err(e) => HttpResponse::server_error(format!("Template error: {e}")),
        }
    } else {
        let body = serde_json::to_string_pretty(context).unwrap_or_default();
        let html = format!(
            "<!-- Template: {template_name} -->\n<html><body><pre>{body}</pre></body></html>"
        );
        let mut response = HttpResponse::ok(html);
        response.set_content_type("text/html");
        response
    }
}

/// Extracts a `NaiveDate` from a JSON value's date field.
///
/// Supports date-only strings (`"2026-02-15"`) and datetime strings
/// (`"2026-02-15T10:30:00"`) by taking just the date part.
fn extract_date(value: &serde_json::Value, date_field: &str) -> Option<NaiveDate> {
    value
        .get(date_field)
        .and_then(|v| v.as_str())
        .and_then(|s| {
            // Try full date first
            NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().or_else(|| {
                // Try datetime (take first 10 chars = YYYY-MM-DD)
                if s.len() >= 10 {
                    NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok()
                } else {
                    None
                }
            })
        })
}

/// Filters objects whose date field falls within a given year.
fn filter_by_year(
    objects: &[serde_json::Value],
    date_field: &str,
    year: i32,
) -> Vec<serde_json::Value> {
    objects
        .iter()
        .filter(|obj| extract_date(obj, date_field).is_some_and(|d| d.year() == year))
        .cloned()
        .collect()
}

/// Filters objects whose date field falls within a given year and month.
fn filter_by_month(
    objects: &[serde_json::Value],
    date_field: &str,
    year: i32,
    month: u32,
) -> Vec<serde_json::Value> {
    objects
        .iter()
        .filter(|obj| {
            extract_date(obj, date_field).is_some_and(|d| d.year() == year && d.month() == month)
        })
        .cloned()
        .collect()
}

/// Filters objects whose date field matches a specific date.
fn filter_by_day(
    objects: &[serde_json::Value],
    date_field: &str,
    date: NaiveDate,
) -> Vec<serde_json::Value> {
    objects
        .iter()
        .filter(|obj| extract_date(obj, date_field).is_some_and(|d| d == date))
        .cloned()
        .collect()
}

/// Sorts objects by date field, newest first.
fn sort_by_date_desc(objects: &mut [serde_json::Value], date_field: &str) {
    objects.sort_by(|a, b| {
        let da = extract_date(a, date_field);
        let db = extract_date(b, date_field);
        db.cmp(&da) // Newest first
    });
}

/// Collects unique dates from objects, sorted newest first.
fn collect_date_list(objects: &[serde_json::Value], date_field: &str) -> Vec<String> {
    let mut dates: Vec<NaiveDate> = objects
        .iter()
        .filter_map(|obj| extract_date(obj, date_field))
        .collect();
    dates.sort_unstable();
    dates.dedup();
    dates.reverse(); // Newest first
    dates
        .iter()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .collect()
}

// ── Trait: DateMixin (shared config) ──────────────────────────────────

/// Provides the date field configuration shared by all archive views.
///
/// This is a helper trait that centralizes common date-related configuration,
/// similar to Django's `DateMixin`.
pub trait DateMixin {
    /// Returns the name of the date field to filter on.
    fn date_field(&self) -> &str;

    /// Whether to allow future dates in the archive. Default: `false`.
    fn allow_future(&self) -> bool {
        false
    }
}

// ── ArchiveIndexView ──────────────────────────────────────────────────

/// A view for listing all objects by date, newest first.
///
/// Equivalent to Django's `ArchiveIndexView`. This is the "landing page"
/// of a date-based archive, showing the most recent objects and a list
/// of dates that have objects.
///
/// The template context includes:
/// - `object_list`: The objects for the current page
/// - `date_list`: Unique dates that have objects
/// - `latest`: The most recent object, if any
#[async_trait]
pub trait ArchiveIndexView: View + ContextMixin + DateMixin + Send + Sync {
    /// Returns the model name for this archive view.
    fn model_name(&self) -> &str;

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}_archive.html", self.model_name())
    }

    /// Returns the number of objects per page, or `None` for no pagination.
    fn paginate_by(&self) -> Option<usize> {
        None
    }

    /// Returns an optional template engine for rendering.
    fn engine(&self) -> Option<&Engine> {
        None
    }

    /// Retrieves all objects to display.
    async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError>;

    /// Handles GET requests for the archive index.
    async fn archive_index(&self, request: HttpRequest) -> HttpResponse {
        match self.get_queryset().await {
            Ok(mut objects) => {
                let date_field = self.date_field();

                // Filter out future dates unless allowed
                if !self.allow_future() {
                    let today = chrono::Utc::now().date_naive();
                    objects
                        .retain(|obj| extract_date(obj, date_field).map_or(true, |d| d <= today));
                }

                // Sort newest first
                sort_by_date_desc(&mut objects, date_field);

                // Collect date list
                let date_list = collect_date_list(&objects, date_field);
                let latest = objects.first().cloned();

                let mut context = self.get_context_data(&HashMap::new());
                context.insert("date_list".to_string(), serde_json::json!(date_list));
                if let Some(latest_obj) = latest {
                    context.insert("latest".to_string(), latest_obj);
                }

                // Paginate
                if let Some(per_page) = self.paginate_by() {
                    let page_number = request
                        .get()
                        .get("page")
                        .and_then(|p| p.parse::<usize>().ok())
                        .unwrap_or(1);
                    let paginator = Paginator::new(objects, per_page);
                    let page = paginator.get_page(page_number);

                    context.insert(
                        "object_list".to_string(),
                        serde_json::Value::Array(page.object_list().to_vec()),
                    );
                    context.insert(
                        "page_obj".to_string(),
                        serde_json::json!({
                            "number": page.number(),
                            "has_next": page.has_next(),
                            "has_previous": page.has_previous(),
                        }),
                    );
                    context.insert(
                        "is_paginated".to_string(),
                        serde_json::Value::Bool(paginator.num_pages() > 1),
                    );
                } else {
                    context.insert("object_list".to_string(), serde_json::Value::Array(objects));
                    context.insert("is_paginated".to_string(), serde_json::Value::Bool(false));
                }

                let template = self.template_name();
                render_with_engine(&template, &context, self.engine())
            }
            Err(e) => HttpResponse::server_error(format!("Error fetching objects: {e}")),
        }
    }
}

// ── YearArchiveView ───────────────────────────────────────────────────

/// A view for listing objects for a given year.
///
/// Equivalent to Django's `YearArchiveView`. Filters the queryset to objects
/// whose date field falls within the specified year.
///
/// The template context includes:
/// - `object_list`: The objects for the year
/// - `year`: The year as a string
/// - `date_list`: Unique dates within the year
#[async_trait]
pub trait YearArchiveView: View + ContextMixin + DateMixin + Send + Sync {
    /// Returns the model name for this archive view.
    fn model_name(&self) -> &str;

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}_archive_year.html", self.model_name())
    }

    /// Returns an optional template engine for rendering.
    fn engine(&self) -> Option<&Engine> {
        None
    }

    /// Retrieves all objects to filter.
    async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError>;

    /// Handles GET requests for a year archive.
    async fn year_archive(&self, _request: HttpRequest, year: i32) -> HttpResponse {
        match self.get_queryset().await {
            Ok(objects) => {
                let date_field = self.date_field();
                let mut filtered = filter_by_year(&objects, date_field, year);

                if !self.allow_future() {
                    let today = chrono::Utc::now().date_naive();
                    filtered
                        .retain(|obj| extract_date(obj, date_field).map_or(true, |d| d <= today));
                }

                sort_by_date_desc(&mut filtered, date_field);
                let date_list = collect_date_list(&filtered, date_field);

                let mut context = self.get_context_data(&HashMap::new());
                context.insert("year".to_string(), serde_json::json!(year.to_string()));
                context.insert(
                    "object_list".to_string(),
                    serde_json::Value::Array(filtered),
                );
                context.insert("date_list".to_string(), serde_json::json!(date_list));

                let template = self.template_name();
                render_with_engine(&template, &context, self.engine())
            }
            Err(e) => HttpResponse::server_error(format!("Error fetching objects: {e}")),
        }
    }
}

// ── MonthArchiveView ──────────────────────────────────────────────────

/// A view for listing objects for a given year and month.
///
/// Equivalent to Django's `MonthArchiveView`. Filters the queryset to objects
/// whose date field falls within the specified year and month.
///
/// The template context includes:
/// - `object_list`: The objects for the month
/// - `year`: The year as a string
/// - `month`: The month as a string (zero-padded)
/// - `date_list`: Unique dates within the month
#[async_trait]
pub trait MonthArchiveView: View + ContextMixin + DateMixin + Send + Sync {
    /// Returns the model name for this archive view.
    fn model_name(&self) -> &str;

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}_archive_month.html", self.model_name())
    }

    /// Returns an optional template engine for rendering.
    fn engine(&self) -> Option<&Engine> {
        None
    }

    /// Retrieves all objects to filter.
    async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError>;

    /// Handles GET requests for a month archive.
    async fn month_archive(&self, _request: HttpRequest, year: i32, month: u32) -> HttpResponse {
        match self.get_queryset().await {
            Ok(objects) => {
                let date_field = self.date_field();
                let mut filtered = filter_by_month(&objects, date_field, year, month);

                if !self.allow_future() {
                    let today = chrono::Utc::now().date_naive();
                    filtered
                        .retain(|obj| extract_date(obj, date_field).map_or(true, |d| d <= today));
                }

                sort_by_date_desc(&mut filtered, date_field);
                let date_list = collect_date_list(&filtered, date_field);

                let mut context = self.get_context_data(&HashMap::new());
                context.insert("year".to_string(), serde_json::json!(year.to_string()));
                context.insert(
                    "month".to_string(),
                    serde_json::json!(format!("{month:02}")),
                );
                context.insert(
                    "object_list".to_string(),
                    serde_json::Value::Array(filtered),
                );
                context.insert("date_list".to_string(), serde_json::json!(date_list));

                let template = self.template_name();
                render_with_engine(&template, &context, self.engine())
            }
            Err(e) => HttpResponse::server_error(format!("Error fetching objects: {e}")),
        }
    }
}

// ── DayArchiveView ────────────────────────────────────────────────────

/// A view for listing objects on a specific date.
///
/// Equivalent to Django's `DayArchiveView`. Filters the queryset to objects
/// whose date field matches the specified date exactly.
///
/// The template context includes:
/// - `object_list`: The objects for the day
/// - `day`: The date as a `YYYY-MM-DD` string
#[async_trait]
pub trait DayArchiveView: View + ContextMixin + DateMixin + Send + Sync {
    /// Returns the model name for this archive view.
    fn model_name(&self) -> &str;

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}_archive_day.html", self.model_name())
    }

    /// Returns an optional template engine for rendering.
    fn engine(&self) -> Option<&Engine> {
        None
    }

    /// Retrieves all objects to filter.
    async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError>;

    /// Handles GET requests for a day archive.
    async fn day_archive(
        &self,
        _request: HttpRequest,
        year: i32,
        month: u32,
        day: u32,
    ) -> HttpResponse {
        let Some(date) = NaiveDate::from_ymd_opt(year, month, day) else {
            return HttpResponse::not_found(format!("Invalid date: {year}-{month:02}-{day:02}"));
        };

        if !self.allow_future() {
            let today = chrono::Utc::now().date_naive();
            if date > today {
                return HttpResponse::not_found(format!("Future date not allowed: {date}"));
            }
        }

        match self.get_queryset().await {
            Ok(objects) => {
                let date_field = self.date_field();
                let mut filtered = filter_by_day(&objects, date_field, date);
                sort_by_date_desc(&mut filtered, date_field);

                let mut context = self.get_context_data(&HashMap::new());
                context.insert(
                    "day".to_string(),
                    serde_json::json!(date.format("%Y-%m-%d").to_string()),
                );
                context.insert(
                    "object_list".to_string(),
                    serde_json::Value::Array(filtered),
                );

                let template = self.template_name();
                render_with_engine(&template, &context, self.engine())
            }
            Err(e) => HttpResponse::server_error(format!("Error fetching objects: {e}")),
        }
    }
}

// ── TodayArchiveView ──────────────────────────────────────────────────

/// A view for listing objects on today's date.
///
/// Equivalent to Django's `TodayArchiveView`. This is a convenience view
/// that delegates to `DayArchiveView` with today's date.
///
/// The template context includes:
/// - `object_list`: The objects for today
/// - `day`: Today's date as a `YYYY-MM-DD` string
#[async_trait]
pub trait TodayArchiveView: View + ContextMixin + DateMixin + Send + Sync {
    /// Returns the model name for this archive view.
    fn model_name(&self) -> &str;

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}_archive_day.html", self.model_name())
    }

    /// Returns an optional template engine for rendering.
    fn engine(&self) -> Option<&Engine> {
        None
    }

    /// Retrieves all objects to filter.
    async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError>;

    /// Handles GET requests for today's archive.
    async fn today_archive(&self, _request: HttpRequest) -> HttpResponse {
        let today = chrono::Utc::now().date_naive();

        match self.get_queryset().await {
            Ok(objects) => {
                let date_field = self.date_field();
                let mut filtered = filter_by_day(&objects, date_field, today);
                sort_by_date_desc(&mut filtered, date_field);

                let mut context = self.get_context_data(&HashMap::new());
                context.insert(
                    "day".to_string(),
                    serde_json::json!(today.format("%Y-%m-%d").to_string()),
                );
                context.insert(
                    "object_list".to_string(),
                    serde_json::Value::Array(filtered),
                );

                let template = self.template_name();
                render_with_engine(&template, &context, self.engine())
            }
            Err(e) => HttpResponse::server_error(format!("Error fetching objects: {e}")),
        }
    }
}

// ── DateDetailView ────────────────────────────────────────────────────

/// A detail view that validates the object's date field against the URL.
///
/// Equivalent to Django's `DateDetailView`. In addition to fetching a single
/// object by primary key or slug, this view validates that the object's date
/// matches the year/month/day specified in the URL.
///
/// The template context includes:
/// - `object`: The fetched object
/// - `day`: The validated date as a `YYYY-MM-DD` string
#[async_trait]
pub trait DateDetailView: View + ContextMixin + DateMixin + Send + Sync {
    /// Returns the model name for this detail view.
    fn model_name(&self) -> &str;

    /// Returns the template name for this view.
    fn template_name(&self) -> String {
        format!("{}_detail.html", self.model_name())
    }

    /// Returns an optional template engine for rendering.
    fn engine(&self) -> Option<&Engine> {
        None
    }

    /// Retrieves the object by keyword arguments (e.g., primary key).
    async fn get_object(
        &self,
        kwargs: &HashMap<String, String>,
    ) -> Result<serde_json::Value, DjangoError>;

    /// Handles GET requests for a date-validated detail view.
    async fn date_detail(
        &self,
        _request: HttpRequest,
        year: i32,
        month: u32,
        day: u32,
        kwargs: &HashMap<String, String>,
    ) -> HttpResponse {
        let Some(expected_date) = NaiveDate::from_ymd_opt(year, month, day) else {
            return HttpResponse::not_found(format!("Invalid date: {year}-{month:02}-{day:02}"));
        };

        match self.get_object(kwargs).await {
            Ok(object) => {
                let date_field = self.date_field();
                let obj_date = extract_date(&object, date_field);

                if obj_date != Some(expected_date) {
                    return HttpResponse::not_found(format!(
                        "Object date does not match {expected_date}"
                    ));
                }

                if !self.allow_future() {
                    let today = chrono::Utc::now().date_naive();
                    if expected_date > today {
                        return HttpResponse::not_found(format!(
                            "Future date not allowed: {expected_date}"
                        ));
                    }
                }

                let mut context = self.get_context_data(kwargs);
                context.insert("object".to_string(), object);
                context.insert(
                    "day".to_string(),
                    serde_json::json!(expected_date.format("%Y-%m-%d").to_string()),
                );

                let template = self.template_name();
                render_with_engine(&template, &context, self.engine())
            }
            Err(DjangoError::NotFound(msg) | DjangoError::DoesNotExist(msg)) => {
                HttpResponse::not_found(msg)
            }
            Err(e) => HttpResponse::server_error(format!("Error fetching object: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    // ── Helper functions tests ────────────────────────────────────────

    #[test]
    fn test_extract_date_from_date_string() {
        let obj = serde_json::json!({"pub_date": "2026-02-15"});
        let date = extract_date(&obj, "pub_date").unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 2, 15).unwrap());
    }

    #[test]
    fn test_extract_date_from_datetime_string() {
        let obj = serde_json::json!({"pub_date": "2026-02-15T10:30:00"});
        let date = extract_date(&obj, "pub_date").unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 2, 15).unwrap());
    }

    #[test]
    fn test_extract_date_missing_field() {
        let obj = serde_json::json!({"title": "Test"});
        assert!(extract_date(&obj, "pub_date").is_none());
    }

    #[test]
    fn test_extract_date_invalid_format() {
        let obj = serde_json::json!({"pub_date": "not-a-date"});
        assert!(extract_date(&obj, "pub_date").is_none());
    }

    #[test]
    fn test_extract_date_non_string_value() {
        let obj = serde_json::json!({"pub_date": 12345});
        assert!(extract_date(&obj, "pub_date").is_none());
    }

    #[test]
    fn test_filter_by_year() {
        let objects = vec![
            serde_json::json!({"title": "A", "pub_date": "2025-06-15"}),
            serde_json::json!({"title": "B", "pub_date": "2026-01-10"}),
            serde_json::json!({"title": "C", "pub_date": "2026-07-20"}),
            serde_json::json!({"title": "D", "pub_date": "2024-12-01"}),
        ];
        let result = filter_by_year(&objects, "pub_date", 2026);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["title"], "B");
        assert_eq!(result[1]["title"], "C");
    }

    #[test]
    fn test_filter_by_year_no_match() {
        let objects = vec![serde_json::json!({"pub_date": "2025-01-01"})];
        let result = filter_by_year(&objects, "pub_date", 2026);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_by_month() {
        let objects = vec![
            serde_json::json!({"title": "A", "pub_date": "2026-01-15"}),
            serde_json::json!({"title": "B", "pub_date": "2026-02-10"}),
            serde_json::json!({"title": "C", "pub_date": "2026-02-20"}),
            serde_json::json!({"title": "D", "pub_date": "2026-03-01"}),
        ];
        let result = filter_by_month(&objects, "pub_date", 2026, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["title"], "B");
        assert_eq!(result[1]["title"], "C");
    }

    #[test]
    fn test_filter_by_day() {
        let objects = vec![
            serde_json::json!({"title": "A", "pub_date": "2026-02-15"}),
            serde_json::json!({"title": "B", "pub_date": "2026-02-15"}),
            serde_json::json!({"title": "C", "pub_date": "2026-02-16"}),
        ];
        let date = NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let result = filter_by_day(&objects, "pub_date", date);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_sort_by_date_desc() {
        let mut objects = vec![
            serde_json::json!({"pub_date": "2026-01-01"}),
            serde_json::json!({"pub_date": "2026-03-01"}),
            serde_json::json!({"pub_date": "2026-02-01"}),
        ];
        sort_by_date_desc(&mut objects, "pub_date");
        assert_eq!(objects[0]["pub_date"], "2026-03-01");
        assert_eq!(objects[1]["pub_date"], "2026-02-01");
        assert_eq!(objects[2]["pub_date"], "2026-01-01");
    }

    #[test]
    fn test_collect_date_list() {
        let objects = vec![
            serde_json::json!({"pub_date": "2026-02-15"}),
            serde_json::json!({"pub_date": "2026-02-15"}),
            serde_json::json!({"pub_date": "2026-01-10"}),
            serde_json::json!({"pub_date": "2026-03-20"}),
        ];
        let dates = collect_date_list(&objects, "pub_date");
        assert_eq!(dates.len(), 3);
        // Newest first
        assert_eq!(dates[0], "2026-03-20");
        assert_eq!(dates[1], "2026-02-15");
        assert_eq!(dates[2], "2026-01-10");
    }

    #[test]
    fn test_collect_date_list_empty() {
        let objects: Vec<serde_json::Value> = vec![];
        let dates = collect_date_list(&objects, "pub_date");
        assert!(dates.is_empty());
    }

    // ── ArchiveIndexView tests ────────────────────────────────────────

    struct TestArchiveIndexView {
        items: Vec<serde_json::Value>,
        paginate: Option<usize>,
    }

    impl ContextMixin for TestArchiveIndexView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
    }

    impl DateMixin for TestArchiveIndexView {
        fn date_field(&self) -> &str {
            "pub_date"
        }
    }

    #[async_trait]
    impl View for TestArchiveIndexView {
        async fn get(&self, request: HttpRequest) -> HttpResponse {
            self.archive_index(request).await
        }
    }

    #[async_trait]
    impl ArchiveIndexView for TestArchiveIndexView {
        fn model_name(&self) -> &str {
            "article"
        }

        fn paginate_by(&self) -> Option<usize> {
            self.paginate
        }

        async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError> {
            Ok(self.items.clone())
        }
    }

    #[tokio::test]
    async fn test_archive_index_template_name() {
        let view = TestArchiveIndexView {
            items: vec![],
            paginate: None,
        };
        assert_eq!(view.template_name(), "article_archive.html");
    }

    #[tokio::test]
    async fn test_archive_index_basic() {
        let view = TestArchiveIndexView {
            items: vec![
                serde_json::json!({"title": "Older", "pub_date": "2025-01-01"}),
                serde_json::json!({"title": "Newer", "pub_date": "2025-06-15"}),
            ],
            paginate: None,
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("Newer"));
        assert!(body.contains("Older"));
        assert!(body.contains("date_list"));
    }

    #[tokio::test]
    async fn test_archive_index_sorted_newest_first() {
        let view = TestArchiveIndexView {
            items: vec![
                serde_json::json!({"title": "A", "pub_date": "2025-01-01"}),
                serde_json::json!({"title": "C", "pub_date": "2025-06-15"}),
                serde_json::json!({"title": "B", "pub_date": "2025-03-10"}),
            ],
            paginate: None,
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.dispatch(request).await;
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        // Verify "C" (newest) appears before "A" (oldest)
        let pos_c = body.find('C').unwrap();
        let pos_a = body.find("\"A\"").unwrap();
        assert!(pos_c < pos_a);
    }

    #[tokio::test]
    async fn test_archive_index_with_pagination() {
        let view = TestArchiveIndexView {
            items: (1..=5)
                .map(|i| {
                    serde_json::json!({
                        "title": format!("Article {i}"),
                        "pub_date": format!("2025-01-{i:02}")
                    })
                })
                .collect(),
            paginate: Some(2),
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.dispatch(request).await;
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("is_paginated"));
        assert!(body.contains("page_obj"));
    }

    #[tokio::test]
    async fn test_archive_index_empty() {
        let view = TestArchiveIndexView {
            items: vec![],
            paginate: None,
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("object_list"));
    }

    // ── YearArchiveView tests ─────────────────────────────────────────

    struct TestYearArchiveView {
        items: Vec<serde_json::Value>,
    }

    impl ContextMixin for TestYearArchiveView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
    }

    impl DateMixin for TestYearArchiveView {
        fn date_field(&self) -> &str {
            "pub_date"
        }
    }

    #[async_trait]
    impl View for TestYearArchiveView {
        async fn get(&self, request: HttpRequest) -> HttpResponse {
            self.year_archive(request, 2025).await
        }
    }

    #[async_trait]
    impl YearArchiveView for TestYearArchiveView {
        fn model_name(&self) -> &str {
            "article"
        }

        async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError> {
            Ok(self.items.clone())
        }
    }

    #[tokio::test]
    async fn test_year_archive_template_name() {
        let view = TestYearArchiveView { items: vec![] };
        assert_eq!(view.template_name(), "article_archive_year.html");
    }

    #[tokio::test]
    async fn test_year_archive_filters_by_year() {
        let view = TestYearArchiveView {
            items: vec![
                serde_json::json!({"title": "A", "pub_date": "2025-06-15"}),
                serde_json::json!({"title": "B", "pub_date": "2024-01-10"}),
                serde_json::json!({"title": "C", "pub_date": "2025-12-01"}),
            ],
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.year_archive(request, 2025).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("\"A\""));
        assert!(body.contains("\"C\""));
        assert!(!body.contains("\"B\"")); // 2024 - filtered out
        assert!(body.contains("\"year\""));
    }

    // ── MonthArchiveView tests ────────────────────────────────────────

    struct TestMonthArchiveView {
        items: Vec<serde_json::Value>,
    }

    impl ContextMixin for TestMonthArchiveView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
    }

    impl DateMixin for TestMonthArchiveView {
        fn date_field(&self) -> &str {
            "pub_date"
        }
    }

    #[async_trait]
    impl View for TestMonthArchiveView {}

    #[async_trait]
    impl MonthArchiveView for TestMonthArchiveView {
        fn model_name(&self) -> &str {
            "article"
        }

        async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError> {
            Ok(self.items.clone())
        }
    }

    #[tokio::test]
    async fn test_month_archive_template_name() {
        let view = TestMonthArchiveView { items: vec![] };
        assert_eq!(view.template_name(), "article_archive_month.html");
    }

    #[tokio::test]
    async fn test_month_archive_filters_by_month() {
        let view = TestMonthArchiveView {
            items: vec![
                serde_json::json!({"title": "A", "pub_date": "2025-02-15"}),
                serde_json::json!({"title": "B", "pub_date": "2025-02-20"}),
                serde_json::json!({"title": "C", "pub_date": "2025-03-01"}),
                serde_json::json!({"title": "D", "pub_date": "2025-01-31"}),
            ],
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.month_archive(request, 2025, 2).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("\"A\""));
        assert!(body.contains("\"B\""));
        assert!(!body.contains("\"C\""));
        assert!(!body.contains("\"D\""));
        assert!(body.contains("\"month\""));
        assert!(body.contains("\"02\""));
    }

    // ── DayArchiveView tests ──────────────────────────────────────────

    struct TestDayArchiveView {
        items: Vec<serde_json::Value>,
    }

    impl ContextMixin for TestDayArchiveView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
    }

    impl DateMixin for TestDayArchiveView {
        fn date_field(&self) -> &str {
            "pub_date"
        }

        fn allow_future(&self) -> bool {
            false
        }
    }

    #[async_trait]
    impl View for TestDayArchiveView {}

    #[async_trait]
    impl DayArchiveView for TestDayArchiveView {
        fn model_name(&self) -> &str {
            "article"
        }

        async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError> {
            Ok(self.items.clone())
        }
    }

    #[tokio::test]
    async fn test_day_archive_template_name() {
        let view = TestDayArchiveView { items: vec![] };
        assert_eq!(view.template_name(), "article_archive_day.html");
    }

    #[tokio::test]
    async fn test_day_archive_filters_by_day() {
        let view = TestDayArchiveView {
            items: vec![
                serde_json::json!({"title": "A", "pub_date": "2025-02-15"}),
                serde_json::json!({"title": "B", "pub_date": "2025-02-15"}),
                serde_json::json!({"title": "C", "pub_date": "2025-02-16"}),
            ],
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.day_archive(request, 2025, 2, 15).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("\"A\""));
        assert!(body.contains("\"B\""));
        assert!(!body.contains("\"C\""));
        assert!(body.contains("2025-02-15"));
    }

    #[tokio::test]
    async fn test_day_archive_invalid_date() {
        let view = TestDayArchiveView { items: vec![] };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.day_archive(request, 2025, 2, 31).await;
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_day_archive_future_date_rejected() {
        let view = TestDayArchiveView { items: vec![] };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        // Use a date far in the future
        let response = view.day_archive(request, 2099, 1, 1).await;
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    }

    // ── DayArchiveView allowing future ────────────────────────────────

    struct FutureDayArchiveView {
        items: Vec<serde_json::Value>,
    }

    impl ContextMixin for FutureDayArchiveView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
    }

    impl DateMixin for FutureDayArchiveView {
        fn date_field(&self) -> &str {
            "pub_date"
        }

        fn allow_future(&self) -> bool {
            true
        }
    }

    #[async_trait]
    impl View for FutureDayArchiveView {}

    #[async_trait]
    impl DayArchiveView for FutureDayArchiveView {
        fn model_name(&self) -> &str {
            "article"
        }

        async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError> {
            Ok(self.items.clone())
        }
    }

    #[tokio::test]
    async fn test_day_archive_future_allowed() {
        let view = FutureDayArchiveView {
            items: vec![serde_json::json!({"title": "Future", "pub_date": "2099-01-01"})],
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.day_archive(request, 2099, 1, 1).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("Future"));
    }

    // ── TodayArchiveView tests ────────────────────────────────────────

    struct TestTodayArchiveView {
        items: Vec<serde_json::Value>,
    }

    impl ContextMixin for TestTodayArchiveView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
    }

    impl DateMixin for TestTodayArchiveView {
        fn date_field(&self) -> &str {
            "pub_date"
        }
    }

    #[async_trait]
    impl View for TestTodayArchiveView {
        async fn get(&self, request: HttpRequest) -> HttpResponse {
            self.today_archive(request).await
        }
    }

    #[async_trait]
    impl TodayArchiveView for TestTodayArchiveView {
        fn model_name(&self) -> &str {
            "article"
        }

        async fn get_queryset(&self) -> Result<Vec<serde_json::Value>, DjangoError> {
            Ok(self.items.clone())
        }
    }

    #[tokio::test]
    async fn test_today_archive_template_name() {
        let view = TestTodayArchiveView { items: vec![] };
        assert_eq!(view.template_name(), "article_archive_day.html");
    }

    #[tokio::test]
    async fn test_today_archive_returns_today_objects() {
        let today_str = chrono::Utc::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        let view = TestTodayArchiveView {
            items: vec![
                serde_json::json!({"title": "Today", "pub_date": today_str.clone()}),
                serde_json::json!({"title": "Yesterday", "pub_date": "2020-01-01"}),
            ],
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("Today"));
        assert!(!body.contains("Yesterday"));
        assert!(body.contains(&today_str));
    }

    #[tokio::test]
    async fn test_today_archive_empty() {
        let view = TestTodayArchiveView { items: vec![] };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let response = view.dispatch(request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    // ── DateDetailView tests ──────────────────────────────────────────

    struct TestDateDetailView {
        object: Option<serde_json::Value>,
    }

    impl ContextMixin for TestDateDetailView {
        fn get_context_data(
            &self,
            _kwargs: &HashMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
    }

    impl DateMixin for TestDateDetailView {
        fn date_field(&self) -> &str {
            "pub_date"
        }
    }

    #[async_trait]
    impl View for TestDateDetailView {}

    #[async_trait]
    impl DateDetailView for TestDateDetailView {
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
    async fn test_date_detail_template_name() {
        let view = TestDateDetailView { object: None };
        assert_eq!(view.template_name(), "article_detail.html");
    }

    #[tokio::test]
    async fn test_date_detail_matching_date() {
        let view = TestDateDetailView {
            object: Some(serde_json::json!({
                "title": "Test Article",
                "pub_date": "2025-02-15"
            })),
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let kwargs = HashMap::new();
        let response = view.date_detail(request, 2025, 2, 15, &kwargs).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("Test Article"));
        assert!(body.contains("2025-02-15"));
    }

    #[tokio::test]
    async fn test_date_detail_mismatching_date() {
        let view = TestDateDetailView {
            object: Some(serde_json::json!({
                "title": "Test Article",
                "pub_date": "2025-02-15"
            })),
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let kwargs = HashMap::new();
        let response = view.date_detail(request, 2025, 3, 15, &kwargs).await;
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_date_detail_invalid_date() {
        let view = TestDateDetailView {
            object: Some(serde_json::json!({"pub_date": "2025-02-15"})),
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let kwargs = HashMap::new();
        let response = view.date_detail(request, 2025, 13, 1, &kwargs).await;
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_date_detail_object_not_found() {
        let view = TestDateDetailView { object: None };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let kwargs = HashMap::new();
        let response = view.date_detail(request, 2025, 2, 15, &kwargs).await;
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_date_detail_future_rejected() {
        let view = TestDateDetailView {
            object: Some(serde_json::json!({
                "title": "Future",
                "pub_date": "2099-01-01"
            })),
        };
        let request = HttpRequest::builder().method(http::Method::GET).build();
        let kwargs = HashMap::new();
        let response = view.date_detail(request, 2099, 1, 1, &kwargs).await;
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    }

    // ── DateMixin default tests ───────────────────────────────────────

    struct DefaultDateMixinView;

    impl DateMixin for DefaultDateMixinView {
        fn date_field(&self) -> &str {
            "created_at"
        }
    }

    #[test]
    fn test_date_mixin_defaults() {
        let view = DefaultDateMixinView;
        assert_eq!(view.date_field(), "created_at");
        assert!(!view.allow_future());
    }

    // ── Edge case tests ───────────────────────────────────────────────

    #[test]
    fn test_filter_by_year_with_datetime_strings() {
        let objects = vec![
            serde_json::json!({"pub_date": "2025-06-15T10:30:00"}),
            serde_json::json!({"pub_date": "2026-01-10T14:00:00"}),
        ];
        let result = filter_by_year(&objects, "pub_date", 2025);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_by_month_with_datetime_strings() {
        let objects = vec![
            serde_json::json!({"pub_date": "2025-02-15T10:30:00"}),
            serde_json::json!({"pub_date": "2025-03-10T14:00:00"}),
        ];
        let result = filter_by_month(&objects, "pub_date", 2025, 2);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_sort_by_date_desc_handles_missing_dates() {
        let mut objects = vec![
            serde_json::json!({"title": "No Date"}),
            serde_json::json!({"title": "Has Date", "pub_date": "2025-01-01"}),
        ];
        sort_by_date_desc(&mut objects, "pub_date");
        // Object with date should come first (has a value)
        assert_eq!(objects[0]["title"], "Has Date");
    }

    #[test]
    fn test_collect_date_list_with_invalid_dates() {
        let objects = vec![
            serde_json::json!({"pub_date": "not-a-date"}),
            serde_json::json!({"pub_date": "2025-01-01"}),
        ];
        let dates = collect_date_list(&objects, "pub_date");
        assert_eq!(dates.len(), 1);
        assert_eq!(dates[0], "2025-01-01");
    }
}
