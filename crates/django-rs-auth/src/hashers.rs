//! Password hashing framework for django-rs.
//!
//! This module provides multiple password hashing backends (Argon2, bcrypt, PBKDF2)
//! and password validation utilities. All hashing operations are async, delegating
//! CPU-bound work to `tokio::task::spawn_blocking` to avoid blocking the async runtime.
//!
//! # Hashers
//!
//! - [`Argon2Hasher`] - Primary hasher using Argon2id (recommended)
//! - [`BcryptHasher`] - Fallback using bcrypt
//! - [`Pbkdf2Hasher`] - Legacy support using PBKDF2-HMAC-SHA256
//!
//! # Validators
//!
//! - [`MinimumLengthValidator`] - Enforces minimum password length
//! - [`CommonPasswordValidator`] - Rejects common passwords
//! - [`NumericPasswordValidator`] - Rejects all-numeric passwords
//! - [`UserAttributeSimilarityValidator`] - Rejects passwords similar to user attributes

use async_trait::async_trait;
use django_rs_core::error::DjangoError;

/// Marker string for unusable passwords (accounts with no usable password).
const UNUSABLE_PASSWORD_PREFIX: &str = "!";

/// Trait for password hashing backends.
///
/// Implementations must be `Send + Sync` for safe concurrent access.
/// All hashing and verification methods are async, using `spawn_blocking`
/// internally for CPU-bound cryptographic work.
#[async_trait]
pub trait PasswordHasher: Send + Sync {
    /// Returns the algorithm identifier (e.g., "argon2", "bcrypt", "`pbkdf2_sha256`").
    fn algorithm(&self) -> &str;

    /// Hashes a password and returns the encoded hash string.
    ///
    /// The returned string includes algorithm metadata so the hasher can
    /// be identified during verification.
    async fn hash(&self, password: &str) -> Result<String, DjangoError>;

    /// Verifies a password against an encoded hash.
    ///
    /// Returns `true` if the password matches the hash.
    async fn verify(&self, password: &str, hash: &str) -> Result<bool, DjangoError>;

    /// Returns `true` if the hash should be re-hashed (e.g., parameters have changed).
    fn must_update(&self, hash: &str) -> bool;
}

/// Argon2id password hasher (primary/recommended).
///
/// Uses the Argon2id variant with secure default parameters. This is the
/// recommended hasher for new installations.
#[derive(Debug, Clone)]
pub struct Argon2Hasher;

#[async_trait]
impl PasswordHasher for Argon2Hasher {
    fn algorithm(&self) -> &'static str {
        "argon2"
    }

    async fn hash(&self, password: &str) -> Result<String, DjangoError> {
        let password = password.to_string();
        tokio::task::spawn_blocking(move || {
            use argon2::password_hash::{rand_core::OsRng, PasswordHasher as _, SaltString};
            use argon2::Argon2;

            let salt = SaltString::generate(&mut OsRng);
            let argon2 = Argon2::default();
            let hash = argon2
                .hash_password(password.as_bytes(), &salt)
                .map_err(|e| DjangoError::InternalServerError(format!("Argon2 hash error: {e}")))?;
            Ok(hash.to_string())
        })
        .await
        .map_err(|e| DjangoError::InternalServerError(format!("Task join error: {e}")))?
    }

    async fn verify(&self, password: &str, hash: &str) -> Result<bool, DjangoError> {
        let password = password.to_string();
        let hash = hash.to_string();
        tokio::task::spawn_blocking(move || {
            use argon2::password_hash::PasswordHash;
            use argon2::password_hash::PasswordVerifier;
            use argon2::Argon2;

            let parsed_hash = PasswordHash::new(&hash)
                .map_err(|e| DjangoError::InternalServerError(format!("Invalid hash: {e}")))?;
            Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok())
        })
        .await
        .map_err(|e| DjangoError::InternalServerError(format!("Task join error: {e}")))?
    }

    fn must_update(&self, hash: &str) -> bool {
        // Update if using old argon2 parameters (check for argon2id)
        !hash.starts_with("$argon2id$")
    }
}

/// Bcrypt password hasher (fallback).
///
/// Uses bcrypt with a default cost of 12. Suitable as a fallback when
/// Argon2 is not available.
#[derive(Debug, Clone)]
pub struct BcryptHasher {
    /// The bcrypt cost parameter (default: 12).
    pub cost: u32,
}

impl Default for BcryptHasher {
    fn default() -> Self {
        Self { cost: 12 }
    }
}

#[async_trait]
impl PasswordHasher for BcryptHasher {
    fn algorithm(&self) -> &'static str {
        "bcrypt"
    }

    async fn hash(&self, password: &str) -> Result<String, DjangoError> {
        let password = password.to_string();
        let cost = self.cost;
        tokio::task::spawn_blocking(move || {
            bcrypt::hash(password, cost)
                .map_err(|e| DjangoError::InternalServerError(format!("Bcrypt hash error: {e}")))
        })
        .await
        .map_err(|e| DjangoError::InternalServerError(format!("Task join error: {e}")))?
    }

    async fn verify(&self, password: &str, hash: &str) -> Result<bool, DjangoError> {
        let password = password.to_string();
        let hash = hash.to_string();
        tokio::task::spawn_blocking(move || {
            bcrypt::verify(password, &hash)
                .map_err(|e| DjangoError::InternalServerError(format!("Bcrypt verify error: {e}")))
        })
        .await
        .map_err(|e| DjangoError::InternalServerError(format!("Task join error: {e}")))?
    }

    fn must_update(&self, hash: &str) -> bool {
        // Bcrypt hashes encode cost in the hash: $2b$XX$...
        if let Some(cost_str) = hash.strip_prefix("$2b$").and_then(|s| s.get(..2)) {
            if let Ok(stored_cost) = cost_str.parse::<u32>() {
                return stored_cost < self.cost;
            }
        }
        false
    }
}

/// PBKDF2-HMAC-SHA256 password hasher (legacy support).
///
/// Uses PBKDF2 with HMAC-SHA256 and a configurable number of iterations.
/// This hasher exists for compatibility with legacy password hashes.
#[derive(Debug, Clone)]
pub struct Pbkdf2Hasher {
    /// The number of PBKDF2 iterations (default: `600_000`).
    pub iterations: u32,
}

impl Default for Pbkdf2Hasher {
    fn default() -> Self {
        Self {
            iterations: 600_000,
        }
    }
}

#[async_trait]
impl PasswordHasher for Pbkdf2Hasher {
    fn algorithm(&self) -> &'static str {
        "pbkdf2_sha256"
    }

    async fn hash(&self, password: &str) -> Result<String, DjangoError> {
        let password = password.to_string();
        let iterations = self.iterations;
        tokio::task::spawn_blocking(move || {
            use base64::Engine;
            use hmac::Hmac;
            use rand::RngCore;
            use sha2::Sha256;

            // Generate a 16-byte random salt
            let mut salt = [0u8; 16];
            rand::thread_rng().fill_bytes(&mut salt);
            let salt_b64 = base64::engine::general_purpose::STANDARD.encode(salt);

            // Derive the key
            let mut dk = [0u8; 32];
            pbkdf2_hmac::<Hmac<Sha256>>(password.as_bytes(), salt_b64.as_bytes(), iterations, &mut dk);
            let hash_b64 = base64::engine::general_purpose::STANDARD.encode(dk);

            // Django-compatible format: algorithm$iterations$salt$hash
            Ok(format!("pbkdf2_sha256${iterations}${salt_b64}${hash_b64}"))
        })
        .await
        .map_err(|e| DjangoError::InternalServerError(format!("Task join error: {e}")))?
    }

    async fn verify(&self, password: &str, hash: &str) -> Result<bool, DjangoError> {
        let password = password.to_string();
        let hash = hash.to_string();
        tokio::task::spawn_blocking(move || {
            use base64::Engine;
            use hmac::Hmac;
            use sha2::Sha256;

            let parts: Vec<&str> = hash.splitn(4, '$').collect();
            if parts.len() != 4 || parts[0] != "pbkdf2_sha256" {
                return Ok(false);
            }

            let iterations: u32 = parts[1]
                .parse()
                .map_err(|_| DjangoError::InternalServerError("Invalid iterations in hash".to_string()))?;
            let salt = parts[2];
            let stored_hash = parts[3];

            let mut dk = [0u8; 32];
            pbkdf2_hmac::<Hmac<Sha256>>(password.as_bytes(), salt.as_bytes(), iterations, &mut dk);
            let computed = base64::engine::general_purpose::STANDARD.encode(dk);

            Ok(constant_time_eq(computed.as_bytes(), stored_hash.as_bytes()))
        })
        .await
        .map_err(|e| DjangoError::InternalServerError(format!("Task join error: {e}")))?
    }

    fn must_update(&self, hash: &str) -> bool {
        if let Some(iter_str) = hash.strip_prefix("pbkdf2_sha256$").and_then(|s| s.split('$').next()) {
            if let Ok(stored_iterations) = iter_str.parse::<u32>() {
                return stored_iterations < self.iterations;
            }
        }
        false
    }
}

/// Simple PBKDF2 implementation using HMAC.
fn pbkdf2_hmac<M: hmac::Mac + hmac::digest::KeyInit + Clone>(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    output: &mut [u8],
) {
    let dk_len = output.len();
    // Compute output size by finalizing a dummy MAC
    let dummy = <M as hmac::digest::KeyInit>::new_from_slice(password).expect("HMAC key init");
    let h_len = dummy.finalize().into_bytes().len();
    let blocks_needed = dk_len.div_ceil(h_len);

    for block_num in 1..=blocks_needed {
        let offset = (block_num - 1) * h_len;
        let end = std::cmp::min(offset + h_len, dk_len);

        // U_1 = PRF(password, salt || INT_32_BE(i))
        let mut mac =
            <M as hmac::digest::KeyInit>::new_from_slice(password).expect("HMAC key init");
        mac.update(salt);
        #[allow(clippy::cast_possible_truncation)]
        let block_idx = block_num as u32;
        mac.update(&block_idx.to_be_bytes());
        let u1 = mac.finalize().into_bytes();

        let mut result = u1.to_vec();
        let mut prev = u1;

        for _ in 1..iterations {
            let mut mac =
                <M as hmac::digest::KeyInit>::new_from_slice(password).expect("HMAC key init");
            mac.update(&prev);
            let u_i = mac.finalize().into_bytes();
            for (r, u) in result.iter_mut().zip(u_i.iter()) {
                *r ^= u;
            }
            prev = u_i;
        }

        output[offset..end].copy_from_slice(&result[..end - offset]);
    }
}

/// Constant-time byte comparison to prevent timing attacks.
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

/// Returns the default list of password hashers.
///
/// The first hasher in the list is used for new passwords. Others are
/// tried during verification for backwards compatibility.
pub fn default_hashers() -> Vec<Box<dyn PasswordHasher>> {
    vec![
        Box::new(Argon2Hasher),
        Box::new(BcryptHasher::default()),
        Box::new(Pbkdf2Hasher::default()),
    ]
}

/// Identifies the hasher for a given encoded hash.
fn identify_hasher(encoded: &str) -> Option<Box<dyn PasswordHasher>> {
    if encoded.starts_with("$argon2") {
        Some(Box::new(Argon2Hasher))
    } else if encoded.starts_with("$2b$") || encoded.starts_with("$2a$") {
        Some(Box::new(BcryptHasher::default()))
    } else if encoded.starts_with("pbkdf2_sha256$") {
        Some(Box::new(Pbkdf2Hasher::default()))
    } else {
        None
    }
}

/// Hashes a password using the preferred (first) hasher.
///
/// Uses Argon2id by default. The work is offloaded to a blocking thread.
pub async fn make_password(password: &str) -> Result<String, DjangoError> {
    let hashers = default_hashers();
    let preferred = &hashers[0];
    preferred.hash(password).await
}

/// Checks a password against an encoded hash.
///
/// Automatically identifies the correct hasher from the hash format.
/// Returns `false` for unusable password hashes.
pub async fn check_password(password: &str, hash: &str) -> Result<bool, DjangoError> {
    if !is_password_usable(hash) {
        return Ok(false);
    }

    let hasher = identify_hasher(hash).ok_or_else(|| {
        DjangoError::InternalServerError(format!(
            "Unknown password hashing algorithm for hash: {}",
            hash.chars().take(20).collect::<String>()
        ))
    })?;

    hasher.verify(password, hash).await
}

/// Returns `true` if the encoded hash represents a usable password.
///
/// Passwords prefixed with `!` (or empty) are considered unusable. This
/// is used for accounts that should not be able to log in with a password.
pub fn is_password_usable(hash: &str) -> bool {
    !hash.is_empty() && !hash.starts_with(UNUSABLE_PASSWORD_PREFIX)
}

// ── Password Validators ──────────────────────────────────────────────

/// Trait for password validators.
///
/// Validators check whether a password meets specific requirements.
pub trait PasswordValidator: Send + Sync {
    /// Validates a password, returning an error message if it fails.
    fn validate(&self, password: &str) -> Result<(), String>;

    /// Returns a description of this validator's requirements.
    fn get_help_text(&self) -> String;
}

/// Validates that a password meets a minimum length requirement.
#[derive(Debug, Clone)]
pub struct MinimumLengthValidator {
    /// The minimum allowed password length.
    pub min_length: usize,
}

impl Default for MinimumLengthValidator {
    fn default() -> Self {
        Self { min_length: 8 }
    }
}

impl PasswordValidator for MinimumLengthValidator {
    fn validate(&self, password: &str) -> Result<(), String> {
        if password.len() < self.min_length {
            Err(format!(
                "This password is too short. It must contain at least {} characters.",
                self.min_length
            ))
        } else {
            Ok(())
        }
    }

    fn get_help_text(&self) -> String {
        format!(
            "Your password must contain at least {} characters.",
            self.min_length
        )
    }
}

/// Validates that a password is not in a list of common passwords.
///
/// Uses a built-in list of commonly used passwords.
#[derive(Debug, Clone)]
pub struct CommonPasswordValidator {
    /// The set of common passwords to reject.
    pub common_passwords: Vec<String>,
}

impl Default for CommonPasswordValidator {
    fn default() -> Self {
        Self {
            common_passwords: vec![
                "password", "123456", "12345678", "1234", "qwerty", "12345",
                "dragon", "pussy", "baseball", "football", "letmein", "monkey",
                "696969", "abc123", "mustang", "michael", "shadow", "master",
                "jennifer", "111111", "2000", "jordan", "superman", "harley",
                "1234567", "fuckme", "hunter", "fuckyou", "trustno1", "ranger",
                "buster", "thomas", "tigger", "robert", "soccer", "fuck",
                "batman", "test", "pass", "killer", "hockey", "george", "charlie",
                "andrew", "michelle", "love", "sunshine", "jessica", "asshole",
                "6969", "pepper", "daniel", "access", "123456789", "654321",
                "joshua", "maggie", "starwars", "silver", "william", "dallas",
                "yankees", "123123", "ashley", "666666", "hello", "amanda",
                "orange", "biteme", "freedom", "computer", "sexy", "thunder",
                "nicole", "ginger", "heather", "hammer", "summer", "corvette",
                "taylor", "fucker", "austin", "1111", "merlin", "matthew",
                "121212", "golfer", "cheese", "princess", "martin", "chelsea",
                "patrick", "richard", "diamond", "yellow", "bigdog", "secret",
                "asdfgh", "sparky", "cowboy", "iloveyou", "admin", "password1",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        }
    }
}

impl PasswordValidator for CommonPasswordValidator {
    fn validate(&self, password: &str) -> Result<(), String> {
        let lower = password.to_lowercase();
        if self.common_passwords.iter().any(|p| p == &lower) {
            Err("This password is too common.".to_string())
        } else {
            Ok(())
        }
    }

    fn get_help_text(&self) -> String {
        "Your password can't be a commonly used password.".to_string()
    }
}

/// Validates that a password is not entirely numeric.
#[derive(Debug, Clone, Default)]
pub struct NumericPasswordValidator;

impl PasswordValidator for NumericPasswordValidator {
    fn validate(&self, password: &str) -> Result<(), String> {
        if !password.is_empty() && password.chars().all(|c| c.is_ascii_digit()) {
            Err("This password is entirely numeric.".to_string())
        } else {
            Ok(())
        }
    }

    fn get_help_text(&self) -> String {
        "Your password can't be entirely numeric.".to_string()
    }
}

/// Validates that a password is not too similar to user attributes.
///
/// Checks the password against a list of user attribute values
/// (username, email, first name, last name) using basic similarity
/// detection.
#[derive(Debug, Clone, Default)]
pub struct UserAttributeSimilarityValidator {
    /// The maximum allowed similarity ratio (0.0 to 1.0). Default: 0.7.
    pub max_similarity: f64,
}

impl UserAttributeSimilarityValidator {
    /// Creates a new validator with the default similarity threshold (0.7).
    pub const fn new() -> Self {
        Self {
            max_similarity: 0.7,
        }
    }

    /// Validates a password against a set of user attribute values.
    pub fn validate_with_attributes(
        &self,
        password: &str,
        attribute_values: &[&str],
    ) -> Result<(), String> {
        let password_lower = password.to_lowercase();
        for attr in attribute_values {
            if attr.is_empty() {
                continue;
            }
            let attr_lower = attr.to_lowercase();
            let similarity = compute_similarity(&password_lower, &attr_lower);
            if similarity >= self.max_similarity {
                return Err("The password is too similar to your personal information.".to_string());
            }
        }
        Ok(())
    }
}

impl PasswordValidator for UserAttributeSimilarityValidator {
    fn validate(&self, _password: &str) -> Result<(), String> {
        // Without user attributes, we cannot perform similarity checking.
        // Use `validate_with_attributes` when user data is available.
        Ok(())
    }

    fn get_help_text(&self) -> String {
        "Your password can't be too similar to your other personal information.".to_string()
    }
}

/// Computes a simple similarity ratio between two strings.
///
/// Returns a value between 0.0 (completely different) and 1.0 (identical).
/// Uses the longest common subsequence length relative to the longer string.
fn compute_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }

    // Check if one contains the other
    if a.contains(b) || b.contains(a) {
        return 1.0;
    }

    // Simple longest common subsequence (LCS) based similarity
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    // Use two-row DP for memory efficiency
    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a_bytes[i - 1] == b_bytes[j - 1] {
                curr[j] = prev[j - 1] + 1;
            } else {
                curr[j] = curr[j - 1].max(prev[j]);
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }

    let lcs_len = prev[n];
    #[allow(clippy::cast_precision_loss)]
    let similarity = (2.0 * lcs_len as f64) / (m + n) as f64;
    similarity
}

/// Validates a password against all default validators.
pub fn validate_password(password: &str) -> Result<(), Vec<String>> {
    let validators: Vec<Box<dyn PasswordValidator>> = vec![
        Box::new(MinimumLengthValidator::default()),
        Box::new(CommonPasswordValidator::default()),
        Box::new(NumericPasswordValidator),
    ];

    let errors: Vec<String> = validators
        .iter()
        .filter_map(|v| v.validate(password).err())
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Argon2Hasher tests ───────────────────────────────────────────

    #[tokio::test]
    async fn test_argon2_hash_and_verify() {
        let hasher = Argon2Hasher;
        let hash = hasher.hash("test_password").await.unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(hasher.verify("test_password", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_argon2_wrong_password() {
        let hasher = Argon2Hasher;
        let hash = hasher.hash("correct_password").await.unwrap();
        assert!(!hasher.verify("wrong_password", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_argon2_algorithm() {
        let hasher = Argon2Hasher;
        assert_eq!(hasher.algorithm(), "argon2");
    }

    #[tokio::test]
    async fn test_argon2_must_update_old_hash() {
        let hasher = Argon2Hasher;
        // Non-argon2id hashes should be updated
        assert!(hasher.must_update("$argon2i$v=19$m=65536,t=3,p=1$abc$def"));
        // argon2id hashes should not need updating
        assert!(!hasher.must_update("$argon2id$v=19$m=19456,t=2,p=1$abc$def"));
    }

    #[tokio::test]
    async fn test_argon2_unique_salts() {
        let hasher = Argon2Hasher;
        let hash1 = hasher.hash("same_password").await.unwrap();
        let hash2 = hasher.hash("same_password").await.unwrap();
        assert_ne!(hash1, hash2); // Different salts
        assert!(hasher.verify("same_password", &hash1).await.unwrap());
        assert!(hasher.verify("same_password", &hash2).await.unwrap());
    }

    // ── BcryptHasher tests ───────────────────────────────────────────

    #[tokio::test]
    async fn test_bcrypt_hash_and_verify() {
        let hasher = BcryptHasher { cost: 4 }; // Low cost for fast tests
        let hash = hasher.hash("test_password").await.unwrap();
        assert!(hash.starts_with("$2b$"));
        assert!(hasher.verify("test_password", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_bcrypt_wrong_password() {
        let hasher = BcryptHasher { cost: 4 };
        let hash = hasher.hash("correct_password").await.unwrap();
        assert!(!hasher.verify("wrong_password", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_bcrypt_algorithm() {
        let hasher = BcryptHasher::default();
        assert_eq!(hasher.algorithm(), "bcrypt");
    }

    #[tokio::test]
    async fn test_bcrypt_must_update() {
        let hasher = BcryptHasher { cost: 12 };
        assert!(hasher.must_update("$2b$04$abcdefghijklmnopqrstuuMGzV2iWi7CiUvqzPaIXqVVwK..rJdm"));
        assert!(!hasher.must_update("$2b$12$abcdefghijklmnopqrstuuMGzV2iWi7CiUvqzPaIXqVVwK..rJdm"));
    }

    #[tokio::test]
    async fn test_bcrypt_unique_salts() {
        let hasher = BcryptHasher { cost: 4 };
        let hash1 = hasher.hash("same_password").await.unwrap();
        let hash2 = hasher.hash("same_password").await.unwrap();
        assert_ne!(hash1, hash2);
    }

    // ── Pbkdf2Hasher tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_pbkdf2_hash_and_verify() {
        let hasher = Pbkdf2Hasher { iterations: 1000 }; // Low for fast tests
        let hash = hasher.hash("test_password").await.unwrap();
        assert!(hash.starts_with("pbkdf2_sha256$"));
        assert!(hasher.verify("test_password", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_pbkdf2_wrong_password() {
        let hasher = Pbkdf2Hasher { iterations: 1000 };
        let hash = hasher.hash("correct_password").await.unwrap();
        assert!(!hasher.verify("wrong_password", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_pbkdf2_algorithm() {
        let hasher = Pbkdf2Hasher::default();
        assert_eq!(hasher.algorithm(), "pbkdf2_sha256");
    }

    #[tokio::test]
    async fn test_pbkdf2_must_update() {
        let hasher = Pbkdf2Hasher {
            iterations: 600_000,
        };
        assert!(hasher.must_update("pbkdf2_sha256$100000$salt$hash"));
        assert!(!hasher.must_update("pbkdf2_sha256$600000$salt$hash"));
    }

    #[tokio::test]
    async fn test_pbkdf2_unique_salts() {
        let hasher = Pbkdf2Hasher { iterations: 1000 };
        let hash1 = hasher.hash("same_password").await.unwrap();
        let hash2 = hasher.hash("same_password").await.unwrap();
        assert_ne!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_pbkdf2_hash_format() {
        let hasher = Pbkdf2Hasher { iterations: 5000 };
        let hash = hasher.hash("mypassword").await.unwrap();
        let parts: Vec<&str> = hash.splitn(4, '$').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "pbkdf2_sha256");
        assert_eq!(parts[1], "5000");
    }

    // ── make_password / check_password tests ────────────────────────

    #[tokio::test]
    async fn test_make_password_and_check() {
        let hash = make_password("my_secure_password").await.unwrap();
        assert!(check_password("my_secure_password", &hash).await.unwrap());
        assert!(!check_password("wrong_password", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_check_password_bcrypt() {
        let hasher = BcryptHasher { cost: 4 };
        let hash = hasher.hash("bcrypt_password").await.unwrap();
        assert!(check_password("bcrypt_password", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_check_password_pbkdf2() {
        let hasher = Pbkdf2Hasher { iterations: 1000 };
        let hash = hasher.hash("pbkdf2_password").await.unwrap();
        assert!(check_password("pbkdf2_password", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_check_password_unusable() {
        assert!(!check_password("password", "!unusable").await.unwrap());
    }

    #[tokio::test]
    async fn test_check_password_empty_hash() {
        assert!(!check_password("password", "").await.unwrap());
    }

    #[tokio::test]
    async fn test_check_password_unknown_algorithm() {
        let result = check_password("password", "unknown$hash$format").await;
        assert!(result.is_err());
    }

    // ── is_password_usable tests ────────────────────────────────────

    #[test]
    fn test_is_password_usable() {
        assert!(is_password_usable("$argon2id$v=19$hash"));
        assert!(is_password_usable("$2b$12$hash"));
        assert!(is_password_usable("pbkdf2_sha256$600000$salt$hash"));
        assert!(!is_password_usable("!"));
        assert!(!is_password_usable("!unusable"));
        assert!(!is_password_usable(""));
    }

    // ── identify_hasher tests ───────────────────────────────────────

    #[test]
    fn test_identify_hasher_argon2() {
        let hasher = identify_hasher("$argon2id$v=19$hash").unwrap();
        assert_eq!(hasher.algorithm(), "argon2");
    }

    #[test]
    fn test_identify_hasher_bcrypt() {
        let hasher = identify_hasher("$2b$12$hash").unwrap();
        assert_eq!(hasher.algorithm(), "bcrypt");
    }

    #[test]
    fn test_identify_hasher_pbkdf2() {
        let hasher = identify_hasher("pbkdf2_sha256$1000$salt$hash").unwrap();
        assert_eq!(hasher.algorithm(), "pbkdf2_sha256");
    }

    #[test]
    fn test_identify_hasher_unknown() {
        assert!(identify_hasher("unknown_format").is_none());
    }

    // ── MinimumLengthValidator tests ────────────────────────────────

    #[test]
    fn test_minimum_length_validator_pass() {
        let v = MinimumLengthValidator { min_length: 8 };
        assert!(v.validate("long_enough_password").is_ok());
    }

    #[test]
    fn test_minimum_length_validator_fail() {
        let v = MinimumLengthValidator { min_length: 8 };
        assert!(v.validate("short").is_err());
    }

    #[test]
    fn test_minimum_length_validator_exact() {
        let v = MinimumLengthValidator { min_length: 8 };
        assert!(v.validate("12345678").is_ok());
    }

    #[test]
    fn test_minimum_length_validator_help_text() {
        let v = MinimumLengthValidator { min_length: 10 };
        assert!(v.get_help_text().contains("10"));
    }

    // ── CommonPasswordValidator tests ───────────────────────────────

    #[test]
    fn test_common_password_validator_reject() {
        let v = CommonPasswordValidator::default();
        assert!(v.validate("password").is_err());
        assert!(v.validate("123456").is_err());
        assert!(v.validate("qwerty").is_err());
    }

    #[test]
    fn test_common_password_validator_accept() {
        let v = CommonPasswordValidator::default();
        assert!(v.validate("kj3h98fhq3w98fh").is_ok());
    }

    #[test]
    fn test_common_password_validator_case_insensitive() {
        let v = CommonPasswordValidator::default();
        assert!(v.validate("PASSWORD").is_err());
        assert!(v.validate("Password").is_err());
    }

    // ── NumericPasswordValidator tests ──────────────────────────────

    #[test]
    fn test_numeric_validator_reject() {
        let v = NumericPasswordValidator;
        assert!(v.validate("12345678").is_err());
    }

    #[test]
    fn test_numeric_validator_accept_mixed() {
        let v = NumericPasswordValidator;
        assert!(v.validate("1234abcd").is_ok());
    }

    #[test]
    fn test_numeric_validator_accept_alpha() {
        let v = NumericPasswordValidator;
        assert!(v.validate("abcdefgh").is_ok());
    }

    // ── UserAttributeSimilarityValidator tests ──────────────────────

    #[test]
    fn test_user_attribute_similarity_reject() {
        let v = UserAttributeSimilarityValidator::new();
        assert!(v
            .validate_with_attributes("johndoe123", &["johndoe"])
            .is_err());
    }

    #[test]
    fn test_user_attribute_similarity_accept() {
        let v = UserAttributeSimilarityValidator::new();
        assert!(v
            .validate_with_attributes("xK9mQ2pL", &["johndoe"])
            .is_ok());
    }

    #[test]
    fn test_user_attribute_similarity_empty_attrs() {
        let v = UserAttributeSimilarityValidator::new();
        assert!(v.validate_with_attributes("password", &[""]).is_ok());
    }

    #[test]
    fn test_user_attribute_similarity_exact_match() {
        let v = UserAttributeSimilarityValidator::new();
        assert!(v.validate_with_attributes("johndoe", &["johndoe"]).is_err());
    }

    // ── validate_password tests ─────────────────────────────────────

    #[test]
    fn test_validate_password_strong() {
        assert!(validate_password("k8Hj!mNpQ2x").is_ok());
    }

    #[test]
    fn test_validate_password_too_short() {
        let result = validate_password("abc");
        assert!(result.is_err());
        assert!(result.unwrap_err().iter().any(|e| e.contains("short")));
    }

    #[test]
    fn test_validate_password_common() {
        let result = validate_password("password");
        assert!(result.is_err());
        assert!(result.unwrap_err().iter().any(|e| e.contains("common")));
    }

    #[test]
    fn test_validate_password_numeric() {
        let result = validate_password("12345678");
        assert!(result.is_err());
        assert!(result.unwrap_err().iter().any(|e| e.contains("numeric")));
    }

    #[test]
    fn test_validate_password_multiple_errors() {
        let result = validate_password("123");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        // Too short AND entirely numeric
        assert!(errors.len() >= 2);
    }

    // ── compute_similarity tests ────────────────────────────────────

    #[test]
    fn test_similarity_identical() {
        assert!((compute_similarity("hello", "hello") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_similarity_completely_different() {
        let sim = compute_similarity("abc", "xyz");
        assert!(sim < 0.5);
    }

    #[test]
    fn test_similarity_contains() {
        assert!((compute_similarity("john", "johndoe") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_similarity_empty() {
        assert!((compute_similarity("", "") - 1.0).abs() < f64::EPSILON);
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
        assert!(!constant_time_eq(b"hi", b"hello"));
    }

    // ── default_hashers tests ───────────────────────────────────────

    #[test]
    fn test_default_hashers_order() {
        let hashers = default_hashers();
        assert_eq!(hashers.len(), 3);
        assert_eq!(hashers[0].algorithm(), "argon2");
        assert_eq!(hashers[1].algorithm(), "bcrypt");
        assert_eq!(hashers[2].algorithm(), "pbkdf2_sha256");
    }
}
