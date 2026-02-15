//! Built-in management commands.
//!
//! This module contains implementations of Django's standard management commands,
//! adapted for the django-rs framework. Each command implements the
//! [`ManagementCommand`](crate::command::ManagementCommand) trait.

pub mod check;
pub mod collectstatic;
pub mod createsuperuser;
pub mod makemigrations;
pub mod migrate;
pub mod runserver;
pub mod showmigrations;

pub use check::CheckCommand;
pub use collectstatic::CollectstaticCommand;
pub use createsuperuser::CreatesuperuserCommand;
pub use makemigrations::MakemigrationsCommand;
pub use migrate::MigrateCommand;
pub use runserver::RunserverCommand;
pub use showmigrations::ShowmigrationsCommand;

use crate::command::CommandRegistry;

/// Registers all built-in management commands into the given registry.
pub fn register_builtin_commands(registry: &mut CommandRegistry) {
    registry.register(Box::new(RunserverCommand));
    registry.register(Box::new(MigrateCommand));
    registry.register(Box::new(MakemigrationsCommand));
    registry.register(Box::new(CheckCommand));
    registry.register(Box::new(ShowmigrationsCommand));
    registry.register(Box::new(CreatesuperuserCommand));
    registry.register(Box::new(CollectstaticCommand));
}
