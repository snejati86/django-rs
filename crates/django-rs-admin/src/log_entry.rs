//! Admin action log (`LogEntry`).
//!
//! This module provides [`LogEntry`] and [`LogEntryStore`] for recording admin
//! actions. Each time an admin user creates, changes, or deletes an object,
//! a `LogEntry` is created to maintain an audit trail.
//!
//! The log entries are stored in memory via [`InMemoryLogEntryStore`], which is
//! the default implementation. In a production deployment, a database-backed
//! store could be used instead.
//!
//! # Examples
//!
//! ```
//! use django_rs_admin::log_entry::{InMemoryLogEntryStore, LogEntryStore, ActionFlag};
//!
//! let store = InMemoryLogEntryStore::new();
//! store.log_addition(1, "blog.article", "42", "My Article", "Created via admin");
//! store.log_change(1, "blog.article", "42", "My Article", "Changed title");
//! store.log_deletion(1, "blog.article", "42", "My Article", "");
//!
//! let history = store.get_for_object("blog.article", "42");
//! assert_eq!(history.len(), 3);
//! ```

use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Action flag constants matching Django's `LogEntry.ADDITION`, `CHANGE`, `DELETION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ActionFlag {
    /// Object was created (Django: `ADDITION = 1`).
    Addition = 1,
    /// Object was modified (Django: `CHANGE = 2`).
    Change = 2,
    /// Object was deleted (Django: `DELETION = 3`).
    Deletion = 3,
}

impl ActionFlag {
    /// Returns the numeric value of this action flag.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Creates an `ActionFlag` from a numeric value.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Addition),
            2 => Some(Self::Change),
            3 => Some(Self::Deletion),
            _ => None,
        }
    }

    /// Returns a human-readable label for this action flag.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Addition => "Addition",
            Self::Change => "Change",
            Self::Deletion => "Deletion",
        }
    }
}

impl std::fmt::Display for ActionFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// A single admin action log entry.
///
/// Records who did what to which object and when, mirroring Django's
/// `django.contrib.admin.models.LogEntry`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Auto-generated primary key.
    pub id: u64,
    /// Timestamp when the action occurred.
    pub action_time: DateTime<Utc>,
    /// The ID of the user who performed the action.
    pub user_id: u64,
    /// The content type identifier (e.g., "blog.article").
    pub content_type: String,
    /// The primary key of the affected object, as a string.
    pub object_id: String,
    /// A human-readable representation of the affected object.
    pub object_repr: String,
    /// The type of action performed.
    pub action_flag: ActionFlag,
    /// A description of the changes made.
    pub change_message: String,
}

impl LogEntry {
    /// Returns `true` if this log entry records an addition.
    pub fn is_addition(&self) -> bool {
        self.action_flag == ActionFlag::Addition
    }

    /// Returns `true` if this log entry records a change.
    pub fn is_change(&self) -> bool {
        self.action_flag == ActionFlag::Change
    }

    /// Returns `true` if this log entry records a deletion.
    pub fn is_deletion(&self) -> bool {
        self.action_flag == ActionFlag::Deletion
    }

    /// Returns a human-readable description of this log entry.
    pub fn description(&self) -> String {
        let action = self.action_flag.label();
        if self.change_message.is_empty() {
            format!("{action}: {}", self.object_repr)
        } else {
            format!(
                "{action}: {} - {}",
                self.object_repr, self.change_message
            )
        }
    }
}

impl std::fmt::Display for LogEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} {} (pk={}) by user {}",
            self.action_time.format("%Y-%m-%d %H:%M:%S"),
            self.action_flag,
            self.object_repr,
            self.object_id,
            self.user_id,
        )
    }
}

/// Trait for log entry storage backends.
///
/// Provides methods to create log entries and query history for objects or users.
pub trait LogEntryStore: Send + Sync {
    /// Logs an addition (creation) of an object.
    fn log_addition(
        &self,
        user_id: u64,
        content_type: &str,
        object_id: &str,
        object_repr: &str,
        change_message: &str,
    ) -> LogEntry;

    /// Logs a change (update) of an object.
    fn log_change(
        &self,
        user_id: u64,
        content_type: &str,
        object_id: &str,
        object_repr: &str,
        change_message: &str,
    ) -> LogEntry;

    /// Logs a deletion of an object.
    fn log_deletion(
        &self,
        user_id: u64,
        content_type: &str,
        object_id: &str,
        object_repr: &str,
        change_message: &str,
    ) -> LogEntry;

    /// Returns all log entries for a specific object, newest first.
    fn get_for_object(&self, content_type: &str, object_id: &str) -> Vec<LogEntry>;

    /// Returns all log entries by a specific user, newest first.
    fn get_for_user(&self, user_id: u64) -> Vec<LogEntry>;

    /// Returns all log entries of a specific action type, newest first.
    fn get_by_action(&self, action_flag: ActionFlag) -> Vec<LogEntry>;

    /// Returns the most recent log entries, up to `limit`, newest first.
    fn recent(&self, limit: usize) -> Vec<LogEntry>;

    /// Returns the total number of log entries.
    fn count(&self) -> usize;

    /// Clears all log entries.
    fn clear(&self);
}

/// In-memory implementation of [`LogEntryStore`].
///
/// Stores log entries in a thread-safe `Vec` behind `Arc<RwLock>`.
/// Suitable for testing and development.
#[derive(Debug, Clone)]
pub struct InMemoryLogEntryStore {
    entries: Arc<RwLock<Vec<LogEntry>>>,
    next_id: Arc<AtomicU64>,
}

impl InMemoryLogEntryStore {
    /// Creates a new empty in-memory log entry store.
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Creates a log entry with the given parameters and stores it.
    fn create_entry(
        &self,
        user_id: u64,
        content_type: &str,
        object_id: &str,
        object_repr: &str,
        action_flag: ActionFlag,
        change_message: &str,
    ) -> LogEntry {
        let id = self.next_id.fetch_add(1, AtomicOrdering::Relaxed);
        let entry = LogEntry {
            id,
            action_time: Utc::now(),
            user_id,
            content_type: content_type.to_string(),
            object_id: object_id.to_string(),
            object_repr: object_repr.to_string(),
            action_flag,
            change_message: change_message.to_string(),
        };
        let mut entries = self.entries.write().unwrap();
        entries.push(entry.clone());
        entry
    }
}

impl Default for InMemoryLogEntryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LogEntryStore for InMemoryLogEntryStore {
    fn log_addition(
        &self,
        user_id: u64,
        content_type: &str,
        object_id: &str,
        object_repr: &str,
        change_message: &str,
    ) -> LogEntry {
        self.create_entry(
            user_id,
            content_type,
            object_id,
            object_repr,
            ActionFlag::Addition,
            change_message,
        )
    }

    fn log_change(
        &self,
        user_id: u64,
        content_type: &str,
        object_id: &str,
        object_repr: &str,
        change_message: &str,
    ) -> LogEntry {
        self.create_entry(
            user_id,
            content_type,
            object_id,
            object_repr,
            ActionFlag::Change,
            change_message,
        )
    }

    fn log_deletion(
        &self,
        user_id: u64,
        content_type: &str,
        object_id: &str,
        object_repr: &str,
        change_message: &str,
    ) -> LogEntry {
        self.create_entry(
            user_id,
            content_type,
            object_id,
            object_repr,
            ActionFlag::Deletion,
            change_message,
        )
    }

    #[allow(clippy::significant_drop_tightening)]
    fn get_for_object(&self, content_type: &str, object_id: &str) -> Vec<LogEntry> {
        let entries = self.entries.read().unwrap();
        let mut result: Vec<LogEntry> = entries
            .iter()
            .filter(|e| e.content_type == content_type && e.object_id == object_id)
            .cloned()
            .collect();
        result.sort_by(|a, b| b.action_time.cmp(&a.action_time));
        result
    }

    #[allow(clippy::significant_drop_tightening)]
    fn get_for_user(&self, user_id: u64) -> Vec<LogEntry> {
        let entries = self.entries.read().unwrap();
        let mut result: Vec<LogEntry> = entries
            .iter()
            .filter(|e| e.user_id == user_id)
            .cloned()
            .collect();
        result.sort_by(|a, b| b.action_time.cmp(&a.action_time));
        result
    }

    #[allow(clippy::significant_drop_tightening)]
    fn get_by_action(&self, action_flag: ActionFlag) -> Vec<LogEntry> {
        let entries = self.entries.read().unwrap();
        let mut result: Vec<LogEntry> = entries
            .iter()
            .filter(|e| e.action_flag == action_flag)
            .cloned()
            .collect();
        result.sort_by(|a, b| b.action_time.cmp(&a.action_time));
        result
    }

    #[allow(clippy::significant_drop_tightening)]
    fn recent(&self, limit: usize) -> Vec<LogEntry> {
        let entries = self.entries.read().unwrap();
        let mut result: Vec<LogEntry> = entries.to_vec();
        result.sort_by(|a, b| b.action_time.cmp(&a.action_time));
        result.truncate(limit);
        result
    }

    fn count(&self) -> usize {
        let entries = self.entries.read().unwrap();
        entries.len()
    }

    fn clear(&self) {
        let mut entries = self.entries.write().unwrap();
        entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_flag_values() {
        assert_eq!(ActionFlag::Addition.as_u8(), 1);
        assert_eq!(ActionFlag::Change.as_u8(), 2);
        assert_eq!(ActionFlag::Deletion.as_u8(), 3);
    }

    #[test]
    fn test_action_flag_from_u8() {
        assert_eq!(ActionFlag::from_u8(1), Some(ActionFlag::Addition));
        assert_eq!(ActionFlag::from_u8(2), Some(ActionFlag::Change));
        assert_eq!(ActionFlag::from_u8(3), Some(ActionFlag::Deletion));
        assert_eq!(ActionFlag::from_u8(0), None);
        assert_eq!(ActionFlag::from_u8(4), None);
    }

    #[test]
    fn test_action_flag_label() {
        assert_eq!(ActionFlag::Addition.label(), "Addition");
        assert_eq!(ActionFlag::Change.label(), "Change");
        assert_eq!(ActionFlag::Deletion.label(), "Deletion");
    }

    #[test]
    fn test_action_flag_display() {
        assert_eq!(format!("{}", ActionFlag::Addition), "Addition");
        assert_eq!(format!("{}", ActionFlag::Change), "Change");
        assert_eq!(format!("{}", ActionFlag::Deletion), "Deletion");
    }

    #[test]
    fn test_action_flag_serialization() {
        let json = serde_json::to_string(&ActionFlag::Addition).unwrap();
        assert!(json.contains("Addition"));
        let deserialized: ActionFlag = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ActionFlag::Addition);
    }

    #[test]
    fn test_log_entry_is_addition() {
        let entry = LogEntry {
            id: 1,
            action_time: Utc::now(),
            user_id: 1,
            content_type: "blog.article".to_string(),
            object_id: "1".to_string(),
            object_repr: "Test Article".to_string(),
            action_flag: ActionFlag::Addition,
            change_message: String::new(),
        };
        assert!(entry.is_addition());
        assert!(!entry.is_change());
        assert!(!entry.is_deletion());
    }

    #[test]
    fn test_log_entry_is_change() {
        let entry = LogEntry {
            id: 1,
            action_time: Utc::now(),
            user_id: 1,
            content_type: "blog.article".to_string(),
            object_id: "1".to_string(),
            object_repr: "Test Article".to_string(),
            action_flag: ActionFlag::Change,
            change_message: "Changed title".to_string(),
        };
        assert!(!entry.is_addition());
        assert!(entry.is_change());
        assert!(!entry.is_deletion());
    }

    #[test]
    fn test_log_entry_is_deletion() {
        let entry = LogEntry {
            id: 1,
            action_time: Utc::now(),
            user_id: 1,
            content_type: "blog.article".to_string(),
            object_id: "1".to_string(),
            object_repr: "Test Article".to_string(),
            action_flag: ActionFlag::Deletion,
            change_message: String::new(),
        };
        assert!(!entry.is_addition());
        assert!(!entry.is_change());
        assert!(entry.is_deletion());
    }

    #[test]
    fn test_log_entry_description_no_message() {
        let entry = LogEntry {
            id: 1,
            action_time: Utc::now(),
            user_id: 1,
            content_type: "blog.article".to_string(),
            object_id: "1".to_string(),
            object_repr: "Test Article".to_string(),
            action_flag: ActionFlag::Addition,
            change_message: String::new(),
        };
        assert_eq!(entry.description(), "Addition: Test Article");
    }

    #[test]
    fn test_log_entry_description_with_message() {
        let entry = LogEntry {
            id: 1,
            action_time: Utc::now(),
            user_id: 1,
            content_type: "blog.article".to_string(),
            object_id: "1".to_string(),
            object_repr: "Test Article".to_string(),
            action_flag: ActionFlag::Change,
            change_message: "Changed title, body".to_string(),
        };
        assert_eq!(
            entry.description(),
            "Change: Test Article - Changed title, body"
        );
    }

    #[test]
    fn test_log_entry_display() {
        let entry = LogEntry {
            id: 1,
            action_time: Utc::now(),
            user_id: 42,
            content_type: "blog.article".to_string(),
            object_id: "1".to_string(),
            object_repr: "Test Article".to_string(),
            action_flag: ActionFlag::Addition,
            change_message: String::new(),
        };
        let display = format!("{entry}");
        assert!(display.contains("Addition"));
        assert!(display.contains("Test Article"));
        assert!(display.contains("pk=1"));
        assert!(display.contains("user 42"));
    }

    #[test]
    fn test_log_entry_serialization() {
        let entry = LogEntry {
            id: 1,
            action_time: Utc::now(),
            user_id: 1,
            content_type: "blog.article".to_string(),
            object_id: "1".to_string(),
            object_repr: "Test".to_string(),
            action_flag: ActionFlag::Addition,
            change_message: "Created".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"content_type\":\"blog.article\""));
        assert!(json.contains("\"object_id\":\"1\""));
        assert!(json.contains("\"change_message\":\"Created\""));
    }

    // ── InMemoryLogEntryStore tests ──────────────────────────────────

    #[test]
    fn test_store_new() {
        let store = InMemoryLogEntryStore::new();
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_store_default() {
        let store = InMemoryLogEntryStore::default();
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_store_log_addition() {
        let store = InMemoryLogEntryStore::new();
        let entry = store.log_addition(
            1,
            "blog.article",
            "42",
            "My Article",
            "Created via admin",
        );
        assert_eq!(entry.id, 1);
        assert_eq!(entry.user_id, 1);
        assert_eq!(entry.content_type, "blog.article");
        assert_eq!(entry.object_id, "42");
        assert_eq!(entry.object_repr, "My Article");
        assert_eq!(entry.action_flag, ActionFlag::Addition);
        assert_eq!(entry.change_message, "Created via admin");
        assert_eq!(store.count(), 1);
    }

    #[test]
    fn test_store_log_change() {
        let store = InMemoryLogEntryStore::new();
        let entry = store.log_change(
            1,
            "blog.article",
            "42",
            "My Article",
            "Changed title",
        );
        assert_eq!(entry.action_flag, ActionFlag::Change);
        assert_eq!(entry.change_message, "Changed title");
    }

    #[test]
    fn test_store_log_deletion() {
        let store = InMemoryLogEntryStore::new();
        let entry = store.log_deletion(
            1,
            "blog.article",
            "42",
            "My Article",
            "",
        );
        assert_eq!(entry.action_flag, ActionFlag::Deletion);
    }

    #[test]
    fn test_store_auto_increment_ids() {
        let store = InMemoryLogEntryStore::new();
        let e1 = store.log_addition(1, "blog.article", "1", "A1", "");
        let e2 = store.log_addition(1, "blog.article", "2", "A2", "");
        let e3 = store.log_change(1, "blog.article", "1", "A1", "Updated");
        assert_eq!(e1.id, 1);
        assert_eq!(e2.id, 2);
        assert_eq!(e3.id, 3);
        assert_eq!(store.count(), 3);
    }

    #[test]
    fn test_store_get_for_object() {
        let store = InMemoryLogEntryStore::new();
        store.log_addition(1, "blog.article", "1", "Article 1", "Created");
        store.log_change(1, "blog.article", "1", "Article 1", "Updated");
        store.log_addition(1, "blog.article", "2", "Article 2", "Created");
        store.log_deletion(2, "blog.article", "1", "Article 1", "");

        let history = store.get_for_object("blog.article", "1");
        assert_eq!(history.len(), 3);
        // Should be newest first
        assert_eq!(history[0].action_flag, ActionFlag::Deletion);
        assert_eq!(history[2].action_flag, ActionFlag::Addition);
    }

    #[test]
    fn test_store_get_for_object_no_match() {
        let store = InMemoryLogEntryStore::new();
        store.log_addition(1, "blog.article", "1", "Article 1", "");
        let history = store.get_for_object("blog.article", "999");
        assert!(history.is_empty());
    }

    #[test]
    fn test_store_get_for_user() {
        let store = InMemoryLogEntryStore::new();
        store.log_addition(1, "blog.article", "1", "A1", "");
        store.log_addition(2, "blog.article", "2", "A2", "");
        store.log_change(1, "blog.article", "1", "A1", "Updated");

        let history = store.get_for_user(1);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_store_get_for_user_no_match() {
        let store = InMemoryLogEntryStore::new();
        store.log_addition(1, "blog.article", "1", "A1", "");
        let history = store.get_for_user(999);
        assert!(history.is_empty());
    }

    #[test]
    fn test_store_get_by_action() {
        let store = InMemoryLogEntryStore::new();
        store.log_addition(1, "blog.article", "1", "A1", "");
        store.log_addition(1, "blog.article", "2", "A2", "");
        store.log_change(1, "blog.article", "1", "A1", "Updated");
        store.log_deletion(1, "blog.article", "2", "A2", "");

        let additions = store.get_by_action(ActionFlag::Addition);
        assert_eq!(additions.len(), 2);

        let changes = store.get_by_action(ActionFlag::Change);
        assert_eq!(changes.len(), 1);

        let deletions = store.get_by_action(ActionFlag::Deletion);
        assert_eq!(deletions.len(), 1);
    }

    #[test]
    fn test_store_recent() {
        let store = InMemoryLogEntryStore::new();
        for i in 1..=10 {
            store.log_addition(1, "blog.article", &i.to_string(), &format!("A{i}"), "");
        }

        let recent = store.recent(5);
        assert_eq!(recent.len(), 5);
        // Should be newest first
        assert_eq!(recent[0].object_id, "10");
        assert_eq!(recent[4].object_id, "6");
    }

    #[test]
    fn test_store_recent_fewer_than_limit() {
        let store = InMemoryLogEntryStore::new();
        store.log_addition(1, "blog.article", "1", "A1", "");
        store.log_addition(1, "blog.article", "2", "A2", "");

        let recent = store.recent(10);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_store_clear() {
        let store = InMemoryLogEntryStore::new();
        store.log_addition(1, "blog.article", "1", "A1", "");
        store.log_addition(1, "blog.article", "2", "A2", "");
        assert_eq!(store.count(), 2);

        store.clear();
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_store_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let store = Arc::new(InMemoryLogEntryStore::new());
        let mut handles = Vec::new();

        for i in 0..10 {
            let store_clone = Arc::clone(&store);
            handles.push(thread::spawn(move || {
                store_clone.log_addition(
                    1,
                    "blog.article",
                    &i.to_string(),
                    &format!("Article {i}"),
                    "",
                );
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(store.count(), 10);
    }

    #[test]
    fn test_store_clone() {
        let store = InMemoryLogEntryStore::new();
        store.log_addition(1, "blog.article", "1", "A1", "");

        let cloned = store.clone();
        assert_eq!(cloned.count(), 1);

        // Modifications on one should reflect in the other (shared Arc)
        store.log_addition(1, "blog.article", "2", "A2", "");
        assert_eq!(cloned.count(), 2);
    }

    #[test]
    fn test_store_mixed_content_types() {
        let store = InMemoryLogEntryStore::new();
        store.log_addition(1, "blog.article", "1", "Article", "");
        store.log_addition(1, "blog.comment", "1", "Comment", "");
        store.log_addition(1, "auth.user", "1", "User", "");

        let articles = store.get_for_object("blog.article", "1");
        assert_eq!(articles.len(), 1);

        let comments = store.get_for_object("blog.comment", "1");
        assert_eq!(comments.len(), 1);

        assert_eq!(store.count(), 3);
    }

    #[test]
    fn test_log_entry_store_is_object_safe() {
        fn _assert_object_safe(_: &dyn LogEntryStore) {}
    }

    #[test]
    fn test_action_flag_equality() {
        assert_eq!(ActionFlag::Addition, ActionFlag::Addition);
        assert_ne!(ActionFlag::Addition, ActionFlag::Change);
        assert_ne!(ActionFlag::Change, ActionFlag::Deletion);
    }

    #[test]
    fn test_action_flag_copy() {
        let flag = ActionFlag::Addition;
        let copied = flag;
        assert_eq!(flag, copied);
    }

    #[test]
    fn test_log_entry_debug() {
        let entry = LogEntry {
            id: 1,
            action_time: Utc::now(),
            user_id: 1,
            content_type: "blog.article".to_string(),
            object_id: "1".to_string(),
            object_repr: "Test".to_string(),
            action_flag: ActionFlag::Addition,
            change_message: String::new(),
        };
        let debug = format!("{entry:?}");
        assert!(debug.contains("LogEntry"));
        assert!(debug.contains("Addition"));
    }
}
