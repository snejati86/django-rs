//! Management command framework for django-rs.
//!
//! This module provides the [`ManagementCommand`] trait for defining CLI commands
//! and [`CommandRegistry`] for registering and discovering them. It mirrors Django's
//! `django.core.management` module.
//!
//! ## Defining a Custom Command
//!
//! ```rust,no_run
//! use async_trait::async_trait;
//! use django_rs_cli::command::ManagementCommand;
//! use django_rs_core::{DjangoError, Settings};
//!
//! struct GreetCommand;
//!
//! #[async_trait]
//! impl ManagementCommand for GreetCommand {
//!     fn name(&self) -> &str { "greet" }
//!     fn help(&self) -> &str { "Say hello" }
//!
//!     async fn handle(
//!         &self,
//!         _matches: &clap::ArgMatches,
//!         _settings: &Settings,
//!     ) -> Result<(), DjangoError> {
//!         println!("Hello from django-rs!");
//!         Ok(())
//!     }
//! }
//! ```

use std::collections::HashMap;

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

/// A management command that can be registered and invoked through the CLI.
///
/// This trait mirrors Django's `BaseCommand` class. Implementations define
/// a name, help text, optional arguments, and an async handler function.
/// All commands must be `Send + Sync` to support concurrent execution.
#[async_trait]
pub trait ManagementCommand: Send + Sync {
    /// Returns the name of this command (used to invoke it from the CLI).
    fn name(&self) -> &str;

    /// Returns a short help description for this command.
    fn help(&self) -> &str;

    /// Adds custom arguments to the clap command.
    ///
    /// Override this to add positional arguments, flags, or options.
    /// The default implementation returns the command unchanged.
    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd
    }

    /// Executes the command with the given argument matches and settings.
    ///
    /// This is the main entry point for the command logic. It runs
    /// asynchronously and can perform I/O, database operations, etc.
    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        settings: &Settings,
    ) -> Result<(), DjangoError>;
}

/// A registry of management commands.
///
/// Commands are registered by name and can be looked up, listed, or executed.
/// This is the central dispatcher for the django-rs management CLI, mirroring
/// Django's `ManagementUtility`.
pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn ManagementCommand>>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    /// Creates a new empty command registry.
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Registers a management command.
    ///
    /// If a command with the same name already exists, it is replaced.
    pub fn register(&mut self, command: Box<dyn ManagementCommand>) {
        let name = command.name().to_string();
        self.commands.insert(name, command);
    }

    /// Returns a reference to the command with the given name, if registered.
    pub fn get(&self, name: &str) -> Option<&dyn ManagementCommand> {
        self.commands.get(name).map(AsRef::as_ref)
    }

    /// Returns a sorted list of all registered command names.
    pub fn list_commands(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.commands.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// Returns the number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Returns `true` if no commands are registered.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Builds a top-level clap `Command` containing all registered subcommands.
    ///
    /// Collects command metadata (name, help text, arguments) into owned values
    /// so that the resulting `clap::Command` is independent of `&self`.
    pub fn build_cli(&self) -> clap::Command {
        let mut app = clap::Command::new("django-rs")
            .about("django-rs management utility")
            .subcommand_required(true);

        // Collect entries and sort by name
        let mut entries: Vec<_> = self.commands.iter().collect();
        entries.sort_by_key(|(name, _)| (*name).clone());

        for (name, cmd) in entries {
            // clap requires &'static str for command names. We leak the string
            // to satisfy this. Management commands are registered once at startup,
            // so the leak is bounded and acceptable.
            let static_name: &'static str = Box::leak(name.clone().into_boxed_str());
            let subcmd = clap::Command::new(static_name).about(cmd.help().to_string());
            let subcmd = cmd.add_arguments(subcmd);
            app = app.subcommand(subcmd);
        }

        app
    }

    /// Executes the command identified by the given argument matches.
    ///
    /// Looks up the subcommand name from `matches` and dispatches to the
    /// registered command's `handle` method.
    pub async fn execute(
        &self,
        matches: &clap::ArgMatches,
        settings: &Settings,
    ) -> Result<(), DjangoError> {
        let (name, sub_matches) = matches.subcommand().ok_or_else(|| {
            DjangoError::ConfigurationError("No subcommand specified".to_string())
        })?;

        let cmd = self.get(name).ok_or_else(|| {
            DjangoError::ConfigurationError(format!("Unknown command: {name}"))
        })?;

        cmd.handle(sub_matches, settings).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCommand {
        cmd_name: String,
    }

    impl TestCommand {
        fn new(name: &str) -> Self {
            Self {
                cmd_name: name.to_string(),
            }
        }
    }

    #[async_trait]
    impl ManagementCommand for TestCommand {
        fn name(&self) -> &str {
            &self.cmd_name
        }

        fn help(&self) -> &'static str {
            "A test command"
        }

        fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
            cmd.arg(
                clap::Arg::new("verbose")
                    .long("verbose")
                    .action(clap::ArgAction::SetTrue),
            )
        }

        async fn handle(
            &self,
            _matches: &clap::ArgMatches,
            _settings: &Settings,
        ) -> Result<(), DjangoError> {
            Ok(())
        }
    }

    struct FailingCommand;

    #[async_trait]
    impl ManagementCommand for FailingCommand {
        fn name(&self) -> &'static str {
            "fail"
        }

        fn help(&self) -> &'static str {
            "A command that always fails"
        }

        async fn handle(
            &self,
            _matches: &clap::ArgMatches,
            _settings: &Settings,
        ) -> Result<(), DjangoError> {
            Err(DjangoError::ConfigurationError("deliberate failure".to_string()))
        }
    }

    #[test]
    fn test_registry_new_is_empty() {
        let registry = CommandRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_default() {
        let registry = CommandRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = CommandRegistry::new();
        registry.register(Box::new(TestCommand::new("test")));
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let cmd = registry.get("test");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name(), "test");
        assert_eq!(cmd.unwrap().help(), "A test command");
    }

    #[test]
    fn test_get_nonexistent() {
        let registry = CommandRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_list_commands_sorted() {
        let mut registry = CommandRegistry::new();
        registry.register(Box::new(TestCommand::new("zebra")));
        registry.register(Box::new(TestCommand::new("alpha")));
        registry.register(Box::new(TestCommand::new("middle")));

        let names = registry.list_commands();
        assert_eq!(names, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn test_register_replaces_existing() {
        let mut registry = CommandRegistry::new();
        registry.register(Box::new(TestCommand::new("test")));
        registry.register(Box::new(TestCommand::new("test")));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_build_cli() {
        let mut registry = CommandRegistry::new();
        registry.register(Box::new(TestCommand::new("check")));
        registry.register(Box::new(TestCommand::new("runserver")));

        let cli = registry.build_cli();
        // Verify the CLI has subcommands by trying to parse
        let result = cli.try_get_matches_from(["django-rs", "check"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_cli_with_arguments() {
        let mut registry = CommandRegistry::new();
        registry.register(Box::new(TestCommand::new("test")));

        let cli = registry.build_cli();
        let matches = cli
            .try_get_matches_from(["django-rs", "test", "--verbose"])
            .unwrap();
        let (name, sub_matches) = matches.subcommand().unwrap();
        assert_eq!(name, "test");
        assert!(sub_matches.get_flag("verbose"));
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mut registry = CommandRegistry::new();
        registry.register(Box::new(TestCommand::new("test")));

        let cli = registry.build_cli();
        let matches = cli
            .try_get_matches_from(["django-rs", "test"])
            .unwrap();

        let settings = Settings::default();
        let result = registry.execute(&matches, &settings).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_failing_command() {
        let mut registry = CommandRegistry::new();
        registry.register(Box::new(FailingCommand));

        let cli = registry.build_cli();
        let matches = cli
            .try_get_matches_from(["django-rs", "fail"])
            .unwrap();

        let settings = Settings::default();
        let result = registry.execute(&matches, &settings).await;
        assert!(result.is_err());
    }
}
