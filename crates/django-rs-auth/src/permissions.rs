//! Permission and group system for django-rs.
//!
//! This module implements Role-Based Access Control (RBAC) mirroring Django's
//! permission system. Users can have:
//!
//! - **Direct permissions** assigned to their account
//! - **Group permissions** inherited from groups they belong to
//! - **Superuser access** which grants all permissions unconditionally
//!
//! Permissions use the format `"app_label.codename"` (e.g., `"blog.add_post"`).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::user::AbstractUser;

/// A single permission, identified by a codename and associated with a content type.
///
/// Mirrors Django's `auth.Permission` model. Permissions are typically auto-generated
/// for each model (add, change, delete, view) but can also be created manually.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Permission {
    /// The machine-readable identifier (e.g., "`add_post`").
    pub codename: String,
    /// The human-readable name (e.g., "Can add post").
    pub name: String,
    /// The content type this permission applies to (e.g., "blog.post").
    pub content_type: String,
}

impl Permission {
    /// Creates a new permission.
    pub fn new(
        codename: impl Into<String>,
        name: impl Into<String>,
        content_type: impl Into<String>,
    ) -> Self {
        Self {
            codename: codename.into(),
            name: name.into(),
            content_type: content_type.into(),
        }
    }

    /// Returns the full permission string in `"content_type.codename"` format.
    pub fn full_codename(&self) -> String {
        format!("{}.{}", self.content_type, self.codename)
    }
}

/// A group of users with shared permissions.
///
/// Mirrors Django's `auth.Group` model. Users inherit all permissions
/// from the groups they belong to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// The group name.
    pub name: String,
    /// Permissions assigned to this group.
    pub permissions: Vec<Permission>,
}

impl Group {
    /// Creates a new group with the given name and no permissions.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            permissions: Vec::new(),
        }
    }

    /// Adds a permission to this group.
    pub fn add_permission(&mut self, permission: Permission) {
        if !self.permissions.contains(&permission) {
            self.permissions.push(permission);
        }
    }

    /// Removes a permission from this group.
    pub fn remove_permission(&mut self, codename: &str) {
        self.permissions.retain(|p| p.codename != codename);
    }

    /// Returns all permission codenames in `"content_type.codename"` format.
    pub fn get_permissions(&self) -> HashSet<String> {
        self.permissions
            .iter()
            .map(Permission::full_codename)
            .collect()
    }
}

/// Checks if a user has a specific permission.
///
/// The permission string should be in `"app_label.codename"` format.
/// Superusers automatically have all permissions. Inactive users have no permissions.
pub fn has_perm(user: &AbstractUser, perm: &str) -> bool {
    if !user.base.is_active {
        return false;
    }
    if user.is_superuser {
        return true;
    }
    get_all_permissions(user).contains(perm)
}

/// Checks if a user has all of the given permissions.
pub fn has_perms(user: &AbstractUser, perms: &[&str]) -> bool {
    if !user.base.is_active {
        return false;
    }
    if user.is_superuser {
        return true;
    }
    let all_perms = get_all_permissions(user);
    perms.iter().all(|p| all_perms.contains(*p))
}

/// Checks if a user has any permissions for the given app/module label.
///
/// A superuser always returns `true`. For other users, checks if they have
/// at least one permission with the matching app label prefix.
pub fn has_module_perms(user: &AbstractUser, app_label: &str) -> bool {
    if !user.base.is_active {
        return false;
    }
    if user.is_superuser {
        return true;
    }
    let prefix = format!("{app_label}.");
    get_all_permissions(user)
        .iter()
        .any(|p| p.starts_with(&prefix))
}

/// Returns all permissions for a user (direct + group permissions).
pub fn get_all_permissions(user: &AbstractUser) -> HashSet<String> {
    let perms: HashSet<String> = user.user_permissions.iter().cloned().collect();

    // In a full implementation, group permissions would be loaded from the database.
    // For now, groups are represented by name only, and their permissions would need
    // to be resolved against a group registry.
    // Group permissions are added via the group_permissions parameter in specialized functions.

    perms
}

/// Returns all permissions for a user including permissions from specified groups.
///
/// This function resolves group memberships against a provided list of groups.
pub fn get_all_permissions_with_groups(user: &AbstractUser, groups: &[Group]) -> HashSet<String> {
    let mut perms = get_all_permissions(user);

    // Add permissions from groups the user belongs to
    for group in groups {
        if user.groups.contains(&group.name) {
            perms.extend(group.get_permissions());
        }
    }

    perms
}

/// Checks if a user has a specific permission, considering group memberships.
pub fn has_perm_with_groups(user: &AbstractUser, perm: &str, groups: &[Group]) -> bool {
    if !user.base.is_active {
        return false;
    }
    if user.is_superuser {
        return true;
    }
    get_all_permissions_with_groups(user, groups).contains(perm)
}

/// Generates default permissions for a model (add, change, delete, view).
///
/// Returns four `Permission` instances following Django's convention.
pub fn generate_default_permissions(app_label: &str, model_name: &str) -> Vec<Permission> {
    vec![
        Permission::new(
            format!("add_{model_name}"),
            format!("Can add {model_name}"),
            format!("{app_label}.{model_name}"),
        ),
        Permission::new(
            format!("change_{model_name}"),
            format!("Can change {model_name}"),
            format!("{app_label}.{model_name}"),
        ),
        Permission::new(
            format!("delete_{model_name}"),
            format!("Can delete {model_name}"),
            format!("{app_label}.{model_name}"),
        ),
        Permission::new(
            format!("view_{model_name}"),
            format!("Can view {model_name}"),
            format!("{app_label}.{model_name}"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user(username: &str) -> AbstractUser {
        AbstractUser::new(username)
    }

    fn make_superuser(username: &str) -> AbstractUser {
        let mut user = AbstractUser::new(username);
        user.is_superuser = true;
        user
    }

    fn make_user_with_perms(username: &str, perms: Vec<&str>) -> AbstractUser {
        let mut user = AbstractUser::new(username);
        user.user_permissions = perms.into_iter().map(String::from).collect();
        user
    }

    // ── Permission tests ────────────────────────────────────────────

    #[test]
    fn test_permission_new() {
        let perm = Permission::new("add_post", "Can add post", "blog.post");
        assert_eq!(perm.codename, "add_post");
        assert_eq!(perm.name, "Can add post");
        assert_eq!(perm.content_type, "blog.post");
    }

    #[test]
    fn test_permission_full_codename() {
        let perm = Permission::new("add_post", "Can add post", "blog.post");
        assert_eq!(perm.full_codename(), "blog.post.add_post");
    }

    #[test]
    fn test_permission_equality() {
        let p1 = Permission::new("add_post", "Can add post", "blog.post");
        let p2 = Permission::new("add_post", "Can add post", "blog.post");
        assert_eq!(p1, p2);
    }

    // ── Group tests ─────────────────────────────────────────────────

    #[test]
    fn test_group_new() {
        let group = Group::new("editors");
        assert_eq!(group.name, "editors");
        assert!(group.permissions.is_empty());
    }

    #[test]
    fn test_group_add_permission() {
        let mut group = Group::new("editors");
        let perm = Permission::new("change_post", "Can change post", "blog.post");
        group.add_permission(perm);
        assert_eq!(group.permissions.len(), 1);
    }

    #[test]
    fn test_group_add_duplicate_permission() {
        let mut group = Group::new("editors");
        let perm = Permission::new("change_post", "Can change post", "blog.post");
        group.add_permission(perm.clone());
        group.add_permission(perm);
        assert_eq!(group.permissions.len(), 1);
    }

    #[test]
    fn test_group_remove_permission() {
        let mut group = Group::new("editors");
        group.add_permission(Permission::new(
            "change_post",
            "Can change post",
            "blog.post",
        ));
        group.add_permission(Permission::new("add_post", "Can add post", "blog.post"));
        group.remove_permission("change_post");
        assert_eq!(group.permissions.len(), 1);
        assert_eq!(group.permissions[0].codename, "add_post");
    }

    #[test]
    fn test_group_get_permissions() {
        let mut group = Group::new("editors");
        group.add_permission(Permission::new(
            "change_post",
            "Can change post",
            "blog.post",
        ));
        group.add_permission(Permission::new("add_post", "Can add post", "blog.post"));
        let perms = group.get_permissions();
        assert_eq!(perms.len(), 2);
        assert!(perms.contains("blog.post.change_post"));
        assert!(perms.contains("blog.post.add_post"));
    }

    // ── has_perm tests ──────────────────────────────────────────────

    #[test]
    fn test_has_perm_no_perms() {
        let user = make_user("alice");
        assert!(!has_perm(&user, "blog.add_post"));
    }

    #[test]
    fn test_has_perm_direct_perm() {
        let user = make_user_with_perms("alice", vec!["blog.add_post"]);
        assert!(has_perm(&user, "blog.add_post"));
        assert!(!has_perm(&user, "blog.delete_post"));
    }

    #[test]
    fn test_has_perm_superuser() {
        let user = make_superuser("admin");
        assert!(has_perm(&user, "any.permission"));
        assert!(has_perm(&user, "another.permission"));
    }

    #[test]
    fn test_has_perm_inactive_user() {
        let mut user = make_superuser("admin");
        user.base.is_active = false;
        assert!(!has_perm(&user, "any.permission"));
    }

    // ── has_perms tests ─────────────────────────────────────────────

    #[test]
    fn test_has_perms_all() {
        let user = make_user_with_perms("alice", vec!["blog.add_post", "blog.change_post"]);
        assert!(has_perms(&user, &["blog.add_post", "blog.change_post"]));
    }

    #[test]
    fn test_has_perms_missing_one() {
        let user = make_user_with_perms("alice", vec!["blog.add_post"]);
        assert!(!has_perms(&user, &["blog.add_post", "blog.change_post"]));
    }

    #[test]
    fn test_has_perms_empty() {
        let user = make_user("alice");
        assert!(has_perms(&user, &[]));
    }

    #[test]
    fn test_has_perms_superuser() {
        let user = make_superuser("admin");
        assert!(has_perms(&user, &["any.perm", "other.perm"]));
    }

    // ── has_module_perms tests ──────────────────────────────────────

    #[test]
    fn test_has_module_perms_no_perms() {
        let user = make_user("alice");
        assert!(!has_module_perms(&user, "blog"));
    }

    #[test]
    fn test_has_module_perms_with_perm() {
        let user = make_user_with_perms("alice", vec!["blog.add_post"]);
        assert!(has_module_perms(&user, "blog"));
        assert!(!has_module_perms(&user, "auth"));
    }

    #[test]
    fn test_has_module_perms_superuser() {
        let user = make_superuser("admin");
        assert!(has_module_perms(&user, "blog"));
        assert!(has_module_perms(&user, "any_module"));
    }

    #[test]
    fn test_has_module_perms_inactive() {
        let mut user = make_user_with_perms("alice", vec!["blog.add_post"]);
        user.base.is_active = false;
        assert!(!has_module_perms(&user, "blog"));
    }

    // ── get_all_permissions tests ───────────────────────────────────

    #[test]
    fn test_get_all_permissions_empty() {
        let user = make_user("alice");
        assert!(get_all_permissions(&user).is_empty());
    }

    #[test]
    fn test_get_all_permissions_direct() {
        let user = make_user_with_perms("alice", vec!["blog.add_post", "blog.change_post"]);
        let perms = get_all_permissions(&user);
        assert_eq!(perms.len(), 2);
        assert!(perms.contains("blog.add_post"));
        assert!(perms.contains("blog.change_post"));
    }

    // ── get_all_permissions_with_groups tests ───────────────────────

    #[test]
    fn test_permissions_with_groups() {
        let mut user = make_user("alice");
        user.groups = vec!["editors".to_string()];

        let mut editors = Group::new("editors");
        editors.add_permission(Permission::new("change_post", "Can change", "blog.post"));

        let perms = get_all_permissions_with_groups(&user, &[editors]);
        assert!(perms.contains("blog.post.change_post"));
    }

    #[test]
    fn test_permissions_with_groups_and_direct() {
        let mut user = make_user_with_perms("alice", vec!["blog.add_post"]);
        user.groups = vec!["editors".to_string()];

        let mut editors = Group::new("editors");
        editors.add_permission(Permission::new("change_post", "Can change", "blog.post"));

        let perms = get_all_permissions_with_groups(&user, &[editors]);
        assert!(perms.contains("blog.add_post"));
        assert!(perms.contains("blog.post.change_post"));
    }

    #[test]
    fn test_permissions_with_non_member_group() {
        let user = make_user("alice");
        let mut editors = Group::new("editors");
        editors.add_permission(Permission::new("change_post", "Can change", "blog.post"));

        let perms = get_all_permissions_with_groups(&user, &[editors]);
        assert!(perms.is_empty());
    }

    // ── has_perm_with_groups tests ──────────────────────────────────

    #[test]
    fn test_has_perm_with_groups() {
        let mut user = make_user("alice");
        user.groups = vec!["editors".to_string()];

        let mut editors = Group::new("editors");
        editors.add_permission(Permission::new("change_post", "Can change", "blog.post"));

        assert!(has_perm_with_groups(
            &user,
            "blog.post.change_post",
            &[editors]
        ));
    }

    #[test]
    fn test_has_perm_with_groups_inactive() {
        let mut user = make_user("alice");
        user.base.is_active = false;
        user.groups = vec!["editors".to_string()];

        let mut editors = Group::new("editors");
        editors.add_permission(Permission::new("change_post", "Can change", "blog.post"));

        assert!(!has_perm_with_groups(
            &user,
            "blog.post.change_post",
            &[editors]
        ));
    }

    // ── generate_default_permissions tests ──────────────────────────

    #[test]
    fn test_generate_default_permissions() {
        let perms = generate_default_permissions("blog", "post");
        assert_eq!(perms.len(), 4);

        let codenames: Vec<&str> = perms.iter().map(|p| p.codename.as_str()).collect();
        assert!(codenames.contains(&"add_post"));
        assert!(codenames.contains(&"change_post"));
        assert!(codenames.contains(&"delete_post"));
        assert!(codenames.contains(&"view_post"));

        for perm in &perms {
            assert_eq!(perm.content_type, "blog.post");
        }
    }

    #[test]
    fn test_generate_default_permissions_names() {
        let perms = generate_default_permissions("blog", "post");
        let names: Vec<&str> = perms.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Can add post"));
        assert!(names.contains(&"Can change post"));
        assert!(names.contains(&"Can delete post"));
        assert!(names.contains(&"Can view post"));
    }
}
