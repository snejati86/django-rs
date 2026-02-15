//! # django-rs-auth
//!
//! Authentication and authorization framework for django-rs.
//!
//! This crate provides a complete auth layer modeled after Django's `django.contrib.auth`,
//! including:
//!
//! - **Password hashing** with Argon2, bcrypt, and PBKDF2 backends (`hashers`)
//! - **User models** mirroring `AbstractBaseUser` and `AbstractUser` (`user`)
//! - **Authentication backends** for pluggable credential verification (`backends`)
//! - **Permission and group system** with RBAC support (`permissions`)
//! - **CSRF protection middleware** (`csrf`)
//! - **Security middleware** for host validation and security headers (`security`)
//! - **Auth view configuration types** and token generators (`views`)
//!
//! ## Design Principles
//!
//! All CPU-bound cryptographic operations (password hashing, token generation) are
//! executed via `tokio::task::spawn_blocking` to avoid blocking the async runtime.
//! All traits are `Send + Sync` to enable safe concurrent access.

pub mod backends;
pub mod csrf;
pub mod forms;
pub mod hashers;
pub mod permissions;
pub mod security;
pub mod session_auth;
pub mod user;
pub mod views;

// Re-exports for convenience
pub use backends::{authenticate, login, logout, AuthBackend, Credentials, ModelBackend};
pub use csrf::{generate_csrf_token, validate_csrf_token, CsrfMiddleware};
pub use forms::{
    AuthenticationForm, PasswordChangeForm, PasswordResetForm, SetPasswordForm, UserCreationForm,
};
pub use hashers::{check_password, is_password_usable, make_password, PasswordHasher};
pub use permissions::{has_module_perms, has_perm, has_perms, Group, Permission};
pub use security::SecurityMiddleware;
pub use session_auth::{
    get_user_from_request, get_user_from_session, is_authenticated, login_to_session,
    logout_from_session,
};
pub use user::{AbstractBaseUser, AbstractUser, AnonymousUser};
pub use views::{
    login_view, logout_view, password_change_view, DefaultTokenGenerator, LoginConfig,
    LogoutConfig, PasswordChangeConfig, PasswordResetConfig, TokenGenerator,
};
