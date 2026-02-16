//! Messages framework for one-time notifications.
//!
//! Provides a message storage system for passing one-time notifications
//! to the frontend (e.g., "Object saved successfully"). Messages are consumed
//! when read, mirroring Django's `django.contrib.messages`.

use serde::{Deserialize, Serialize};

/// The severity level of a message.
///
/// Mirrors Django's message levels (DEBUG=10, INFO=20, SUCCESS=25, WARNING=30, ERROR=40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessageLevel {
    /// Debug-level message, typically not shown in production.
    Debug = 10,
    /// Informational message.
    Info = 20,
    /// Success notification (e.g., "Saved successfully").
    Success = 25,
    /// Warning that requires attention.
    Warning = 30,
    /// Error message indicating a failure.
    Error = 40,
}

impl MessageLevel {
    /// Returns the CSS tag class for this level.
    pub const fn tag(&self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

impl std::fmt::Display for MessageLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.tag())
    }
}

/// A single notification message with a level, text, and optional tags.
///
/// Messages are one-time: they are consumed when read from storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The severity level of this message.
    pub level: MessageLevel,
    /// The message text.
    pub text: String,
    /// Space-separated CSS class tags for rendering.
    pub tags: String,
}

impl Message {
    /// Creates a new message with the given level and text.
    pub fn new(level: MessageLevel, text: impl Into<String>) -> Self {
        let tags = level.tag().to_string();
        Self {
            level,
            text: text.into(),
            tags,
        }
    }

    /// Creates a new message with custom tags.
    pub fn with_tags(level: MessageLevel, text: impl Into<String>, extra_tags: &str) -> Self {
        let base_tag = level.tag();
        let tags = if extra_tags.is_empty() {
            base_tag.to_string()
        } else {
            format!("{extra_tags} {base_tag}")
        };
        Self {
            level,
            text: text.into(),
            tags,
        }
    }
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

/// Storage for one-time messages.
///
/// Messages are added during request processing and drained (consumed) when
/// read. This mirrors Django's message storage backends.
///
/// # Examples
///
/// ```
/// use django_rs_admin::contrib::messages::{MessageStorage, MessageLevel};
///
/// let mut storage = MessageStorage::new();
/// storage.success("Item saved successfully.");
/// storage.warning("Some fields were ignored.");
///
/// let messages = storage.get_messages();
/// assert_eq!(messages.len(), 2);
///
/// // Messages are drained after reading
/// let messages = storage.get_messages();
/// assert!(messages.is_empty());
/// ```
#[derive(Debug, Clone, Default)]
pub struct MessageStorage {
    messages: Vec<Message>,
}

impl MessageStorage {
    /// Creates a new empty message storage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a message with the given level and text.
    pub fn add(&mut self, level: MessageLevel, text: &str) {
        self.messages.push(Message::new(level, text));
    }

    /// Adds a message with custom extra tags.
    pub fn add_with_tags(&mut self, level: MessageLevel, text: &str, extra_tags: &str) {
        self.messages
            .push(Message::with_tags(level, text, extra_tags));
    }

    /// Drains and returns all stored messages.
    ///
    /// After calling this method, the storage is empty. This is the standard
    /// pattern for one-time notifications.
    pub fn get_messages(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.messages)
    }

    /// Returns a reference to the stored messages without consuming them.
    pub fn peek_messages(&self) -> &[Message] {
        &self.messages
    }

    /// Adds a debug-level message.
    pub fn debug(&mut self, text: &str) {
        self.add(MessageLevel::Debug, text);
    }

    /// Adds an info-level message.
    pub fn info(&mut self, text: &str) {
        self.add(MessageLevel::Info, text);
    }

    /// Adds a success-level message.
    pub fn success(&mut self, text: &str) {
        self.add(MessageLevel::Success, text);
    }

    /// Adds a warning-level message.
    pub fn warning(&mut self, text: &str) {
        self.add(MessageLevel::Warning, text);
    }

    /// Adds an error-level message.
    pub fn error(&mut self, text: &str) {
        self.add(MessageLevel::Error, text);
    }

    /// Returns the number of stored messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Returns `true` if no messages are stored.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Clears all stored messages without returning them.
    pub fn clear(&mut self) {
        self.messages.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_level_tag() {
        assert_eq!(MessageLevel::Debug.tag(), "debug");
        assert_eq!(MessageLevel::Info.tag(), "info");
        assert_eq!(MessageLevel::Success.tag(), "success");
        assert_eq!(MessageLevel::Warning.tag(), "warning");
        assert_eq!(MessageLevel::Error.tag(), "error");
    }

    #[test]
    fn test_message_level_display() {
        assert_eq!(MessageLevel::Info.to_string(), "info");
        assert_eq!(MessageLevel::Error.to_string(), "error");
    }

    #[test]
    fn test_message_level_ordering() {
        assert!(MessageLevel::Debug < MessageLevel::Info);
        assert!(MessageLevel::Info < MessageLevel::Success);
        assert!(MessageLevel::Success < MessageLevel::Warning);
        assert!(MessageLevel::Warning < MessageLevel::Error);
    }

    #[test]
    fn test_message_new() {
        let msg = Message::new(MessageLevel::Success, "Saved!");
        assert_eq!(msg.level, MessageLevel::Success);
        assert_eq!(msg.text, "Saved!");
        assert_eq!(msg.tags, "success");
    }

    #[test]
    fn test_message_with_tags() {
        let msg = Message::with_tags(MessageLevel::Error, "Failed!", "important");
        assert_eq!(msg.tags, "important error");
    }

    #[test]
    fn test_message_with_empty_tags() {
        let msg = Message::with_tags(MessageLevel::Info, "Hello", "");
        assert_eq!(msg.tags, "info");
    }

    #[test]
    fn test_message_display() {
        let msg = Message::new(MessageLevel::Info, "Hello World");
        assert_eq!(msg.to_string(), "Hello World");
    }

    #[test]
    fn test_storage_new() {
        let storage = MessageStorage::new();
        assert!(storage.is_empty());
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn test_storage_add() {
        let mut storage = MessageStorage::new();
        storage.add(MessageLevel::Info, "Test");
        assert_eq!(storage.len(), 1);
        assert!(!storage.is_empty());
    }

    #[test]
    fn test_storage_convenience_methods() {
        let mut storage = MessageStorage::new();
        storage.debug("Debug msg");
        storage.info("Info msg");
        storage.success("Success msg");
        storage.warning("Warning msg");
        storage.error("Error msg");
        assert_eq!(storage.len(), 5);

        let messages = storage.get_messages();
        assert_eq!(messages[0].level, MessageLevel::Debug);
        assert_eq!(messages[1].level, MessageLevel::Info);
        assert_eq!(messages[2].level, MessageLevel::Success);
        assert_eq!(messages[3].level, MessageLevel::Warning);
        assert_eq!(messages[4].level, MessageLevel::Error);
    }

    #[test]
    fn test_storage_get_messages_drains() {
        let mut storage = MessageStorage::new();
        storage.info("Hello");
        storage.success("World");

        let messages = storage.get_messages();
        assert_eq!(messages.len(), 2);

        // Should be empty after drain
        let messages = storage.get_messages();
        assert!(messages.is_empty());
    }

    #[test]
    fn test_storage_peek_messages() {
        let mut storage = MessageStorage::new();
        storage.info("Hello");

        let peeked = storage.peek_messages();
        assert_eq!(peeked.len(), 1);

        // Should still be there
        assert_eq!(storage.len(), 1);
    }

    #[test]
    fn test_storage_clear() {
        let mut storage = MessageStorage::new();
        storage.info("Hello");
        storage.error("World");
        storage.clear();
        assert!(storage.is_empty());
    }

    #[test]
    fn test_storage_add_with_tags() {
        let mut storage = MessageStorage::new();
        storage.add_with_tags(MessageLevel::Warning, "Caution!", "sticky");
        let messages = storage.get_messages();
        assert_eq!(messages[0].tags, "sticky warning");
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::new(MessageLevel::Success, "Saved!");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"text\":\"Saved!\""));
        assert!(json.contains("\"tags\":\"success\""));
    }

    #[test]
    fn test_message_level_equality() {
        assert_eq!(MessageLevel::Info, MessageLevel::Info);
        assert_ne!(MessageLevel::Info, MessageLevel::Error);
    }
}
