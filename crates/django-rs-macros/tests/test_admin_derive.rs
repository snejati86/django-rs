//! Integration tests for `#[derive(Admin)]`.
//!
//! These tests verify that the generated admin configuration methods
//! produce correct values.

use django_rs_macros::Admin;

// ── Admin with all options ──────────────────────────────────────────────

#[derive(Admin)]
#[admin(
    list_display = ["title", "author", "published", "created_at"],
    list_filter = ["published", "created_at"],
    search_fields = ["title", "body"],
    ordering = ["-created_at"],
    list_per_page = 25
)]
pub struct PostAdmin;

#[test]
fn test_post_admin_list_display() {
    let display = PostAdmin::list_display();
    assert_eq!(display, vec!["title", "author", "published", "created_at"]);
}

#[test]
fn test_post_admin_list_filter() {
    let filter = PostAdmin::list_filter();
    assert_eq!(filter, vec!["published", "created_at"]);
}

#[test]
fn test_post_admin_search_fields() {
    let search = PostAdmin::search_fields();
    assert_eq!(search, vec!["title", "body"]);
}

#[test]
fn test_post_admin_ordering() {
    let ordering = PostAdmin::ordering();
    assert_eq!(ordering.len(), 1);
    assert_eq!(ordering[0].0, "created_at");
    assert!(ordering[0].1, "Should be descending");
}

#[test]
fn test_post_admin_list_per_page() {
    assert_eq!(PostAdmin::list_per_page(), 25);
}

// ── Admin with defaults ─────────────────────────────────────────────────

#[derive(Admin)]
#[admin]
pub struct MinimalAdmin;

#[test]
fn test_minimal_admin_defaults() {
    assert!(MinimalAdmin::list_display().is_empty());
    assert!(MinimalAdmin::list_filter().is_empty());
    assert!(MinimalAdmin::search_fields().is_empty());
    assert!(MinimalAdmin::ordering().is_empty());
    assert_eq!(MinimalAdmin::list_per_page(), 100); // default
    assert!(MinimalAdmin::readonly_fields().is_empty());
    assert!(MinimalAdmin::list_display_links().is_empty());
    assert!(MinimalAdmin::list_editable().is_empty());
    assert_eq!(MinimalAdmin::date_hierarchy(), None);
}

// ── Admin with extended options ─────────────────────────────────────────

#[derive(Admin)]
#[admin(
    list_display = ["username", "email", "is_active"],
    list_display_links = ["username"],
    list_editable = ["is_active"],
    readonly_fields = ["date_joined"],
    date_hierarchy = "date_joined",
    list_per_page = 50
)]
pub struct UserAdmin;

#[test]
fn test_user_admin_list_display_links() {
    assert_eq!(UserAdmin::list_display_links(), vec!["username"]);
}

#[test]
fn test_user_admin_list_editable() {
    assert_eq!(UserAdmin::list_editable(), vec!["is_active"]);
}

#[test]
fn test_user_admin_readonly_fields() {
    assert_eq!(UserAdmin::readonly_fields(), vec!["date_joined"]);
}

#[test]
fn test_user_admin_date_hierarchy() {
    assert_eq!(UserAdmin::date_hierarchy(), Some("date_joined".to_string()));
}

#[test]
fn test_user_admin_list_per_page() {
    assert_eq!(UserAdmin::list_per_page(), 50);
}

// ── Admin with multiple ordering ────────────────────────────────────────

#[derive(Admin)]
#[admin(ordering = ["-priority", "name", "-created_at"])]
pub struct TaskAdmin;

#[test]
fn test_task_admin_multi_ordering() {
    let ordering = TaskAdmin::ordering();
    assert_eq!(ordering.len(), 3);
    assert_eq!(ordering[0], ("priority", true)); // descending
    assert_eq!(ordering[1], ("name", false)); // ascending
    assert_eq!(ordering[2], ("created_at", true)); // descending
}
