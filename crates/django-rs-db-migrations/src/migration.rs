//! Migration structs and dependency graph.
//!
//! A [`Migration`] is a named unit of schema change containing a sequence of
//! [`Operation`]s. The [`MigrationGraph`] manages the dependency DAG between
//! migrations across all apps, enabling topological ordering.

use std::collections::{HashMap, VecDeque};

use django_rs_core::DjangoError;

use crate::operations::Operation;

/// A single migration containing a sequence of operations.
///
/// Migrations are identified by `(app_label, name)` and may declare
/// dependencies on other migrations. Operations within a migration
/// are applied in order.
pub struct Migration {
    /// The migration name (e.g., "0001_initial").
    pub name: String,
    /// The application label this migration belongs to.
    pub app_label: String,
    /// Dependencies on other migrations: `(app_label, migration_name)`.
    pub dependencies: Vec<(String, String)>,
    /// The operations to apply, in order.
    pub operations: Vec<Box<dyn Operation>>,
    /// Whether this is the initial migration for the app.
    pub initial: bool,
}

impl Migration {
    /// Creates a new migration.
    pub fn new(app_label: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            app_label: app_label.into(),
            dependencies: Vec::new(),
            operations: Vec::new(),
            initial: false,
        }
    }

    /// Marks this migration as the initial migration.
    pub fn initial(mut self) -> Self {
        self.initial = true;
        self
    }

    /// Adds a dependency on another migration.
    pub fn depends_on(mut self, app_label: impl Into<String>, name: impl Into<String>) -> Self {
        self.dependencies.push((app_label.into(), name.into()));
        self
    }

    /// Adds an operation to this migration.
    pub fn add_operation(mut self, op: Box<dyn Operation>) -> Self {
        self.operations.push(op);
        self
    }

    /// Returns the `(app_label, name)` key for this migration.
    pub fn key(&self) -> (String, String) {
        (self.app_label.clone(), self.name.clone())
    }
}

/// A directed acyclic graph (DAG) of migrations.
///
/// The graph tracks which migrations exist and their dependency relationships.
/// It provides topological ordering so migrations can be applied in the
/// correct sequence.
pub struct MigrationGraph {
    /// All migration nodes keyed by `(app_label, name)`.
    nodes: HashMap<(String, String), MigrationNode>,
    /// Forward edges: from dependency to dependent.
    forward_edges: HashMap<(String, String), Vec<(String, String)>>,
    /// Backward edges: from dependent to dependency.
    backward_edges: HashMap<(String, String), Vec<(String, String)>>,
}

/// A node in the migration graph.
#[allow(dead_code)]
struct MigrationNode {
    /// The migration key.
    key: (String, String),
    /// Whether this migration is an initial migration.
    initial: bool,
}

impl MigrationGraph {
    /// Creates a new empty migration graph.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            forward_edges: HashMap::new(),
            backward_edges: HashMap::new(),
        }
    }

    /// Adds a migration to the graph.
    pub fn add_node(
        &mut self,
        app_label: impl Into<String>,
        name: impl Into<String>,
        initial: bool,
    ) {
        let key = (app_label.into(), name.into());
        self.nodes.insert(
            key.clone(),
            MigrationNode {
                key: key.clone(),
                initial,
            },
        );
        self.forward_edges.entry(key.clone()).or_default();
        self.backward_edges.entry(key).or_default();
    }

    /// Adds a dependency edge: `child` depends on `parent`.
    ///
    /// Both nodes must have been added previously.
    pub fn add_dependency(
        &mut self,
        child: (String, String),
        parent: (String, String),
    ) -> Result<(), DjangoError> {
        if !self.nodes.contains_key(&child) {
            return Err(DjangoError::DatabaseError(format!(
                "Migration {child:?} not found in graph"
            )));
        }
        if !self.nodes.contains_key(&parent) {
            return Err(DjangoError::DatabaseError(format!(
                "Migration {parent:?} not found in graph"
            )));
        }
        self.forward_edges
            .entry(parent.clone())
            .or_default()
            .push(child.clone());
        self.backward_edges.entry(child).or_default().push(parent);
        Ok(())
    }

    /// Returns all migrations in topological order (dependencies first).
    ///
    /// Returns an error if the graph contains a cycle.
    pub fn topological_order(&self) -> Result<Vec<(String, String)>, DjangoError> {
        let mut in_degree: HashMap<(String, String), usize> = HashMap::new();
        for key in self.nodes.keys() {
            in_degree.insert(key.clone(), 0);
        }
        for children in self.forward_edges.values() {
            for child in children {
                *in_degree.entry(child.clone()).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<(String, String)> = VecDeque::new();
        for (key, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(key.clone());
            }
        }

        // Sort the initial queue for deterministic ordering
        let mut initial: Vec<(String, String)> = queue.into_iter().collect();
        initial.sort();
        queue = initial.into_iter().collect();

        let mut result = Vec::new();
        while let Some(node) = queue.pop_front() {
            result.push(node.clone());
            if let Some(children) = self.forward_edges.get(&node) {
                let mut sorted_children = children.clone();
                sorted_children.sort();
                for child in &sorted_children {
                    if let Some(deg) = in_degree.get_mut(child) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(child.clone());
                        }
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            return Err(DjangoError::DatabaseError(
                "Circular dependency detected in migration graph".to_string(),
            ));
        }

        Ok(result)
    }

    /// Returns the leaf nodes (migrations with no dependents) for a given app.
    pub fn leaf_nodes(&self, app_label: &str) -> Vec<(String, String)> {
        let mut leaves = Vec::new();
        for (key, children) in &self.forward_edges {
            if key.0 == app_label && children.is_empty() {
                leaves.push(key.clone());
            }
        }
        leaves.sort();
        leaves
    }

    /// Returns the root nodes (migrations with no dependencies) for a given app.
    pub fn root_nodes(&self, app_label: &str) -> Vec<(String, String)> {
        let mut roots = Vec::new();
        for (key, parents) in &self.backward_edges {
            if key.0 == app_label && parents.is_empty() {
                roots.push(key.clone());
            }
        }
        roots.sort();
        roots
    }

    /// Returns all node keys in the graph.
    pub fn node_keys(&self) -> Vec<(String, String)> {
        let mut keys: Vec<_> = self.nodes.keys().cloned().collect();
        keys.sort();
        keys
    }

    /// Returns the number of nodes in the graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns whether the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns whether the graph contains a given node.
    pub fn contains(&self, key: &(String, String)) -> bool {
        self.nodes.contains_key(key)
    }

    /// Returns the dependencies of a node.
    pub fn dependencies(&self, key: &(String, String)) -> Vec<(String, String)> {
        self.backward_edges.get(key).cloned().unwrap_or_default()
    }

    /// Returns the dependents of a node.
    pub fn dependents(&self, key: &(String, String)) -> Vec<(String, String)> {
        self.forward_edges.get(key).cloned().unwrap_or_default()
    }

    /// Validates that the graph has no cycles.
    pub fn validate(&self) -> Result<(), DjangoError> {
        self.topological_order()?;
        Ok(())
    }
}

impl Default for MigrationGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Migration tests ─────────────────────────────────────────────

    #[test]
    fn test_migration_new() {
        let m = Migration::new("blog", "0001_initial");
        assert_eq!(m.app_label, "blog");
        assert_eq!(m.name, "0001_initial");
        assert!(!m.initial);
        assert!(m.dependencies.is_empty());
        assert!(m.operations.is_empty());
    }

    #[test]
    fn test_migration_initial() {
        let m = Migration::new("blog", "0001_initial").initial();
        assert!(m.initial);
    }

    #[test]
    fn test_migration_depends_on() {
        let m = Migration::new("blog", "0002_add_author")
            .depends_on("blog", "0001_initial")
            .depends_on("auth", "0001_initial");
        assert_eq!(m.dependencies.len(), 2);
    }

    #[test]
    fn test_migration_key() {
        let m = Migration::new("blog", "0001_initial");
        assert_eq!(m.key(), ("blog".into(), "0001_initial".into()));
    }

    #[test]
    fn test_migration_add_operation() {
        use crate::operations::RunSQL;
        let m = Migration::new("blog", "0001_initial").add_operation(Box::new(RunSQL {
            sql_forwards: "SELECT 1".into(),
            sql_backwards: "SELECT 2".into(),
        }));
        assert_eq!(m.operations.len(), 1);
    }

    // ── MigrationGraph tests ────────────────────────────────────────

    #[test]
    fn test_graph_new() {
        let g = MigrationGraph::new();
        assert!(g.is_empty());
        assert_eq!(g.len(), 0);
    }

    #[test]
    fn test_graph_add_node() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        assert_eq!(g.len(), 1);
        assert!(g.contains(&("blog".into(), "0001_initial".into())));
    }

    #[test]
    fn test_graph_add_dependency() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        g.add_node("blog", "0002_add_title", false);
        g.add_dependency(
            ("blog".into(), "0002_add_title".into()),
            ("blog".into(), "0001_initial".into()),
        )
        .unwrap();
    }

    #[test]
    fn test_graph_add_dependency_missing_child() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        let result = g.add_dependency(
            ("blog".into(), "0002_missing".into()),
            ("blog".into(), "0001_initial".into()),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_graph_add_dependency_missing_parent() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0002_add_title", false);
        let result = g.add_dependency(
            ("blog".into(), "0002_add_title".into()),
            ("blog".into(), "0001_missing".into()),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_graph_topological_order_single() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        let order = g.topological_order().unwrap();
        assert_eq!(order, vec![("blog".into(), "0001_initial".into())]);
    }

    #[test]
    fn test_graph_topological_order_chain() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        g.add_node("blog", "0002_add_title", false);
        g.add_node("blog", "0003_add_body", false);
        g.add_dependency(
            ("blog".into(), "0002_add_title".into()),
            ("blog".into(), "0001_initial".into()),
        )
        .unwrap();
        g.add_dependency(
            ("blog".into(), "0003_add_body".into()),
            ("blog".into(), "0002_add_title".into()),
        )
        .unwrap();

        let order = g.topological_order().unwrap();
        assert_eq!(order.len(), 3);
        // 0001 must come before 0002, 0002 before 0003
        let pos_1 = order.iter().position(|k| k.1 == "0001_initial").unwrap();
        let pos_2 = order.iter().position(|k| k.1 == "0002_add_title").unwrap();
        let pos_3 = order.iter().position(|k| k.1 == "0003_add_body").unwrap();
        assert!(pos_1 < pos_2);
        assert!(pos_2 < pos_3);
    }

    #[test]
    fn test_graph_topological_order_cross_app() {
        let mut g = MigrationGraph::new();
        g.add_node("auth", "0001_initial", true);
        g.add_node("blog", "0001_initial", true);
        g.add_dependency(
            ("blog".into(), "0001_initial".into()),
            ("auth".into(), "0001_initial".into()),
        )
        .unwrap();

        let order = g.topological_order().unwrap();
        let pos_auth = order
            .iter()
            .position(|k| k == &("auth".to_string(), "0001_initial".to_string()))
            .unwrap();
        let pos_blog = order
            .iter()
            .position(|k| k == &("blog".to_string(), "0001_initial".to_string()))
            .unwrap();
        assert!(pos_auth < pos_blog);
    }

    #[test]
    fn test_graph_topological_order_diamond() {
        // A -> B, A -> C, B -> D, C -> D
        let mut g = MigrationGraph::new();
        g.add_node("app", "A", true);
        g.add_node("app", "B", false);
        g.add_node("app", "C", false);
        g.add_node("app", "D", false);
        g.add_dependency(("app".into(), "B".into()), ("app".into(), "A".into()))
            .unwrap();
        g.add_dependency(("app".into(), "C".into()), ("app".into(), "A".into()))
            .unwrap();
        g.add_dependency(("app".into(), "D".into()), ("app".into(), "B".into()))
            .unwrap();
        g.add_dependency(("app".into(), "D".into()), ("app".into(), "C".into()))
            .unwrap();

        let order = g.topological_order().unwrap();
        let pos_a = order.iter().position(|k| k.1 == "A").unwrap();
        let pos_b = order.iter().position(|k| k.1 == "B").unwrap();
        let pos_c = order.iter().position(|k| k.1 == "C").unwrap();
        let pos_d = order.iter().position(|k| k.1 == "D").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_d);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn test_graph_cycle_detection() {
        let mut g = MigrationGraph::new();
        g.add_node("app", "A", false);
        g.add_node("app", "B", false);
        g.add_dependency(("app".into(), "B".into()), ("app".into(), "A".into()))
            .unwrap();
        g.add_dependency(("app".into(), "A".into()), ("app".into(), "B".into()))
            .unwrap();
        let result = g.topological_order();
        assert!(result.is_err());
    }

    #[test]
    fn test_graph_leaf_nodes() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        g.add_node("blog", "0002_add_title", false);
        g.add_dependency(
            ("blog".into(), "0002_add_title".into()),
            ("blog".into(), "0001_initial".into()),
        )
        .unwrap();

        let leaves = g.leaf_nodes("blog");
        assert_eq!(leaves, vec![("blog".into(), "0002_add_title".into())]);
    }

    #[test]
    fn test_graph_root_nodes() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        g.add_node("blog", "0002_add_title", false);
        g.add_dependency(
            ("blog".into(), "0002_add_title".into()),
            ("blog".into(), "0001_initial".into()),
        )
        .unwrap();

        let roots = g.root_nodes("blog");
        assert_eq!(roots, vec![("blog".into(), "0001_initial".into())]);
    }

    #[test]
    fn test_graph_node_keys() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        g.add_node("auth", "0001_initial", true);
        let keys = g.node_keys();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_graph_dependencies_and_dependents() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        g.add_node("blog", "0002_add_title", false);
        g.add_dependency(
            ("blog".into(), "0002_add_title".into()),
            ("blog".into(), "0001_initial".into()),
        )
        .unwrap();

        let deps = g.dependencies(&("blog".into(), "0002_add_title".into()));
        assert_eq!(deps, vec![("blog".into(), "0001_initial".into())]);

        let dependents = g.dependents(&("blog".into(), "0001_initial".into()));
        assert_eq!(dependents, vec![("blog".into(), "0002_add_title".into())]);
    }

    #[test]
    fn test_graph_validate_ok() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        assert!(g.validate().is_ok());
    }

    #[test]
    fn test_graph_validate_cycle() {
        let mut g = MigrationGraph::new();
        g.add_node("app", "A", false);
        g.add_node("app", "B", false);
        g.add_dependency(("app".into(), "B".into()), ("app".into(), "A".into()))
            .unwrap();
        g.add_dependency(("app".into(), "A".into()), ("app".into(), "B".into()))
            .unwrap();
        assert!(g.validate().is_err());
    }

    #[test]
    fn test_graph_default() {
        let g = MigrationGraph::default();
        assert!(g.is_empty());
    }

    #[test]
    fn test_graph_contains() {
        let mut g = MigrationGraph::new();
        g.add_node("blog", "0001_initial", true);
        assert!(g.contains(&("blog".into(), "0001_initial".into())));
        assert!(!g.contains(&("blog".into(), "0002_missing".into())));
    }

    #[test]
    fn test_graph_empty_topological() {
        let g = MigrationGraph::new();
        let order = g.topological_order().unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn test_graph_independent_nodes() {
        let mut g = MigrationGraph::new();
        g.add_node("app1", "0001", true);
        g.add_node("app2", "0001", true);
        g.add_node("app3", "0001", true);
        let order = g.topological_order().unwrap();
        assert_eq!(order.len(), 3);
    }
}
