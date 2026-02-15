//! The `check` management command.
//!
//! Runs system checks to identify potential problems with the project
//! configuration. This mirrors Django's `check` command.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Runs system checks to validate project configuration.
///
/// Inspects the settings and installed apps for common misconfigurations,
/// security issues, and compatibility problems.
pub struct CheckCommand;

/// The result of a single system check.
#[derive(Debug, Clone)]
pub struct CheckMessage {
    /// The severity level of this check result.
    pub level: CheckLevel,
    /// A human-readable description of the issue.
    pub msg: String,
    /// An optional hint for how to resolve the issue.
    pub hint: Option<String>,
    /// A unique identifier for this check (e.g. "security.W001").
    pub id: String,
}

/// Severity levels for system check results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CheckLevel {
    /// Informational message.
    Info,
    /// A warning that may indicate a problem.
    Warning,
    /// An error that must be resolved.
    Error,
    /// A critical error that prevents the application from running.
    Critical,
}

impl std::fmt::Display for CheckLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Error => write!(f, "ERROR"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Runs system checks against the given settings.
///
/// Returns a list of check messages identifying potential issues.
pub fn run_checks(settings: &Settings) -> Vec<CheckMessage> {
    let mut messages = Vec::new();

    // Check: SECRET_KEY should not be empty
    if settings.secret_key.is_empty() {
        messages.push(CheckMessage {
            level: CheckLevel::Warning,
            msg: "SECRET_KEY is empty".to_string(),
            hint: Some("Set a strong, random SECRET_KEY in your settings".to_string()),
            id: "security.W001".to_string(),
        });
    }

    // Check: DEBUG should be false in production
    if settings.debug && !settings.allowed_hosts.is_empty() {
        messages.push(CheckMessage {
            level: CheckLevel::Warning,
            msg: "DEBUG is enabled with ALLOWED_HOSTS set".to_string(),
            hint: Some("Disable DEBUG in production".to_string()),
            id: "security.W002".to_string(),
        });
    }

    // Check: ALLOWED_HOSTS should not be empty when DEBUG is false
    if !settings.debug && settings.allowed_hosts.is_empty() {
        messages.push(CheckMessage {
            level: CheckLevel::Error,
            msg: "ALLOWED_HOSTS is empty with DEBUG=false".to_string(),
            hint: Some("Set ALLOWED_HOSTS to the list of hosts this site can serve".to_string()),
            id: "security.E001".to_string(),
        });
    }

    // Check: database configuration
    if settings.databases.is_empty() {
        messages.push(CheckMessage {
            level: CheckLevel::Error,
            msg: "No databases configured".to_string(),
            hint: Some("Add at least a 'default' database configuration".to_string()),
            id: "database.E001".to_string(),
        });
    }

    // Check: HSTS
    if settings.secure_ssl_redirect && settings.secure_hsts_seconds == 0 {
        messages.push(CheckMessage {
            level: CheckLevel::Warning,
            msg: "SSL redirect is enabled but HSTS is not configured".to_string(),
            hint: Some("Set SECURE_HSTS_SECONDS to enable HSTS".to_string()),
            id: "security.W003".to_string(),
        });
    }

    messages
}

#[async_trait]
impl ManagementCommand for CheckCommand {
    fn name(&self) -> &'static str {
        "check"
    }

    fn help(&self) -> &'static str {
        "Run system checks"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("tag")
                .long("tag")
                .short('t')
                .help("Only run checks with this tag")
                .num_args(0..),
        )
        .arg(
            clap::Arg::new("deploy")
                .long("deploy")
                .action(clap::ArgAction::SetTrue)
                .help("Run deployment checks"),
        )
    }

    async fn handle(
        &self,
        _matches: &clap::ArgMatches,
        settings: &Settings,
    ) -> Result<(), DjangoError> {
        let messages = run_checks(settings);

        if messages.is_empty() {
            tracing::info!("System check identified no issues");
            return Ok(());
        }

        let errors = messages.iter().filter(|m| m.level >= CheckLevel::Error).count();
        let warnings = messages.iter().filter(|m| m.level == CheckLevel::Warning).count();

        for msg in &messages {
            let hint_text = msg
                .hint
                .as_ref()
                .map_or(String::new(), |h| format!("\n\tHINT: {h}"));
            tracing::warn!("{} ({}): {}{}", msg.level, msg.id, msg.msg, hint_text);
        }

        tracing::info!(
            "System check identified {} issue(s) ({} error(s), {} warning(s))",
            messages.len(),
            errors,
            warnings
        );

        if errors > 0 {
            return Err(DjangoError::ConfigurationError(format!(
                "System check found {errors} error(s)"
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_empty_secret_key() {
        let settings = Settings::default();
        let messages = run_checks(&settings);
        assert!(messages.iter().any(|m| m.id == "security.W001"));
    }

    #[test]
    fn test_check_no_issues_for_debug_default() {
        let settings = Settings {
            secret_key: "super-secret-key".to_string(),
            ..Settings::default()
        };
        let messages = run_checks(&settings);
        // Default: debug=true, allowed_hosts=empty => no W002 or E001
        assert!(messages.iter().all(|m| m.id != "security.W002"));
        assert!(messages.iter().all(|m| m.id != "security.E001"));
    }

    #[test]
    fn test_check_debug_with_allowed_hosts() {
        let settings = Settings {
            secret_key: "secret".to_string(),
            allowed_hosts: vec!["example.com".to_string()],
            ..Settings::default()
        };
        let messages = run_checks(&settings);
        assert!(messages.iter().any(|m| m.id == "security.W002"));
    }

    #[test]
    fn test_check_no_debug_empty_allowed_hosts() {
        let settings = Settings {
            secret_key: "secret".to_string(),
            debug: false,
            ..Settings::default()
        };
        let messages = run_checks(&settings);
        assert!(messages.iter().any(|m| m.id == "security.E001"));
    }

    #[test]
    fn test_check_level_display() {
        assert_eq!(CheckLevel::Info.to_string(), "INFO");
        assert_eq!(CheckLevel::Warning.to_string(), "WARNING");
        assert_eq!(CheckLevel::Error.to_string(), "ERROR");
        assert_eq!(CheckLevel::Critical.to_string(), "CRITICAL");
    }

    #[test]
    fn test_check_level_ordering() {
        assert!(CheckLevel::Info < CheckLevel::Warning);
        assert!(CheckLevel::Warning < CheckLevel::Error);
        assert!(CheckLevel::Error < CheckLevel::Critical);
    }

    #[test]
    fn test_check_ssl_no_hsts() {
        let settings = Settings {
            secret_key: "secret".to_string(),
            secure_ssl_redirect: true,
            secure_hsts_seconds: 0,
            ..Settings::default()
        };
        let messages = run_checks(&settings);
        assert!(messages.iter().any(|m| m.id == "security.W003"));
    }
}
