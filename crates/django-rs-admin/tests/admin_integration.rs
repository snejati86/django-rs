//! Integration tests for the admin site, InMemoryAdminDb CRUD, search, filtering,
//! ordering, pagination, and LogEntry audit trail.

use std::collections::HashMap;

use django_rs_admin::db::{AdminDbExecutor, AdminListParams, InMemoryAdminDb};
use django_rs_admin::log_entry::{ActionFlag, InMemoryLogEntryStore, LogEntryStore};
use django_rs_admin::model_admin::{FieldSchema, ModelAdmin};
use django_rs_admin::site::AdminSite;

// ── Helpers ─────────────────────────────────────────────────────────

fn article_admin() -> ModelAdmin {
    ModelAdmin::new("blog", "article")
        .fields_schema(vec![
            FieldSchema::new("id", "BigAutoField").primary_key(),
            FieldSchema::new("title", "CharField").max_length(200),
            FieldSchema::new("body", "TextField").optional(),
            FieldSchema::new("status", "CharField").max_length(20),
            FieldSchema::new("priority", "IntegerField"),
        ])
        .search_fields(vec!["title", "body"])
        .list_filter_fields(vec!["status"])
        .ordering(vec!["-id"])
        .list_per_page(5)
}

async fn seed_articles(db: &InMemoryAdminDb, admin: &ModelAdmin, count: usize) {
    for i in 1..=count {
        let mut data = HashMap::new();
        data.insert(
            "title".to_string(),
            serde_json::json!(format!("Article {i}")),
        );
        data.insert(
            "body".to_string(),
            serde_json::json!(format!("Body text {i}")),
        );
        data.insert(
            "status".to_string(),
            serde_json::json!(if i % 2 == 0 { "published" } else { "draft" }),
        );
        data.insert("priority".to_string(), serde_json::json!(i));
        db.create_object(admin, &data).await.unwrap();
    }
}

// ═════════════════════════════════════════════════════════════════════
// 1. Register a model with AdminSite
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_register_model_with_admin_site() {
    let mut site = AdminSite::new("admin");
    let admin = article_admin();
    site.register("blog.article", admin);

    assert!(site.is_registered("blog.article"));
    assert_eq!(site.model_count(), 1);

    let retrieved = site.get_model_admin("blog.article").unwrap();
    assert_eq!(retrieved.app_label, "blog");
    assert_eq!(retrieved.model_name, "article");
    assert_eq!(retrieved.list_per_page, 5);
    assert_eq!(retrieved.search_fields, vec!["title", "body"]);
}

// ═════════════════════════════════════════════════════════════════════
// 2. InMemoryAdminDb: create object, verify it exists
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_create_object_exists_in_db() {
    let db = InMemoryAdminDb::new();
    let admin = article_admin();

    let mut data = HashMap::new();
    data.insert("title".to_string(), serde_json::json!("First Post"));
    data.insert("status".to_string(), serde_json::json!("draft"));

    let created = db.create_object(&admin, &data).await.unwrap();
    assert_eq!(created["id"], 1);
    assert_eq!(created["title"], "First Post");
    assert_eq!(db.count("blog.article"), 1);

    // Verify via get_object
    let fetched = db.get_object(&admin, "1").await.unwrap();
    assert_eq!(fetched["title"], "First Post");
}

// ═════════════════════════════════════════════════════════════════════
// 3. InMemoryAdminDb: list objects returns all created
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_list_objects_returns_all_created() {
    let db = InMemoryAdminDb::new();
    let admin = article_admin();
    seed_articles(&db, &admin, 7).await;

    let all = db.all_objects("blog.article");
    assert_eq!(all.len(), 7);

    let params = AdminListParams::new().page_size(100);
    let result = db.list_objects(&admin, &params).await.unwrap();
    assert_eq!(result.response.count, 7);
}

// ═════════════════════════════════════════════════════════════════════
// 4. InMemoryAdminDb: get object by PK
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_get_object_by_pk() {
    let db = InMemoryAdminDb::new();
    let admin = article_admin();
    seed_articles(&db, &admin, 3).await;

    let obj = db.get_object(&admin, "2").await.unwrap();
    assert_eq!(obj["title"], "Article 2");
    assert_eq!(obj["id"], 2);

    // Non-existent PK returns error
    let err = db.get_object(&admin, "999").await;
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("not found"));
}

// ═════════════════════════════════════════════════════════════════════
// 5. InMemoryAdminDb: update object changes fields
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_update_object_changes_fields() {
    let db = InMemoryAdminDb::new();
    let admin = article_admin();
    seed_articles(&db, &admin, 1).await;

    let mut update = HashMap::new();
    update.insert("title".to_string(), serde_json::json!("Updated Title"));
    update.insert("status".to_string(), serde_json::json!("published"));

    let updated = db.update_object(&admin, "1", &update).await.unwrap();
    assert_eq!(updated["title"], "Updated Title");
    assert_eq!(updated["status"], "published");
    // PK must not change
    assert_eq!(updated["id"], 1);

    // Verify persistence
    let fetched = db.get_object(&admin, "1").await.unwrap();
    assert_eq!(fetched["title"], "Updated Title");
}

// ═════════════════════════════════════════════════════════════════════
// 6. InMemoryAdminDb: delete object removes it
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_delete_object_removes_it() {
    let db = InMemoryAdminDb::new();
    let admin = article_admin();
    seed_articles(&db, &admin, 3).await;
    assert_eq!(db.count("blog.article"), 3);

    let deleted = db.delete_object(&admin, "2").await.unwrap();
    assert!(deleted);
    assert_eq!(db.count("blog.article"), 2);

    // Verify PK 2 no longer accessible
    assert!(db.get_object(&admin, "2").await.is_err());

    // Deleting non-existent PK returns false
    let not_deleted = db.delete_object(&admin, "2").await.unwrap();
    assert!(!not_deleted);
}

// ═════════════════════════════════════════════════════════════════════
// 7. Admin search: search by field value finds matches
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_search_finds_matching_objects() {
    let db = InMemoryAdminDb::new();
    let admin = article_admin();

    let mut d1 = HashMap::new();
    d1.insert(
        "title".to_string(),
        serde_json::json!("Rust Programming Guide"),
    );
    d1.insert("body".to_string(), serde_json::json!("Learn Rust"));
    db.create_object(&admin, &d1).await.unwrap();

    let mut d2 = HashMap::new();
    d2.insert("title".to_string(), serde_json::json!("Python Basics"));
    d2.insert("body".to_string(), serde_json::json!("Learn Python"));
    db.create_object(&admin, &d2).await.unwrap();

    let mut d3 = HashMap::new();
    d3.insert(
        "title".to_string(),
        serde_json::json!("Advanced Rust Patterns"),
    );
    d3.insert("body".to_string(), serde_json::json!("Macros and traits"));
    db.create_object(&admin, &d3).await.unwrap();

    let params = AdminListParams::new().search("rust");
    let result = db.list_objects(&admin, &params).await.unwrap();
    assert_eq!(result.response.count, 2);
    // All matches should contain "rust" in title or body (case-insensitive)
    for obj in &result.response.results {
        let title = obj["title"].as_str().unwrap_or_default().to_lowercase();
        let body = obj
            .get("body")
            .and_then(|b| b.as_str())
            .unwrap_or_default()
            .to_lowercase();
        assert!(title.contains("rust") || body.contains("rust"));
    }
}

// ═════════════════════════════════════════════════════════════════════
// 8. Admin search: no matches returns empty
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_search_no_matches_returns_empty() {
    let db = InMemoryAdminDb::new();
    let admin = article_admin();
    seed_articles(&db, &admin, 5).await;

    let params = AdminListParams::new().search("zzzznonexistent");
    let result = db.list_objects(&admin, &params).await.unwrap();
    assert_eq!(result.response.count, 0);
    assert!(result.response.results.is_empty());
}

// ═════════════════════════════════════════════════════════════════════
// 9. Admin filtering: filter by field value
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_filter_by_field_value() {
    let db = InMemoryAdminDb::new();
    let admin = article_admin();
    seed_articles(&db, &admin, 10).await;

    // Even articles are "published", odd are "draft"
    let params = AdminListParams::new()
        .page_size(100)
        .filter("status", "published");
    let result = db.list_objects(&admin, &params).await.unwrap();
    assert_eq!(result.response.count, 5); // articles 2,4,6,8,10

    for obj in &result.response.results {
        assert_eq!(obj["status"], "published");
    }
}

// ═════════════════════════════════════════════════════════════════════
// 10. Admin ordering: ascending and descending
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_ordering_ascending_and_descending() {
    let db = InMemoryAdminDb::new();
    let admin = article_admin();
    seed_articles(&db, &admin, 5).await;

    // Ascending by title
    let params_asc = AdminListParams::new().page_size(100).ordering("title");
    let result_asc = db.list_objects(&admin, &params_asc).await.unwrap();
    let titles_asc: Vec<&str> = result_asc
        .response
        .results
        .iter()
        .map(|o| o["title"].as_str().unwrap())
        .collect();
    let mut sorted = titles_asc.clone();
    sorted.sort();
    assert_eq!(titles_asc, sorted);

    // Descending by title
    let params_desc = AdminListParams::new().page_size(100).ordering("-title");
    let result_desc = db.list_objects(&admin, &params_desc).await.unwrap();
    let titles_desc: Vec<&str> = result_desc
        .response
        .results
        .iter()
        .map(|o| o["title"].as_str().unwrap())
        .collect();
    let mut sorted_rev = titles_desc.clone();
    sorted_rev.sort();
    sorted_rev.reverse();
    assert_eq!(titles_desc, sorted_rev);
}

// ═════════════════════════════════════════════════════════════════════
// 11. Admin pagination: first page, second page, correct counts
// ═════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_pagination_pages_and_counts() {
    let db = InMemoryAdminDb::new();
    let admin = article_admin(); // list_per_page = 5
    seed_articles(&db, &admin, 12).await;

    // Page 1 (page_size 5)
    let p1 = AdminListParams::new().page(1).page_size(5);
    let r1 = db.list_objects(&admin, &p1).await.unwrap();
    assert_eq!(r1.response.count, 12);
    assert_eq!(r1.response.results.len(), 5);
    assert_eq!(r1.response.total_pages, 3);
    assert!(r1.response.has_next);
    assert!(!r1.response.has_previous);

    // Page 2
    let p2 = AdminListParams::new().page(2).page_size(5);
    let r2 = db.list_objects(&admin, &p2).await.unwrap();
    assert_eq!(r2.response.results.len(), 5);
    assert!(r2.response.has_next);
    assert!(r2.response.has_previous);

    // Page 3 (last page, partial)
    let p3 = AdminListParams::new().page(3).page_size(5);
    let r3 = db.list_objects(&admin, &p3).await.unwrap();
    assert_eq!(r3.response.results.len(), 2);
    assert!(!r3.response.has_next);
    assert!(r3.response.has_previous);
}

// ═════════════════════════════════════════════════════════════════════
// 12. LogEntry: log_addition creates entry
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_log_addition_creates_entry() {
    let store = InMemoryLogEntryStore::new();
    let entry = store.log_addition(1, "blog.article", "42", "My Article", "Created via admin");

    assert!(entry.is_addition());
    assert_eq!(entry.action_flag, ActionFlag::Addition);
    assert_eq!(entry.content_type, "blog.article");
    assert_eq!(entry.object_id, "42");
    assert_eq!(entry.object_repr, "My Article");
    assert_eq!(entry.change_message, "Created via admin");
    assert_eq!(entry.user_id, 1);
    assert_eq!(store.count(), 1);
}

// ═════════════════════════════════════════════════════════════════════
// 13. LogEntry: log_change creates entry
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_log_change_creates_entry() {
    let store = InMemoryLogEntryStore::new();
    let entry = store.log_change(2, "blog.article", "7", "Article #7", "Changed title, body");

    assert!(entry.is_change());
    assert_eq!(entry.action_flag, ActionFlag::Change);
    assert_eq!(entry.user_id, 2);
    assert_eq!(entry.change_message, "Changed title, body");
    assert_eq!(
        entry.description(),
        "Change: Article #7 - Changed title, body"
    );
}

// ═════════════════════════════════════════════════════════════════════
// 14. LogEntry: log_deletion creates entry
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_log_deletion_creates_entry() {
    let store = InMemoryLogEntryStore::new();
    let entry = store.log_deletion(1, "blog.article", "99", "Deleted Article", "");

    assert!(entry.is_deletion());
    assert_eq!(entry.action_flag, ActionFlag::Deletion);
    assert_eq!(entry.description(), "Deletion: Deleted Article");
}

// ═════════════════════════════════════════════════════════════════════
// 15. LogEntry: get_for_object returns full history
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_log_get_for_object_returns_history() {
    let store = InMemoryLogEntryStore::new();

    // Simulate lifecycle of object "5" in blog.article
    store.log_addition(1, "blog.article", "5", "Article 5", "Created");
    store.log_change(1, "blog.article", "5", "Article 5", "Changed title");
    store.log_change(2, "blog.article", "5", "Article 5", "Changed body");
    store.log_deletion(1, "blog.article", "5", "Article 5", "");

    // Also log actions on a different object to verify isolation
    store.log_addition(1, "blog.article", "6", "Article 6", "Created");

    let history = store.get_for_object("blog.article", "5");
    assert_eq!(history.len(), 4);
    // Newest first
    assert!(history[0].is_deletion());
    assert!(history[3].is_addition());

    // Different object should not be included
    let other = store.get_for_object("blog.article", "6");
    assert_eq!(other.len(), 1);
    assert!(other[0].is_addition());

    // Total store count
    assert_eq!(store.count(), 5);

    // get_for_user
    let user1_entries = store.get_for_user(1);
    assert_eq!(user1_entries.len(), 4);
    let user2_entries = store.get_for_user(2);
    assert_eq!(user2_entries.len(), 1);

    // get_by_action
    let changes = store.get_by_action(ActionFlag::Change);
    assert_eq!(changes.len(), 2);
}
