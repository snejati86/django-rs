//! Auth view configuration types and token generators.
//!
//! This module provides configuration structs for authentication-related views
//! (login, logout, password change, password reset) and a token generation
//! system for password reset flows.
//!
//! ## Token Generation
//!
//! The [`DefaultTokenGenerator`] creates HMAC-based tokens with embedded timestamps
//! for secure, time-limited password reset links. Tokens are verified using
//! constant-time comparison to prevent timing attacks.

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::user::AbstractUser;

/// Configuration for the login view.
#[derive(Debug, Clone)]
pub struct LoginConfig {
    /// The template to render for the login page.
    pub template_name: String,
    /// The URL parameter name for the post-login redirect destination.
    pub redirect_field_name: String,
    /// Whether to redirect already-authenticated users away from login.
    pub redirect_authenticated_user: bool,
    /// The default URL to redirect to after successful login.
    pub success_url: String,
}

impl Default for LoginConfig {
    fn default() -> Self {
        Self {
            template_name: "registration/login.html".to_string(),
            redirect_field_name: "next".to_string(),
            redirect_authenticated_user: false,
            success_url: "/".to_string(),
        }
    }
}

/// Configuration for the logout view.
#[derive(Debug, Clone)]
pub struct LogoutConfig {
    /// The URL to redirect to after logout.
    pub next_page: String,
    /// The template to render for the logout confirmation page.
    pub template_name: String,
}

impl Default for LogoutConfig {
    fn default() -> Self {
        Self {
            next_page: "/".to_string(),
            template_name: "registration/logged_out.html".to_string(),
        }
    }
}

/// Configuration for the password change view.
#[derive(Debug, Clone)]
pub struct PasswordChangeConfig {
    /// The template to render for the password change form.
    pub template_name: String,
    /// The URL to redirect to after a successful password change.
    pub success_url: String,
}

impl Default for PasswordChangeConfig {
    fn default() -> Self {
        Self {
            template_name: "registration/password_change_form.html".to_string(),
            success_url: "/password_change/done/".to_string(),
        }
    }
}

/// Configuration for the password reset flow.
pub struct PasswordResetConfig {
    /// The template to render for the password reset request form.
    pub template_name: String,
    /// The template for the password reset email body.
    pub email_template_name: String,
    /// The template for the password reset email subject.
    pub subject_template_name: String,
    /// The URL to redirect to after the reset email is sent.
    pub success_url: String,
    /// The token generator used for creating and verifying reset tokens.
    pub token_generator: Box<dyn TokenGenerator>,
}

impl std::fmt::Debug for PasswordResetConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordResetConfig")
            .field("template_name", &self.template_name)
            .field("email_template_name", &self.email_template_name)
            .field("subject_template_name", &self.subject_template_name)
            .field("success_url", &self.success_url)
            .field("token_generator", &"<dyn TokenGenerator>")
            .finish()
    }
}

impl Default for PasswordResetConfig {
    fn default() -> Self {
        Self {
            template_name: "registration/password_reset_form.html".to_string(),
            email_template_name: "registration/password_reset_email.html".to_string(),
            subject_template_name: "registration/password_reset_subject.txt".to_string(),
            success_url: "/password_reset/done/".to_string(),
            token_generator: Box::new(DefaultTokenGenerator::new("default-secret-key")),
        }
    }
}

/// Trait for generating and verifying password reset tokens.
///
/// Implementations must be `Send + Sync` for safe use across async tasks.
#[async_trait]
pub trait TokenGenerator: Send + Sync {
    /// Generates a time-limited token for the given user.
    fn make_token(&self, user: &AbstractUser) -> String;

    /// Verifies that a token is valid for the given user.
    fn check_token(&self, user: &AbstractUser, token: &str) -> bool;
}

/// Default HMAC-based token generator for password reset.
///
/// Creates tokens that include:
/// - A timestamp (for expiration)
/// - An HMAC signature over user data and timestamp (for integrity)
///
/// Tokens expire after a configurable number of seconds (default: 3 days).
#[derive(Debug, Clone)]
pub struct DefaultTokenGenerator {
    /// The secret key used for HMAC signing.
    secret: String,
    /// Token validity period in seconds (default: 259200 = 3 days).
    pub token_lifetime_seconds: u64,
}

impl DefaultTokenGenerator {
    /// Creates a new token generator with the given secret key.
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
            token_lifetime_seconds: 259_200, // 3 days
        }
    }

    /// Returns the current timestamp as seconds since epoch.
    fn current_timestamp() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Computes the HMAC signature for token data.
    fn compute_hmac(&self, data: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(data.as_bytes());
        let result = mac.finalize();
        hex_encode(&result.into_bytes())
    }

    /// Builds the data string to sign, incorporating user state.
    fn make_hash_value(user: &AbstractUser, timestamp: u64) -> String {
        // Include fields that change when the token should be invalidated:
        // - username (user identity)
        // - password hash (invalidates token on password change)
        // - last_login (invalidates token on login)
        // - is_active (invalidates token if user is deactivated)
        let last_login = user
            .base
            .last_login
            .map_or_else(String::new, |dt| dt.timestamp().to_string());

        format!(
            "{}:{}:{}:{}:{}",
            user.username,
            user.base.password,
            last_login,
            user.base.is_active,
            timestamp
        )
    }
}

#[async_trait]
impl TokenGenerator for DefaultTokenGenerator {
    fn make_token(&self, user: &AbstractUser) -> String {
        let timestamp = Self::current_timestamp();
        let hash_value = Self::make_hash_value(user, timestamp);
        let hmac = self.compute_hmac(&hash_value);

        // Token format: timestamp-hmac (base36 timestamp for compactness)
        let ts_str = format!("{timestamp:x}");
        format!("{ts_str}-{hmac}")
    }

    fn check_token(&self, user: &AbstractUser, token: &str) -> bool {
        // Parse the token
        let parts: Vec<&str> = token.splitn(2, '-').collect();
        if parts.len() != 2 {
            return false;
        }

        let Ok(timestamp) = u64::from_str_radix(parts[0], 16) else {
            return false;
        };

        // Check expiration
        let now = Self::current_timestamp();
        if now.saturating_sub(timestamp) > self.token_lifetime_seconds {
            return false;
        }

        // Verify the HMAC
        let hash_value = Self::make_hash_value(user, timestamp);
        let expected_hmac = self.compute_hmac(&hash_value);

        constant_time_eq(parts[1].as_bytes(), expected_hmac.as_bytes())
    }
}

/// Constant-time byte comparison.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Hex-encodes a byte slice.
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_user() -> AbstractUser {
        let mut user = AbstractUser::new("testuser");
        user.base.password = "$argon2id$test_hash".to_string();
        user.base.is_active = true;
        user
    }

    // ── LoginConfig tests ───────────────────────────────────────────

    #[test]
    fn test_login_config_default() {
        let config = LoginConfig::default();
        assert_eq!(config.template_name, "registration/login.html");
        assert_eq!(config.redirect_field_name, "next");
        assert!(!config.redirect_authenticated_user);
        assert_eq!(config.success_url, "/");
    }

    // ── LogoutConfig tests ──────────────────────────────────────────

    #[test]
    fn test_logout_config_default() {
        let config = LogoutConfig::default();
        assert_eq!(config.next_page, "/");
        assert_eq!(config.template_name, "registration/logged_out.html");
    }

    // ── PasswordChangeConfig tests ──────────────────────────────────

    #[test]
    fn test_password_change_config_default() {
        let config = PasswordChangeConfig::default();
        assert!(config.template_name.contains("password_change"));
        assert!(config.success_url.contains("done"));
    }

    // ── PasswordResetConfig tests ───────────────────────────────────

    #[test]
    fn test_password_reset_config_default() {
        let config = PasswordResetConfig::default();
        assert!(config.template_name.contains("password_reset"));
        assert!(config.email_template_name.contains("email"));
        assert!(config.subject_template_name.contains("subject"));
    }

    // ── DefaultTokenGenerator tests ─────────────────────────────────

    #[test]
    fn test_token_generator_make_token() {
        let gen = DefaultTokenGenerator::new("my-secret-key");
        let user = make_test_user();
        let token = gen.make_token(&user);
        assert!(!token.is_empty());
        assert!(token.contains('-'));
    }

    #[test]
    fn test_token_generator_check_valid_token() {
        let gen = DefaultTokenGenerator::new("my-secret-key");
        let user = make_test_user();
        let token = gen.make_token(&user);
        assert!(gen.check_token(&user, &token));
    }

    #[test]
    fn test_token_generator_check_invalid_token() {
        let gen = DefaultTokenGenerator::new("my-secret-key");
        let user = make_test_user();
        assert!(!gen.check_token(&user, "invalid-token"));
    }

    #[test]
    fn test_token_generator_check_tampered_token() {
        let gen = DefaultTokenGenerator::new("my-secret-key");
        let user = make_test_user();
        let token = gen.make_token(&user);
        let tampered = format!("{token}x");
        assert!(!gen.check_token(&user, &tampered));
    }

    #[test]
    fn test_token_generator_different_users() {
        let gen = DefaultTokenGenerator::new("my-secret-key");
        let user1 = make_test_user();
        let mut user2 = make_test_user();
        user2.username = "otheruser".to_string();

        let token = gen.make_token(&user1);
        assert!(!gen.check_token(&user2, &token));
    }

    #[test]
    fn test_token_generator_different_secrets() {
        let gen1 = DefaultTokenGenerator::new("secret-1");
        let gen2 = DefaultTokenGenerator::new("secret-2");
        let user = make_test_user();

        let token = gen1.make_token(&user);
        assert!(!gen2.check_token(&user, &token));
    }

    #[test]
    fn test_token_invalidated_by_password_change() {
        let gen = DefaultTokenGenerator::new("my-secret-key");
        let user = make_test_user();
        let token = gen.make_token(&user);

        // Change the password
        let mut changed_user = user;
        changed_user.base.password = "$argon2id$new_hash".to_string();

        assert!(!gen.check_token(&changed_user, &token));
    }

    #[test]
    fn test_token_expired() {
        let gen = DefaultTokenGenerator {
            token_lifetime_seconds: 0, // Expire immediately
            ..DefaultTokenGenerator::new("my-secret-key")
        };
        let user = make_test_user();

        // Create a token with a timestamp 10 seconds in the past
        let past_timestamp = DefaultTokenGenerator::current_timestamp() - 10;
        let hash_value = DefaultTokenGenerator::make_hash_value(&user, past_timestamp);
        let hmac = gen.compute_hmac(&hash_value);
        let token = format!("{past_timestamp:x}-{hmac}");

        // Token with past timestamp and 0 lifetime should be expired
        assert!(!gen.check_token(&user, &token));
    }

    #[test]
    fn test_token_format() {
        let gen = DefaultTokenGenerator::new("my-secret-key");
        let user = make_test_user();
        let token = gen.make_token(&user);
        let parts: Vec<&str> = token.splitn(2, '-').collect();
        assert_eq!(parts.len(), 2);
        // First part should be hex timestamp
        assert!(u64::from_str_radix(parts[0], 16).is_ok());
        // Second part should be hex HMAC
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_token_check_malformed() {
        let gen = DefaultTokenGenerator::new("my-secret-key");
        let user = make_test_user();
        assert!(!gen.check_token(&user, ""));
        assert!(!gen.check_token(&user, "no-dash-here-not-hex"));
        assert!(!gen.check_token(&user, "gg-invalidhex"));
    }

    // ── constant_time_eq tests ──────────────────────────────────────

    #[test]
    fn test_constant_time_eq_same() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn test_constant_time_eq_different() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn test_constant_time_eq_different_lengths() {
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    // ── hex_encode tests ────────────────────────────────────────────

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0xFF, 0x00, 0xAB]), "ff00ab");
    }
}
