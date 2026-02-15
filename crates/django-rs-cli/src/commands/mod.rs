//! Built-in management commands.
//!
//! This module contains implementations of Django's standard management commands,
//! adapted for the django-rs framework. Each command implements the
//! [`ManagementCommand`](crate::command::ManagementCommand) trait.

pub mod check;
pub mod collectstatic;
pub mod createsuperuser;
pub mod dumpdata;
pub mod findstatic;
pub mod flush;
pub mod inspectdb;
pub mod loaddata;
pub mod makemigrations;
pub mod migrate;
pub mod runserver;
pub mod showmigrations;
pub mod sqlflush;
pub mod sqlmigrate;
pub mod test_cmd;

pub use check::CheckCommand;
pub use collectstatic::CollectstaticCommand;
pub use createsuperuser::CreatesuperuserCommand;
pub use dumpdata::DumpdataCommand;
pub use findstatic::FindstaticCommand;
pub use flush::FlushCommand;
pub use inspectdb::InspectdbCommand;
pub use loaddata::LoaddataCommand;
pub use makemigrations::MakemigrationsCommand;
pub use migrate::MigrateCommand;
pub use runserver::RunserverCommand;
pub use showmigrations::ShowmigrationsCommand;
pub use sqlflush::SqlflushCommand;
pub use sqlmigrate::SqlmigrateCommand;
pub use test_cmd::TestCommand;

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
    registry.register(Box::new(TestCommand));
    registry.register(Box::new(DumpdataCommand));
    registry.register(Box::new(LoaddataCommand));
    registry.register(Box::new(FlushCommand));
    registry.register(Box::new(InspectdbCommand));
    registry.register(Box::new(SqlmigrateCommand));
    registry.register(Box::new(SqlflushCommand));
    registry.register(Box::new(FindstaticCommand));
}
