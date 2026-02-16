//! Mail capture outbox for testing email sending.
//!
//! [`MailOutbox`] collects emails "sent" during tests by providing an in-memory
//! capture backend. Instead of actually sending emails, the backend stores them
//! in a shared list that can be inspected in assertions.
//!
//! ## Example
//!
//! ```rust,no_run
//! use django_rs_test::mail_outbox::{MailOutbox, EmailMessage};
//!
//! let outbox = MailOutbox::new();
//! outbox.send(EmailMessage {
//!     subject: "Welcome".to_string(),
//!     body: "Hello!".to_string(),
//!     from_email: "noreply@example.com".to_string(),
//!     to: vec!["user@example.com".to_string()],
//!     cc: vec![],
//!     bcc: vec![],
//!     headers: std::collections::HashMap::new(),
//! });
//!
//! assert_eq!(outbox.messages().len(), 1);
//! assert_eq!(outbox.messages()[0].subject, "Welcome");
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A captured email message.
///
/// Contains all the fields that would be sent via a real email backend.
#[derive(Debug, Clone)]
pub struct EmailMessage {
    /// The email subject line.
    pub subject: String,
    /// The email body text.
    pub body: String,
    /// The sender email address.
    pub from_email: String,
    /// The list of primary recipients.
    pub to: Vec<String>,
    /// The list of CC recipients.
    pub cc: Vec<String>,
    /// The list of BCC recipients.
    pub bcc: Vec<String>,
    /// Additional email headers.
    pub headers: HashMap<String, String>,
}

impl EmailMessage {
    /// Creates a simple email message.
    pub fn new(
        subject: impl Into<String>,
        body: impl Into<String>,
        from_email: impl Into<String>,
        to: Vec<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            body: body.into(),
            from_email: from_email.into(),
            to,
            cc: Vec::new(),
            bcc: Vec::new(),
            headers: HashMap::new(),
        }
    }

    /// Returns the total number of recipients (to + cc + bcc).
    pub fn recipient_count(&self) -> usize {
        self.to.len() + self.cc.len() + self.bcc.len()
    }
}

/// An in-memory mail outbox that captures emails for test verification.
///
/// Thread-safe via `Arc<Mutex<...>>`, so it can be shared across async tasks
/// and threads.
#[derive(Debug, Clone)]
pub struct MailOutbox {
    messages_store: Arc<Mutex<Vec<EmailMessage>>>,
}

impl Default for MailOutbox {
    fn default() -> Self {
        Self::new()
    }
}

impl MailOutbox {
    /// Creates a new empty mail outbox.
    pub fn new() -> Self {
        Self {
            messages_store: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// "Sends" an email by capturing it in the outbox.
    ///
    /// The email is not actually sent; it is stored in memory for later
    /// inspection via [`messages`](Self::messages).
    pub fn send(&self, message: EmailMessage) {
        self.messages_store
            .lock()
            .expect("MailOutbox lock poisoned")
            .push(message);
    }

    /// Returns all captured email messages.
    pub fn messages(&self) -> Vec<EmailMessage> {
        self.messages_store
            .lock()
            .expect("MailOutbox lock poisoned")
            .clone()
    }

    /// Returns the number of captured emails.
    pub fn len(&self) -> usize {
        self.messages_store
            .lock()
            .expect("MailOutbox lock poisoned")
            .len()
    }

    /// Returns `true` if no emails have been captured.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all captured emails from the outbox.
    pub fn clear(&self) {
        self.messages_store
            .lock()
            .expect("MailOutbox lock poisoned")
            .clear();
    }

    /// Returns the most recently captured email, if any.
    pub fn last_message(&self) -> Option<EmailMessage> {
        self.messages_store
            .lock()
            .expect("MailOutbox lock poisoned")
            .last()
            .cloned()
    }

    /// Asserts that exactly `expected` emails were sent.
    ///
    /// # Panics
    ///
    /// Panics if the count does not match.
    pub fn assert_count(&self, expected: usize) {
        let actual = self.len();
        assert_eq!(
            actual, expected,
            "Expected {expected} email(s), but {actual} were sent"
        );
    }

    /// Asserts that an email was sent to the given address.
    ///
    /// Checks the `to`, `cc`, and `bcc` fields of all captured messages.
    ///
    /// # Panics
    ///
    /// Panics if no email was sent to the given address.
    pub fn assert_sent_to(&self, address: &str) {
        let messages = self.messages();
        let found = messages.iter().any(|m| {
            m.to.iter().any(|a| a == address)
                || m.cc.iter().any(|a| a == address)
                || m.bcc.iter().any(|a| a == address)
        });
        assert!(
            found,
            "No email was sent to '{address}'. Sent to: {:?}",
            messages
                .iter()
                .flat_map(|m| m.to.iter().chain(m.cc.iter()).chain(m.bcc.iter()))
                .collect::<Vec<_>>()
        );
    }

    /// Asserts that an email with the given subject was sent.
    ///
    /// # Panics
    ///
    /// Panics if no email with the subject was found.
    pub fn assert_subject_contains(&self, substring: &str) {
        let messages = self.messages();
        let found = messages.iter().any(|m| m.subject.contains(substring));
        assert!(
            found,
            "No email with subject containing '{substring}' was found. Subjects: {:?}",
            messages.iter().map(|m| &m.subject).collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_email() -> EmailMessage {
        EmailMessage::new(
            "Test Subject",
            "Test body content",
            "sender@example.com",
            vec!["recipient@example.com".to_string()],
        )
    }

    fn sample_email_with_cc() -> EmailMessage {
        let mut email = sample_email();
        email.cc = vec!["cc@example.com".to_string()];
        email.bcc = vec!["bcc@example.com".to_string()];
        email
    }

    #[test]
    fn test_new_outbox_is_empty() {
        let outbox = MailOutbox::new();
        assert!(outbox.is_empty());
        assert_eq!(outbox.len(), 0);
        assert!(outbox.messages().is_empty());
    }

    #[test]
    fn test_default_outbox_is_empty() {
        let outbox = MailOutbox::default();
        assert!(outbox.is_empty());
    }

    #[test]
    fn test_send_captures_email() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email());

        assert_eq!(outbox.len(), 1);
        assert!(!outbox.is_empty());

        let messages = outbox.messages();
        assert_eq!(messages[0].subject, "Test Subject");
        assert_eq!(messages[0].body, "Test body content");
        assert_eq!(messages[0].from_email, "sender@example.com");
        assert_eq!(messages[0].to, vec!["recipient@example.com"]);
    }

    #[test]
    fn test_send_multiple() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email());

        let mut second = sample_email();
        second.subject = "Second Email".to_string();
        outbox.send(second);

        assert_eq!(outbox.len(), 2);
        assert_eq!(outbox.messages()[1].subject, "Second Email");
    }

    #[test]
    fn test_clear() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email());
        outbox.send(sample_email());
        assert_eq!(outbox.len(), 2);

        outbox.clear();
        assert!(outbox.is_empty());
    }

    #[test]
    fn test_last_message() {
        let outbox = MailOutbox::new();
        assert!(outbox.last_message().is_none());

        outbox.send(sample_email());

        let mut second = sample_email();
        second.subject = "Last One".to_string();
        outbox.send(second);

        let last = outbox.last_message().unwrap();
        assert_eq!(last.subject, "Last One");
    }

    #[test]
    fn test_assert_count_passes() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email());
        outbox.send(sample_email());
        outbox.assert_count(2);
    }

    #[test]
    #[should_panic(expected = "Expected 3 email(s), but 1 were sent")]
    fn test_assert_count_fails() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email());
        outbox.assert_count(3);
    }

    #[test]
    fn test_assert_sent_to_passes() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email());
        outbox.assert_sent_to("recipient@example.com");
    }

    #[test]
    fn test_assert_sent_to_cc() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email_with_cc());
        outbox.assert_sent_to("cc@example.com");
    }

    #[test]
    fn test_assert_sent_to_bcc() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email_with_cc());
        outbox.assert_sent_to("bcc@example.com");
    }

    #[test]
    #[should_panic(expected = "No email was sent to")]
    fn test_assert_sent_to_fails() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email());
        outbox.assert_sent_to("nobody@example.com");
    }

    #[test]
    fn test_assert_subject_contains_passes() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email());
        outbox.assert_subject_contains("Subject");
    }

    #[test]
    #[should_panic(expected = "No email with subject containing")]
    fn test_assert_subject_contains_fails() {
        let outbox = MailOutbox::new();
        outbox.send(sample_email());
        outbox.assert_subject_contains("Missing");
    }

    #[test]
    fn test_email_message_new() {
        let email = EmailMessage::new("Hello", "World", "a@b.com", vec!["c@d.com".to_string()]);
        assert_eq!(email.subject, "Hello");
        assert_eq!(email.body, "World");
        assert!(email.cc.is_empty());
        assert!(email.bcc.is_empty());
        assert!(email.headers.is_empty());
    }

    #[test]
    fn test_email_recipient_count() {
        let email = sample_email_with_cc();
        assert_eq!(email.recipient_count(), 3);
    }

    #[test]
    fn test_clone_shares_state() {
        let outbox = MailOutbox::new();
        let outbox2 = outbox.clone();
        outbox.send(sample_email());
        assert_eq!(outbox2.len(), 1);
    }
}
