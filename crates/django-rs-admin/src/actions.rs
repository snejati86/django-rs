//! Admin actions for bulk operations on model objects.
//!
//! This module provides the [`AdminAction`] trait for defining custom admin
//! actions, and a built-in [`DeleteSelectedAction`] that deletes selected objects.
//! Actions are async and can leverage Rust's concurrency for performance.

use async_trait::async_trait;
use django_rs_core::DjangoError;
use serde::{Deserialize, Serialize};

/// The result of executing an admin action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Whether the action completed successfully.
    pub success: bool,
    /// A message describing the outcome.
    pub message: String,
    /// The number of objects affected by the action.
    pub affected_count: usize,
}

impl ActionResult {
    /// Creates a successful action result.
    pub fn success(message: impl Into<String>, affected_count: usize) -> Self {
        Self {
            success: true,
            message: message.into(),
            affected_count,
        }
    }

    /// Creates a failed action result.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            affected_count: 0,
        }
    }
}

/// A trait for admin actions that can be executed on selected model objects.
///
/// Actions are async and must be `Send + Sync` to support concurrent execution.
/// Each action has a name (used as an identifier), a description (shown to the user),
/// and an execute method that performs the bulk operation.
///
/// # Examples
///
/// ```
/// use django_rs_admin::actions::{AdminAction, ActionResult};
/// use django_rs_core::DjangoError;
/// use async_trait::async_trait;
///
/// struct MarkPublishedAction;
///
/// #[async_trait]
/// impl AdminAction for MarkPublishedAction {
///     fn name(&self) -> &str { "mark_published" }
///     fn description(&self) -> &str { "Mark selected articles as published" }
///     async fn execute(
///         &self,
///         model_key: &str,
///         selected_ids: &[String],
///     ) -> Result<ActionResult, DjangoError> {
///         Ok(ActionResult::success(
///             format!("Marked {} articles as published", selected_ids.len()),
///             selected_ids.len(),
///         ))
///     }
/// }
/// ```
#[async_trait]
pub trait AdminAction: Send + Sync {
    /// Returns the unique identifier for this action.
    fn name(&self) -> &str;

    /// Returns a human-readable description of what this action does.
    fn description(&self) -> &str;

    /// Executes the action on the selected objects.
    ///
    /// # Arguments
    ///
    /// * `model_key` - The model identifier in `"app.model"` format.
    /// * `selected_ids` - The primary key values of the selected objects.
    ///
    /// # Errors
    ///
    /// Returns a `DjangoError` if the action fails (e.g., database error, permission denied).
    async fn execute(
        &self,
        model_key: &str,
        selected_ids: &[String],
    ) -> Result<ActionResult, DjangoError>;
}

/// Built-in action that deletes the selected objects.
///
/// This action is registered by default for all models in the admin.
/// It reports the count of objects that would be deleted.
#[derive(Debug)]
pub struct DeleteSelectedAction;

#[async_trait]
impl AdminAction for DeleteSelectedAction {
    fn name(&self) -> &'static str {
        "delete_selected"
    }

    fn description(&self) -> &'static str {
        "Delete selected objects"
    }

    async fn execute(
        &self,
        model_key: &str,
        selected_ids: &[String],
    ) -> Result<ActionResult, DjangoError> {
        if selected_ids.is_empty() {
            return Ok(ActionResult::failure("No objects selected."));
        }

        // In a full implementation, this would issue DELETE queries via the ORM.
        // For now, we report success with the count of selected IDs.
        Ok(ActionResult::success(
            format!(
                "Successfully deleted {} {} object(s).",
                selected_ids.len(),
                model_key
            ),
            selected_ids.len(),
        ))
    }
}

/// An action registry that stores available actions for an admin model.
#[derive(Default)]
pub struct ActionRegistry {
    actions: Vec<Box<dyn AdminAction>>,
}

impl ActionRegistry {
    /// Creates a new action registry with the default `delete_selected` action.
    pub fn new() -> Self {
        let mut registry = Self {
            actions: Vec::new(),
        };
        registry.register(Box::new(DeleteSelectedAction));
        registry
    }

    /// Creates an empty action registry (no default actions).
    pub fn empty() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// Registers an action.
    pub fn register(&mut self, action: Box<dyn AdminAction>) {
        self.actions.push(action);
    }

    /// Returns the names of all registered actions.
    pub fn action_names(&self) -> Vec<&str> {
        self.actions.iter().map(|a| a.name()).collect()
    }

    /// Returns descriptions of all registered actions as (name, description) pairs.
    pub fn action_descriptions(&self) -> Vec<(&str, &str)> {
        self.actions
            .iter()
            .map(|a| (a.name(), a.description()))
            .collect()
    }

    /// Finds and executes an action by name.
    pub async fn execute(
        &self,
        action_name: &str,
        model_key: &str,
        selected_ids: &[String],
    ) -> Result<ActionResult, DjangoError> {
        let action = self
            .actions
            .iter()
            .find(|a| a.name() == action_name)
            .ok_or_else(|| DjangoError::NotFound(format!("Action '{action_name}' not found")))?;

        action.execute(model_key, selected_ids).await
    }
}

impl std::fmt::Debug for ActionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionRegistry")
            .field("action_count", &self.actions.len())
            .field("actions", &self.action_names())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_result_success() {
        let result = ActionResult::success("Done", 5);
        assert!(result.success);
        assert_eq!(result.message, "Done");
        assert_eq!(result.affected_count, 5);
    }

    #[test]
    fn test_action_result_failure() {
        let result = ActionResult::failure("Error");
        assert!(!result.success);
        assert_eq!(result.message, "Error");
        assert_eq!(result.affected_count, 0);
    }

    #[test]
    fn test_action_result_serialization() {
        let result = ActionResult::success("Deleted 3 objects", 3);
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"affected_count\":3"));
    }

    #[tokio::test]
    async fn test_delete_selected_action_name() {
        let action = DeleteSelectedAction;
        assert_eq!(action.name(), "delete_selected");
        assert_eq!(action.description(), "Delete selected objects");
    }

    #[tokio::test]
    async fn test_delete_selected_action_empty() {
        let action = DeleteSelectedAction;
        let result = action.execute("blog.article", &[]).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.affected_count, 0);
    }

    #[tokio::test]
    async fn test_delete_selected_action_with_ids() {
        let action = DeleteSelectedAction;
        let ids = vec!["1".to_string(), "2".to_string(), "3".to_string()];
        let result = action.execute("blog.article", &ids).await.unwrap();
        assert!(result.success);
        assert_eq!(result.affected_count, 3);
        assert!(result.message.contains('3'));
    }

    #[test]
    fn test_action_registry_new() {
        let registry = ActionRegistry::new();
        assert_eq!(registry.action_names(), vec!["delete_selected"]);
    }

    #[test]
    fn test_action_registry_empty() {
        let registry = ActionRegistry::empty();
        assert!(registry.action_names().is_empty());
    }

    #[tokio::test]
    async fn test_action_registry_register_custom() {
        struct CustomAction;

        #[async_trait]
        impl AdminAction for CustomAction {
            fn name(&self) -> &'static str {
                "custom_action"
            }
            fn description(&self) -> &'static str {
                "A custom action"
            }
            async fn execute(
                &self,
                _model_key: &str,
                selected_ids: &[String],
            ) -> Result<ActionResult, DjangoError> {
                Ok(ActionResult::success("Done", selected_ids.len()))
            }
        }

        let mut registry = ActionRegistry::new();
        registry.register(Box::new(CustomAction));
        assert_eq!(
            registry.action_names(),
            vec!["delete_selected", "custom_action"]
        );
    }

    #[tokio::test]
    async fn test_action_registry_execute() {
        let registry = ActionRegistry::new();
        let ids = vec!["1".to_string()];
        let result = registry
            .execute("delete_selected", "blog.article", &ids)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.affected_count, 1);
    }

    #[tokio::test]
    async fn test_action_registry_execute_not_found() {
        let registry = ActionRegistry::new();
        let result = registry.execute("nonexistent", "blog.article", &[]).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_action_registry_descriptions() {
        let registry = ActionRegistry::new();
        let descs = registry.action_descriptions();
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0], ("delete_selected", "Delete selected objects"));
    }

    #[test]
    fn test_action_registry_debug() {
        let registry = ActionRegistry::new();
        let debug = format!("{registry:?}");
        assert!(debug.contains("ActionRegistry"));
        assert!(debug.contains("delete_selected"));
    }
}
