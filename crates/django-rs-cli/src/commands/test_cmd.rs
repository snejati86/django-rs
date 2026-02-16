//! The `test` management command.
//!
//! Runs the project test suite by shelling out to `cargo test`. This mirrors
//! Django's `test` command which delegates to the configured test runner.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;

/// Runs the project test suite.
///
/// Executes `cargo test` with the supplied arguments. Supports `--app` to
/// restrict testing to a specific crate, and `--verbosity` to control output.
/// Additional arguments after `--` are forwarded directly to `cargo test`.
pub struct TestCommand;

/// Builds the argument list for `cargo test` based on the parsed CLI arguments.
pub fn build_cargo_test_args(
    app_label: Option<&str>,
    verbosity: u8,
    failfast: bool,
    extra_args: &[String],
) -> Vec<String> {
    let mut args = vec!["test".to_string()];

    if let Some(app) = app_label {
        args.push("--package".to_string());
        args.push(app.to_string());
    } else {
        args.push("--workspace".to_string());
    }

    if verbosity == 0 {
        args.push("--quiet".to_string());
    }

    if failfast {
        // Forward --no-fail-fast is the cargo default; for fail-fast we add
        // `-- --test-threads=1` behaviour by adding a separator then the flag.
        // However, cargo test supports `--no-fail-fast` to disable, and
        // the default is already fail-fast. We just skip the flag since
        // cargo test already fails fast by default.
    }

    if !extra_args.is_empty() {
        args.push("--".to_string());
        args.extend(extra_args.iter().cloned());
    }

    args
}

#[async_trait]
impl ManagementCommand for TestCommand {
    fn name(&self) -> &'static str {
        "test"
    }

    fn help(&self) -> &'static str {
        "Run the project test suite"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("app_label")
                .help("App/crate to test (e.g. django-rs-core)")
                .required(false),
        )
        .arg(
            clap::Arg::new("verbosity")
                .long("verbosity")
                .short('v')
                .default_value("1")
                .help("Verbosity level: 0=quiet, 1=normal, 2=verbose"),
        )
        .arg(
            clap::Arg::new("failfast")
                .long("failfast")
                .action(clap::ArgAction::SetTrue)
                .help("Stop on first test failure"),
        )
        .arg(
            clap::Arg::new("extra")
                .last(true)
                .num_args(0..)
                .help("Additional arguments to pass to cargo test"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        _settings: &Settings,
    ) -> Result<(), DjangoError> {
        let app_label = matches.get_one::<String>("app_label").map(String::as_str);
        let verbosity: u8 = matches
            .get_one::<String>("verbosity")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let failfast = matches.get_flag("failfast");
        let extra_args: Vec<String> = matches
            .get_many::<String>("extra")
            .map_or_else(Vec::new, |vals| vals.cloned().collect());

        let args = build_cargo_test_args(app_label, verbosity, failfast, &extra_args);

        tracing::info!("Running: cargo {}", args.join(" "));

        let status = tokio::process::Command::new("cargo")
            .args(&args)
            .status()
            .await
            .map_err(|e| {
                DjangoError::InternalServerError(format!("Failed to run cargo test: {e}"))
            })?;

        if status.success() {
            tracing::info!("All tests passed");
            Ok(())
        } else {
            Err(DjangoError::InternalServerError(format!(
                "Tests failed with exit code: {}",
                status.code().unwrap_or(-1)
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_cargo_test_args_default() {
        let args = build_cargo_test_args(None, 1, false, &[]);
        assert_eq!(args, vec!["test", "--workspace"]);
    }

    #[test]
    fn test_build_cargo_test_args_with_app() {
        let args = build_cargo_test_args(Some("django-rs-core"), 1, false, &[]);
        assert_eq!(args, vec!["test", "--package", "django-rs-core"]);
    }

    #[test]
    fn test_build_cargo_test_args_quiet() {
        let args = build_cargo_test_args(None, 0, false, &[]);
        assert!(args.contains(&"--quiet".to_string()));
    }

    #[test]
    fn test_build_cargo_test_args_with_extra() {
        let extra = vec!["--nocapture".to_string(), "test_name".to_string()];
        let args = build_cargo_test_args(None, 1, false, &extra);
        assert!(args.contains(&"--".to_string()));
        assert!(args.contains(&"--nocapture".to_string()));
        assert!(args.contains(&"test_name".to_string()));
    }

    #[test]
    fn test_command_metadata() {
        let cmd = TestCommand;
        assert_eq!(cmd.name(), "test");
        assert_eq!(cmd.help(), "Run the project test suite");
    }
}
