//! User models for django-rs authentication.
//!
//! This module provides user abstractions mirroring Django's auth user system:
//!
//! - [`AbstractBaseUser`] - Minimal user with password and login tracking
//! - [`AbstractUser`] - Full-featured user with username, email, groups, permissions
//! - [`AnonymousUser`] - Represents an unauthenticated user
//!
//! ## Async Password Operations
//!
//! Password hashing and verification are async operations that use
//! `tokio::task::spawn_blocking` internally to avoid blocking the runtime.

use chrono::{DateTime, Utc};
use django_rs_core::error::DjangoError;
use serde::{Deserialize, Serialize};

/// Base user model with password and login tracking.
///
/// Mirrors Django's `AbstractBaseUser`. Contains only the essential fields
/// needed for authentication: password hash, last login time, and active status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbstractBaseUser {
    /// The hashed password. May be set to an unusable value (prefixed with `!`).
    pub password: String,
    /// Timestamp of the last successful login, or `None` if never logged in.
    pub last_login: Option<DateTime<Utc>>,
    /// Whether this user account is active. Inactive accounts cannot log in.
    pub is_active: bool,
}

impl Default for AbstractBaseUser {
    fn default() -> Self {
        Self {
            password: String::new(),
            last_login: None,
            is_active: true,
        }
    }
}

impl AbstractBaseUser {
    /// Creates a new base user with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the user's password by hashing it asynchronously.
    ///
    /// The password is hashed using the preferred hasher (Argon2 by default)
    /// via `spawn_blocking` to avoid blocking the async runtime.
    pub async fn set_password(&mut self, raw_password: &str) -> Result<(), DjangoError> {
        self.password = crate::hashers::make_password(raw_password).await?;
        Ok(())
    }

    /// Checks if the given raw password matches the stored hash.
    ///
    /// Returns `false` if the password is unusable (starts with `!`).
    pub async fn check_password(&self, raw_password: &str) -> Result<bool, DjangoError> {
        crate::hashers::check_password(raw_password, &self.password).await
    }

    /// Sets the password to an unusable value.
    ///
    /// After calling this, `check_password` will always return `false` and
    /// `has_usable_password` will return `false`.
    pub fn set_unusable_password(&mut self) {
        use rand::RngCore;
        use std::fmt::Write;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let random: String = bytes.iter().fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        });
        self.password = format!("!{random}");
    }

    /// Returns `true` if this user has a usable password.
    pub fn has_usable_password(&self) -> bool {
        crate::hashers::is_password_usable(&self.password)
    }
}

/// Full-featured user model with identity fields, groups, and permissions.
///
/// Mirrors Django's `AbstractUser`. Extends [`AbstractBaseUser`] with
/// username, name, email, staff/superuser flags, and permission assignments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbstractUser {
    /// Base user fields (password, `last_login`, `is_active`).
    #[serde(flatten)]
    pub base: AbstractBaseUser,
    /// The user's unique username.
    pub username: String,
    /// The user's first name.
    pub first_name: String,
    /// The user's last name.
    pub last_name: String,
    /// The user's email address.
    pub email: String,
    /// Whether this user can access the admin site.
    pub is_staff: bool,
    /// Whether this user has all permissions (superuser).
    pub is_superuser: bool,
    /// When this user account was created.
    pub date_joined: DateTime<Utc>,
    /// Group names this user belongs to.
    pub groups: Vec<String>,
    /// Permission codenames directly assigned to this user.
    pub user_permissions: Vec<String>,
}

impl Default for AbstractUser {
    fn default() -> Self {
        Self {
            base: AbstractBaseUser::default(),
            username: String::new(),
            first_name: String::new(),
            last_name: String::new(),
            email: String::new(),
            is_staff: false,
            is_superuser: false,
            date_joined: Utc::now(),
            groups: Vec::new(),
            user_permissions: Vec::new(),
        }
    }
}

impl AbstractUser {
    /// Creates a new user with the given username and default values.
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            ..Self::default()
        }
    }

    /// Sets the user's password by hashing it asynchronously.
    pub async fn set_password(&mut self, raw_password: &str) -> Result<(), DjangoError> {
        self.base.set_password(raw_password).await
    }

    /// Checks if the given raw password matches the stored hash.
    pub async fn check_password(&self, raw_password: &str) -> Result<bool, DjangoError> {
        self.base.check_password(raw_password).await
    }

    /// Returns the username.
    pub fn get_username(&self) -> &str {
        &self.username
    }

    /// Returns `true` if this user is authenticated (always `true` for real users).
    pub const fn is_authenticated(&self) -> bool {
        true
    }

    /// Returns `true` if this user is anonymous (always `false` for real users).
    pub const fn is_anonymous(&self) -> bool {
        false
    }

    /// Returns the user's full name (first + last).
    pub fn get_full_name(&self) -> String {
        let full = format!("{} {}", self.first_name, self.last_name);
        full.trim().to_string()
    }

    /// Returns the user's short name (first name).
    pub fn get_short_name(&self) -> &str {
        &self.first_name
    }

    /// Checks if the user has a specific permission.
    ///
    /// Superusers automatically have all permissions.
    pub fn has_perm(&self, perm: &str) -> bool {
        crate::permissions::has_perm(self, perm)
    }

    /// Checks if the user has all of the given permissions.
    pub fn has_perms(&self, perms: &[&str]) -> bool {
        crate::permissions::has_perms(self, perms)
    }

    /// Checks if the user has any permissions in the given app/module.
    pub fn has_module_perms(&self, app_label: &str) -> bool {
        crate::permissions::has_module_perms(self, app_label)
    }
}

/// Represents an unauthenticated (anonymous) user.
///
/// Always returns `false` for `is_authenticated()` and `true` for `is_anonymous()`.
/// Has no permissions and cannot have a password set.
#[derive(Debug, Clone, Default)]
pub struct AnonymousUser;

impl AnonymousUser {
    /// Creates a new anonymous user.
    pub const fn new() -> Self {
        Self
    }

    /// Returns `false` - anonymous users are not authenticated.
    pub const fn is_authenticated(&self) -> bool {
        false
    }

    /// Returns `true` - this is an anonymous user.
    pub const fn is_anonymous(&self) -> bool {
        true
    }

    /// Returns an empty string - anonymous users have no username.
    pub const fn get_username(&self) -> &'static str {
        ""
    }

    /// Returns `false` - anonymous users have no permissions.
    pub const fn has_perm(&self, _perm: &str) -> bool {
        false
    }

    /// Returns `false` - anonymous users have no permissions.
    pub const fn has_perms(&self, _perms: &[&str]) -> bool {
        false
    }

    /// Returns `false` - anonymous users have no module permissions.
    pub const fn has_module_perms(&self, _app_label: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── AbstractBaseUser tests ──────────────────────────────────────

    #[test]
    fn test_base_user_defaults() {
        let user = AbstractBaseUser::new();
        assert!(user.password.is_empty());
        assert!(user.last_login.is_none());
        assert!(user.is_active);
    }

    #[tokio::test]
    async fn test_base_user_set_password() {
        let mut user = AbstractBaseUser::new();
        user.set_password("mysecretpassword").await.unwrap();
        assert!(!user.password.is_empty());
        assert!(user.has_usable_password());
    }

    #[tokio::test]
    async fn test_base_user_check_password() {
        let mut user = AbstractBaseUser::new();
        user.set_password("mysecretpassword").await.unwrap();
        assert!(user.check_password("mysecretpassword").await.unwrap());
        assert!(!user.check_password("wrongpassword").await.unwrap());
    }

    #[test]
    fn test_base_user_unusable_password() {
        let mut user = AbstractBaseUser::new();
        user.set_unusable_password();
        assert!(!user.has_usable_password());
        assert!(user.password.starts_with('!'));
    }

    #[tokio::test]
    async fn test_base_user_unusable_password_check() {
        let mut user = AbstractBaseUser::new();
        user.set_unusable_password();
        assert!(!user.check_password("anything").await.unwrap());
    }

    // ── AbstractUser tests ──────────────────────────────────────────

    #[test]
    fn test_abstract_user_new() {
        let user = AbstractUser::new("alice");
        assert_eq!(user.username, "alice");
        assert!(user.first_name.is_empty());
        assert!(user.last_name.is_empty());
        assert!(user.email.is_empty());
        assert!(!user.is_staff);
        assert!(!user.is_superuser);
        assert!(user.groups.is_empty());
        assert!(user.user_permissions.is_empty());
    }

    #[test]
    fn test_abstract_user_get_username() {
        let user = AbstractUser::new("bob");
        assert_eq!(user.get_username(), "bob");
    }

    #[test]
    fn test_abstract_user_is_authenticated() {
        let user = AbstractUser::new("alice");
        assert!(user.is_authenticated());
    }

    #[test]
    fn test_abstract_user_is_not_anonymous() {
        let user = AbstractUser::new("alice");
        assert!(!user.is_anonymous());
    }

    #[test]
    fn test_abstract_user_full_name() {
        let mut user = AbstractUser::new("alice");
        user.first_name = "Alice".to_string();
        user.last_name = "Smith".to_string();
        assert_eq!(user.get_full_name(), "Alice Smith");
    }

    #[test]
    fn test_abstract_user_full_name_first_only() {
        let mut user = AbstractUser::new("alice");
        user.first_name = "Alice".to_string();
        assert_eq!(user.get_full_name(), "Alice");
    }

    #[test]
    fn test_abstract_user_short_name() {
        let mut user = AbstractUser::new("alice");
        user.first_name = "Alice".to_string();
        assert_eq!(user.get_short_name(), "Alice");
    }

    #[tokio::test]
    async fn test_abstract_user_set_and_check_password() {
        let mut user = AbstractUser::new("alice");
        user.set_password("strong_pass_123").await.unwrap();
        assert!(user.check_password("strong_pass_123").await.unwrap());
        assert!(!user.check_password("wrong_pass").await.unwrap());
    }

    #[test]
    fn test_abstract_user_superuser_has_all_perms() {
        let mut user = AbstractUser::new("admin");
        user.is_superuser = true;
        assert!(user.has_perm("any.permission"));
        assert!(user.has_perms(&["perm1", "perm2", "perm3"]));
        assert!(user.has_module_perms("any_app"));
    }

    #[test]
    fn test_abstract_user_regular_user_no_perms() {
        let user = AbstractUser::new("regular");
        assert!(!user.has_perm("blog.add_post"));
    }

    #[test]
    fn test_abstract_user_direct_permissions() {
        let mut user = AbstractUser::new("editor");
        user.user_permissions = vec!["blog.add_post".to_string(), "blog.change_post".to_string()];
        assert!(user.has_perm("blog.add_post"));
        assert!(user.has_perm("blog.change_post"));
        assert!(!user.has_perm("blog.delete_post"));
    }

    #[test]
    fn test_abstract_user_inactive_no_perms() {
        let mut user = AbstractUser::new("inactive");
        user.base.is_active = false;
        user.is_superuser = true;
        assert!(!user.has_perm("any.permission"));
    }

    #[test]
    fn test_abstract_user_default() {
        let user = AbstractUser::default();
        assert!(user.username.is_empty());
        assert!(user.base.is_active);
    }

    // ── AnonymousUser tests ─────────────────────────────────────────

    #[test]
    fn test_anonymous_user_not_authenticated() {
        let user = AnonymousUser::new();
        assert!(!user.is_authenticated());
    }

    #[test]
    fn test_anonymous_user_is_anonymous() {
        let user = AnonymousUser::new();
        assert!(user.is_anonymous());
    }

    #[test]
    fn test_anonymous_user_no_username() {
        let user = AnonymousUser::new();
        assert_eq!(user.get_username(), "");
    }

    #[test]
    fn test_anonymous_user_no_perms() {
        let user = AnonymousUser::new();
        assert!(!user.has_perm("any.perm"));
        assert!(!user.has_perms(&["perm1"]));
        assert!(!user.has_module_perms("any_app"));
    }
}
