//! Multi-database routing for the ORM.
//!
//! This module provides the [`DatabaseRouter`] trait and [`RouterChain`] for
//! routing database operations to specific database connections. It mirrors
//! Django's `DATABASE_ROUTERS` setting and `DatabaseRouter` class.
//!
//! ## How routing works
//!
//! When a query is executed, the router chain is consulted to determine which
//! database to use. Routers are evaluated in order until one returns a definitive
//! answer (`Some`). If no router returns a value, the `"default"` database is used.
//!
//! ## Example
//!
//! ```
//! use django_rs_db::router::{DatabaseRouter, RouterChain};
//!
//! struct ReadReplicaRouter;
//!
//! impl DatabaseRouter for ReadReplicaRouter {
//!     fn db_for_read(&self, app_label: &str, model_name: &str) -> Option<String> {
//!         Some("replica".to_string())
//!     }
//!     fn db_for_write(&self, app_label: &str, model_name: &str) -> Option<String> {
//!         Some("default".to_string())
//!     }
//! }
//!
//! let mut chain = RouterChain::new();
//! chain.add_router(Box::new(ReadReplicaRouter));
//! assert_eq!(chain.db_for_read("blog", "article"), "replica");
//! assert_eq!(chain.db_for_write("blog", "article"), "default");
//! ```

use std::collections::HashMap;

/// Trait for database routers.
///
/// A database router provides hints about which database to use for different
/// operations. Each method returns `Some(db_alias)` to route to a specific
/// database, or `None` to defer to the next router in the chain.
///
/// This mirrors Django's `DatabaseRouter` class.
pub trait DatabaseRouter: Send + Sync {
    /// Suggests the database to use for read operations on the given model.
    ///
    /// Returns `None` to defer to the next router in the chain.
    fn db_for_read(&self, app_label: &str, model_name: &str) -> Option<String> {
        let _ = (app_label, model_name);
        None
    }

    /// Suggests the database to use for write operations on the given model.
    ///
    /// Returns `None` to defer to the next router in the chain.
    fn db_for_write(&self, app_label: &str, model_name: &str) -> Option<String> {
        let _ = (app_label, model_name);
        None
    }

    /// Determines whether a relation between two objects is allowed.
    ///
    /// Returns `Some(true)` to allow, `Some(false)` to deny, or `None`
    /// to defer to the next router.
    fn allow_relation(
        &self,
        obj1_app: &str,
        obj1_model: &str,
        obj2_app: &str,
        obj2_model: &str,
    ) -> Option<bool> {
        let _ = (obj1_app, obj1_model, obj2_app, obj2_model);
        None
    }

    /// Determines whether a migration operation is allowed on the given database.
    ///
    /// Returns `Some(true)` to allow, `Some(false)` to deny, or `None`
    /// to defer to the next router.
    fn allow_migrate(&self, db: &str, app_label: &str, model_name: &str) -> Option<bool> {
        let _ = (db, app_label, model_name);
        None
    }
}

/// A chain of database routers evaluated in order.
///
/// When a routing decision is needed, each router in the chain is consulted
/// in order until one returns `Some`. If no router provides a definitive
/// answer, the default database (`"default"`) is used.
///
/// This mirrors Django's `DATABASE_ROUTERS` setting behavior.
pub struct RouterChain {
    routers: Vec<Box<dyn DatabaseRouter>>,
}

impl Default for RouterChain {
    fn default() -> Self {
        Self::new()
    }
}

impl RouterChain {
    /// Creates a new empty router chain.
    pub fn new() -> Self {
        Self {
            routers: Vec::new(),
        }
    }

    /// Adds a router to the chain. Routers are evaluated in insertion order.
    pub fn add_router(&mut self, router: Box<dyn DatabaseRouter>) {
        self.routers.push(router);
    }

    /// Returns the database alias to use for read operations.
    ///
    /// Evaluates each router in order. If none returns a value, uses `"default"`.
    pub fn db_for_read(&self, app_label: &str, model_name: &str) -> String {
        for router in &self.routers {
            if let Some(db) = router.db_for_read(app_label, model_name) {
                return db;
            }
        }
        "default".to_string()
    }

    /// Returns the database alias to use for write operations.
    ///
    /// Evaluates each router in order. If none returns a value, uses `"default"`.
    pub fn db_for_write(&self, app_label: &str, model_name: &str) -> String {
        for router in &self.routers {
            if let Some(db) = router.db_for_write(app_label, model_name) {
                return db;
            }
        }
        "default".to_string()
    }

    /// Determines whether a relation between two objects is allowed.
    ///
    /// Returns `true` by default if no router makes a decision.
    pub fn allow_relation(
        &self,
        obj1_app: &str,
        obj1_model: &str,
        obj2_app: &str,
        obj2_model: &str,
    ) -> bool {
        for router in &self.routers {
            if let Some(allowed) = router.allow_relation(obj1_app, obj1_model, obj2_app, obj2_model)
            {
                return allowed;
            }
        }
        true
    }

    /// Determines whether a migration is allowed on the given database.
    ///
    /// Returns `true` by default if no router makes a decision.
    pub fn allow_migrate(&self, db: &str, app_label: &str, model_name: &str) -> bool {
        for router in &self.routers {
            if let Some(allowed) = router.allow_migrate(db, app_label, model_name) {
                return allowed;
            }
        }
        true
    }
}

/// Configuration for multiple named database connections.
///
/// This is the Rust equivalent of Django's `DATABASES` setting. Each entry
/// maps a database alias (e.g., `"default"`, `"replica"`, `"analytics"`)
/// to its connection configuration.
#[derive(Debug, Clone)]
pub struct DatabasesConfig {
    /// Named database configurations.
    databases: HashMap<String, DatabaseEntry>,
}

/// A single database connection entry.
#[derive(Debug, Clone)]
pub struct DatabaseEntry {
    /// The database backend type (e.g., "postgresql", "sqlite", "mysql").
    pub engine: String,
    /// The database name or file path.
    pub name: String,
    /// The database host.
    pub host: Option<String>,
    /// The database port.
    pub port: Option<u16>,
    /// The database user.
    pub user: Option<String>,
    /// The database password.
    pub password: Option<String>,
    /// Additional options.
    pub options: HashMap<String, String>,
}

impl DatabasesConfig {
    /// Creates a new empty configuration.
    pub fn new() -> Self {
        Self {
            databases: HashMap::new(),
        }
    }

    /// Adds a database entry with the given alias.
    pub fn add(&mut self, alias: impl Into<String>, entry: DatabaseEntry) {
        self.databases.insert(alias.into(), entry);
    }

    /// Returns the database entry for the given alias.
    pub fn get(&self, alias: &str) -> Option<&DatabaseEntry> {
        self.databases.get(alias)
    }

    /// Returns all configured database aliases.
    pub fn aliases(&self) -> Vec<&str> {
        self.databases.keys().map(String::as_str).collect()
    }

    /// Returns the number of configured databases.
    pub fn len(&self) -> usize {
        self.databases.len()
    }

    /// Returns whether any databases are configured.
    pub fn is_empty(&self) -> bool {
        self.databases.is_empty()
    }
}

impl Default for DatabasesConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── DatabaseRouter and RouterChain tests ─────────────────────────

    struct AuthRouter;

    impl DatabaseRouter for AuthRouter {
        fn db_for_read(&self, app_label: &str, _model_name: &str) -> Option<String> {
            if app_label == "auth" {
                Some("auth_db".to_string())
            } else {
                None
            }
        }

        fn db_for_write(&self, app_label: &str, _model_name: &str) -> Option<String> {
            if app_label == "auth" {
                Some("auth_db".to_string())
            } else {
                None
            }
        }

        fn allow_relation(
            &self,
            obj1_app: &str,
            _obj1_model: &str,
            obj2_app: &str,
            _obj2_model: &str,
        ) -> Option<bool> {
            if obj1_app == "auth" && obj2_app == "auth" {
                Some(true)
            } else if obj1_app == "auth" || obj2_app == "auth" {
                Some(false)
            } else {
                None
            }
        }

        fn allow_migrate(&self, db: &str, app_label: &str, _model_name: &str) -> Option<bool> {
            if app_label == "auth" {
                Some(db == "auth_db")
            } else {
                None
            }
        }
    }

    struct ReadReplicaRouter;

    impl DatabaseRouter for ReadReplicaRouter {
        fn db_for_read(&self, _app_label: &str, _model_name: &str) -> Option<String> {
            Some("replica".to_string())
        }

        fn db_for_write(&self, _app_label: &str, _model_name: &str) -> Option<String> {
            Some("default".to_string())
        }
    }

    #[test]
    fn test_empty_chain_uses_default() {
        let chain = RouterChain::new();
        assert_eq!(chain.db_for_read("blog", "article"), "default");
        assert_eq!(chain.db_for_write("blog", "article"), "default");
    }

    #[test]
    fn test_single_router_read() {
        let mut chain = RouterChain::new();
        chain.add_router(Box::new(ReadReplicaRouter));
        assert_eq!(chain.db_for_read("blog", "article"), "replica");
    }

    #[test]
    fn test_single_router_write() {
        let mut chain = RouterChain::new();
        chain.add_router(Box::new(ReadReplicaRouter));
        assert_eq!(chain.db_for_write("blog", "article"), "default");
    }

    #[test]
    fn test_router_chain_order_matters() {
        // AuthRouter is first: auth models go to auth_db.
        // ReadReplicaRouter is second: everything else reads from replica.
        let mut chain = RouterChain::new();
        chain.add_router(Box::new(AuthRouter));
        chain.add_router(Box::new(ReadReplicaRouter));

        // Auth models should be routed to auth_db (AuthRouter responds first)
        assert_eq!(chain.db_for_read("auth", "user"), "auth_db");
        assert_eq!(chain.db_for_write("auth", "user"), "auth_db");

        // Non-auth models: AuthRouter returns None, ReadReplicaRouter responds
        assert_eq!(chain.db_for_read("blog", "article"), "replica");
        assert_eq!(chain.db_for_write("blog", "article"), "default");
    }

    #[test]
    fn test_allow_relation_default() {
        let chain = RouterChain::new();
        assert!(chain.allow_relation("blog", "article", "blog", "comment"));
    }

    #[test]
    fn test_allow_relation_same_app() {
        let mut chain = RouterChain::new();
        chain.add_router(Box::new(AuthRouter));
        assert!(chain.allow_relation("auth", "user", "auth", "group"));
    }

    #[test]
    fn test_allow_relation_cross_app_denied() {
        let mut chain = RouterChain::new();
        chain.add_router(Box::new(AuthRouter));
        assert!(!chain.allow_relation("auth", "user", "blog", "article"));
    }

    #[test]
    fn test_allow_relation_non_auth_defers() {
        let mut chain = RouterChain::new();
        chain.add_router(Box::new(AuthRouter));
        // Neither model is from "auth", so AuthRouter returns None -> default true
        assert!(chain.allow_relation("blog", "article", "blog", "comment"));
    }

    #[test]
    fn test_allow_migrate() {
        let mut chain = RouterChain::new();
        chain.add_router(Box::new(AuthRouter));

        // Auth models should only migrate to auth_db
        assert!(chain.allow_migrate("auth_db", "auth", "user"));
        assert!(!chain.allow_migrate("default", "auth", "user"));

        // Non-auth models: AuthRouter defers, default is true
        assert!(chain.allow_migrate("default", "blog", "article"));
    }

    #[test]
    fn test_allow_migrate_default() {
        let chain = RouterChain::new();
        assert!(chain.allow_migrate("default", "blog", "article"));
    }

    // ── DatabasesConfig tests ────────────────────────────────────────

    #[test]
    fn test_databases_config_empty() {
        let config = DatabasesConfig::new();
        assert!(config.is_empty());
        assert_eq!(config.len(), 0);
    }

    #[test]
    fn test_databases_config_add_and_get() {
        let mut config = DatabasesConfig::new();
        config.add(
            "default",
            DatabaseEntry {
                engine: "postgresql".to_string(),
                name: "mydb".to_string(),
                host: Some("localhost".to_string()),
                port: Some(5432),
                user: Some("user".to_string()),
                password: Some("pass".to_string()),
                options: HashMap::new(),
            },
        );

        assert_eq!(config.len(), 1);
        let entry = config.get("default").unwrap();
        assert_eq!(entry.engine, "postgresql");
        assert_eq!(entry.name, "mydb");
        assert_eq!(entry.host.as_deref(), Some("localhost"));
        assert_eq!(entry.port, Some(5432));
    }

    #[test]
    fn test_databases_config_multiple() {
        let mut config = DatabasesConfig::new();
        config.add(
            "default",
            DatabaseEntry {
                engine: "postgresql".to_string(),
                name: "primary".to_string(),
                host: Some("db1.example.com".to_string()),
                port: Some(5432),
                user: None,
                password: None,
                options: HashMap::new(),
            },
        );
        config.add(
            "replica",
            DatabaseEntry {
                engine: "postgresql".to_string(),
                name: "replica".to_string(),
                host: Some("db2.example.com".to_string()),
                port: Some(5432),
                user: None,
                password: None,
                options: HashMap::new(),
            },
        );
        config.add(
            "analytics",
            DatabaseEntry {
                engine: "sqlite".to_string(),
                name: "analytics.db".to_string(),
                host: None,
                port: None,
                user: None,
                password: None,
                options: HashMap::new(),
            },
        );

        assert_eq!(config.len(), 3);
        assert!(!config.is_empty());
        assert!(config.get("default").is_some());
        assert!(config.get("replica").is_some());
        assert!(config.get("analytics").is_some());
        assert!(config.get("nonexistent").is_none());
    }

    #[test]
    fn test_databases_config_aliases() {
        let mut config = DatabasesConfig::new();
        config.add(
            "default",
            DatabaseEntry {
                engine: "postgresql".to_string(),
                name: "db".to_string(),
                host: None,
                port: None,
                user: None,
                password: None,
                options: HashMap::new(),
            },
        );
        config.add(
            "secondary",
            DatabaseEntry {
                engine: "sqlite".to_string(),
                name: ":memory:".to_string(),
                host: None,
                port: None,
                user: None,
                password: None,
                options: HashMap::new(),
            },
        );

        let mut aliases = config.aliases();
        aliases.sort();
        assert_eq!(aliases, vec!["default", "secondary"]);
    }

    // ── Default trait router test ────────────────────────────────────

    struct DefaultRouter;

    impl DatabaseRouter for DefaultRouter {}

    #[test]
    fn test_default_router_returns_none() {
        let router = DefaultRouter;
        assert!(router.db_for_read("any", "model").is_none());
        assert!(router.db_for_write("any", "model").is_none());
        assert!(router.allow_relation("a", "b", "c", "d").is_none());
        assert!(router.allow_migrate("default", "any", "model").is_none());
    }

    #[test]
    fn test_router_chain_default_impl() {
        let chain = RouterChain::default();
        assert_eq!(chain.db_for_read("x", "y"), "default");
    }

    // ── Model-specific router test ───────────────────────────────────

    struct ModelSpecificRouter;

    impl DatabaseRouter for ModelSpecificRouter {
        fn db_for_read(&self, app_label: &str, model_name: &str) -> Option<String> {
            if app_label == "analytics" && model_name == "event" {
                Some("analytics_db".to_string())
            } else {
                None
            }
        }
    }

    #[test]
    fn test_model_specific_routing() {
        let mut chain = RouterChain::new();
        chain.add_router(Box::new(ModelSpecificRouter));
        chain.add_router(Box::new(ReadReplicaRouter));

        assert_eq!(chain.db_for_read("analytics", "event"), "analytics_db");
        assert_eq!(chain.db_for_read("analytics", "session"), "replica");
        assert_eq!(chain.db_for_read("blog", "post"), "replica");
    }
}
