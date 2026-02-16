//! Email sending framework for django-rs.
//!
//! This module provides the [`EmailBackend`] trait and built-in implementations
//! for sending email. It mirrors Django's `django.core.mail` module.
//!
//! ## Backends
//!
//! - [`SmtpBackend`] - Real SMTP email sending (async)
//! - [`ConsoleBackend`] - Prints emails to stdout (for development)
//! - [`FileBackend`] - Writes emails to files (for development)
//! - [`InMemoryBackend`] - Collects emails in memory (for testing)

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use django_rs_core::DjangoError;

/// An email attachment.
///
/// Contains the filename, content bytes, and MIME type for a file
/// to be attached to an email message.
#[derive(Debug, Clone)]
pub struct Attachment {
    /// The filename to present in the email.
    pub filename: String,
    /// The raw content of the attachment.
    pub content: Vec<u8>,
    /// The MIME content type (e.g. "application/pdf", "image/png").
    pub mimetype: String,
}

impl Attachment {
    /// Creates a new attachment.
    pub fn new(filename: impl Into<String>, content: Vec<u8>, mimetype: impl Into<String>) -> Self {
        Self {
            filename: filename.into(),
            content,
            mimetype: mimetype.into(),
        }
    }
}

/// An email message, mirroring Django's `EmailMessage`.
///
/// Contains all the components of an email: subject, body, recipients,
/// headers, optional HTML content, and attachments.
#[derive(Debug, Clone)]
pub struct EmailMessage {
    /// The email subject line.
    pub subject: String,
    /// The plain text body of the email.
    pub body: String,
    /// The sender's email address.
    pub from_email: String,
    /// The primary recipients (To addresses).
    pub to: Vec<String>,
    /// Carbon copy recipients.
    pub cc: Vec<String>,
    /// Blind carbon copy recipients.
    pub bcc: Vec<String>,
    /// Reply-to addresses.
    pub reply_to: Vec<String>,
    /// Additional email headers.
    pub headers: HashMap<String, String>,
    /// Optional HTML body. When set, the email is sent as multipart.
    pub html_body: Option<String>,
    /// File attachments.
    pub attachments: Vec<Attachment>,
}

impl EmailMessage {
    /// Creates a new email message with the minimum required fields.
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
            reply_to: Vec::new(),
            headers: HashMap::new(),
            html_body: None,
            attachments: Vec::new(),
        }
    }

    /// Adds an attachment to this email message.
    #[must_use]
    pub fn with_attachment(mut self, attachment: Attachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Sets the HTML body for this email message.
    #[must_use]
    pub fn with_html_body(mut self, html: impl Into<String>) -> Self {
        self.html_body = Some(html.into());
        self
    }

    /// Returns all recipients (to + cc + bcc).
    pub fn recipients(&self) -> Vec<&str> {
        self.to
            .iter()
            .chain(self.cc.iter())
            .chain(self.bcc.iter())
            .map(String::as_str)
            .collect()
    }

    /// Formats the email as a human-readable string.
    pub fn format_message(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();
        let _ = writeln!(output, "From: {}", self.from_email);
        let _ = writeln!(output, "To: {}", self.to.join(", "));

        if !self.cc.is_empty() {
            let _ = writeln!(output, "Cc: {}", self.cc.join(", "));
        }
        if !self.bcc.is_empty() {
            let _ = writeln!(output, "Bcc: {}", self.bcc.join(", "));
        }
        if !self.reply_to.is_empty() {
            let _ = writeln!(output, "Reply-To: {}", self.reply_to.join(", "));
        }

        for (key, value) in &self.headers {
            let _ = writeln!(output, "{key}: {value}");
        }

        let _ = writeln!(output, "Subject: {}", self.subject);
        let _ = writeln!(output, "\n{}", self.body);

        if let Some(html) = &self.html_body {
            let _ = writeln!(output, "\n--- HTML ---\n{html}");
        }

        if !self.attachments.is_empty() {
            let _ = writeln!(output, "\n--- Attachments ---");
            for att in &self.attachments {
                let _ = writeln!(
                    output,
                    "  {} ({}, {} bytes)",
                    att.filename,
                    att.mimetype,
                    att.content.len()
                );
            }
        }

        output
    }
}

/// A backend for sending email messages.
///
/// All methods are async and the trait requires `Send + Sync` for
/// concurrent email sending from multiple tokio tasks.
#[async_trait]
pub trait EmailBackend: Send + Sync {
    /// Sends a single email message.
    async fn send(&self, message: &EmailMessage) -> Result<(), DjangoError>;

    /// Sends multiple email messages, returning the count of successfully sent.
    async fn send_many(&self, messages: &[EmailMessage]) -> Result<usize, DjangoError> {
        let mut count = 0;
        for message in messages {
            if self.send(message).await.is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }
}

/// An async SMTP email backend.
///
/// In a full implementation, this would use an async SMTP client library.
/// Currently a placeholder that logs the email details.
#[derive(Debug, Clone)]
pub struct SmtpBackend {
    /// The SMTP host to connect to.
    pub host: String,
    /// The SMTP port.
    pub port: u16,
    /// Optional username for authentication.
    pub username: Option<String>,
    /// Optional password for authentication.
    pub password: Option<String>,
    /// Whether to use TLS.
    pub use_tls: bool,
}

impl SmtpBackend {
    /// Creates a new SMTP backend with the given host and port.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            username: None,
            password: None,
            use_tls: false,
        }
    }

    /// Enables TLS for SMTP connections.
    #[must_use]
    pub fn with_tls(mut self) -> Self {
        self.use_tls = true;
        self
    }

    /// Sets authentication credentials.
    #[must_use]
    pub fn with_credentials(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }
}

#[async_trait]
impl EmailBackend for SmtpBackend {
    async fn send(&self, message: &EmailMessage) -> Result<(), DjangoError> {
        // In a full implementation, this would open an async SMTP connection
        // and send the email. For now, we simulate the operation.
        tracing::info!(
            "SMTP: Sending email '{}' from {} to {} via {}:{}",
            message.subject,
            message.from_email,
            message.to.join(", "),
            self.host,
            self.port,
        );

        if message.to.is_empty() {
            return Err(DjangoError::BadRequest(
                "Email must have at least one recipient".to_string(),
            ));
        }

        Ok(())
    }
}

/// An email backend that prints emails to stdout.
///
/// Useful for development. This mirrors Django's `ConsoleBackend`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConsoleBackend;

#[async_trait]
impl EmailBackend for ConsoleBackend {
    async fn send(&self, message: &EmailMessage) -> Result<(), DjangoError> {
        let separator = "-".repeat(60);
        let formatted = message.format_message();

        // Use tokio::task::spawn_blocking for stdout I/O to avoid
        // blocking the async runtime.
        tokio::task::spawn_blocking(move || {
            println!("{separator}");
            print!("{formatted}");
            println!("{separator}");
        })
        .await
        .map_err(|e| DjangoError::InternalServerError(e.to_string()))?;

        Ok(())
    }
}

/// An email backend that writes emails to files.
///
/// Each email is written to a separate file in the configured directory.
/// This is useful for development and testing.
#[derive(Debug, Clone)]
pub struct FileBackend {
    /// The directory to write email files to.
    pub dir: PathBuf,
}

impl FileBackend {
    /// Creates a new file backend that writes to the given directory.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

#[async_trait]
impl EmailBackend for FileBackend {
    async fn send(&self, message: &EmailMessage) -> Result<(), DjangoError> {
        tokio::fs::create_dir_all(&self.dir).await?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S_%f");
        let filename = format!("{timestamp}.eml");
        let path = self.dir.join(filename);

        let content = message.format_message();
        tokio::fs::write(&path, content).await?;

        tracing::debug!("Email written to {}", path.display());

        Ok(())
    }
}

/// An email backend that collects emails in memory.
///
/// All sent emails are stored in a thread-safe vector that can be
/// inspected in tests. This mirrors Django's `locmem` email backend.
#[derive(Debug, Clone, Default)]
pub struct InMemoryBackend {
    /// The collected emails.
    messages: Arc<RwLock<Vec<EmailMessage>>>,
}

impl InMemoryBackend {
    /// Creates a new in-memory email backend.
    pub fn new() -> Self {
        Self {
            messages: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Returns a copy of all sent messages.
    pub async fn get_messages(&self) -> Vec<EmailMessage> {
        self.messages.read().await.clone()
    }

    /// Returns the number of sent messages.
    pub async fn message_count(&self) -> usize {
        self.messages.read().await.len()
    }

    /// Clears all stored messages.
    pub async fn clear(&self) {
        self.messages.write().await.clear();
    }
}

#[async_trait]
impl EmailBackend for InMemoryBackend {
    async fn send(&self, message: &EmailMessage) -> Result<(), DjangoError> {
        if message.to.is_empty() {
            return Err(DjangoError::BadRequest(
                "Email must have at least one recipient".to_string(),
            ));
        }

        self.messages.write().await.push(message.clone());
        Ok(())
    }
}

/// Sends a single email message via the given backend.
///
/// This is the primary convenience function for sending email, mirroring
/// Django's `send_mail()`. It constructs an `EmailMessage` from the given
/// parameters and sends it through the provided backend.
///
/// # Examples
///
/// ```rust,no_run
/// # use django_rs_cli::email::{send_mail, InMemoryBackend};
/// # async fn example() {
/// let backend = InMemoryBackend::new();
/// send_mail(
///     "Welcome!",
///     "Thanks for signing up.",
///     "noreply@example.com",
///     &["user@example.com".to_string()],
///     &backend,
/// ).await.unwrap();
/// # }
/// ```
pub async fn send_mail(
    subject: impl Into<String>,
    message: impl Into<String>,
    from_email: impl Into<String>,
    recipient_list: &[String],
    backend: &dyn EmailBackend,
) -> Result<(), DjangoError> {
    let msg = EmailMessage::new(subject, message, from_email, recipient_list.to_vec());
    backend.send(&msg).await
}

/// Sends multiple email messages via the given backend.
///
/// Each tuple in `datatuple` contains `(subject, message, from_email, recipient_list)`.
/// Returns the number of successfully sent messages. Mirrors Django's `send_mass_mail()`.
pub async fn send_mass_mail(
    datatuple: &[(String, String, String, Vec<String>)],
    backend: &dyn EmailBackend,
) -> Result<usize, DjangoError> {
    let messages: Vec<EmailMessage> = datatuple
        .iter()
        .map(|(subject, body, from_email, to)| {
            EmailMessage::new(
                subject.clone(),
                body.clone(),
                from_email.clone(),
                to.clone(),
            )
        })
        .collect();

    backend.send_many(&messages).await
}

/// Creates an `SmtpBackend` from the framework's email settings.
///
/// Reads `email_host` and `email_port` from the settings to construct
/// the backend. This is the factory function used by the framework
/// to create the default email backend.
pub fn get_connection(settings: &django_rs_core::Settings) -> SmtpBackend {
    SmtpBackend::new(&settings.email_host, settings.email_port)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_email() -> EmailMessage {
        EmailMessage::new(
            "Test Subject",
            "Test body",
            "sender@example.com",
            vec!["recipient@example.com".to_string()],
        )
    }

    // ── EmailMessage tests ────────────────────────────────────────────

    #[test]
    fn test_email_message_new() {
        let msg = sample_email();
        assert_eq!(msg.subject, "Test Subject");
        assert_eq!(msg.body, "Test body");
        assert_eq!(msg.from_email, "sender@example.com");
        assert_eq!(msg.to, vec!["recipient@example.com"]);
        assert!(msg.cc.is_empty());
        assert!(msg.bcc.is_empty());
        assert!(msg.reply_to.is_empty());
        assert!(msg.headers.is_empty());
        assert!(msg.html_body.is_none());
    }

    #[test]
    fn test_email_recipients() {
        let mut msg = sample_email();
        msg.cc = vec!["cc@example.com".to_string()];
        msg.bcc = vec!["bcc@example.com".to_string()];

        let recipients = msg.recipients();
        assert_eq!(recipients.len(), 3);
        assert!(recipients.contains(&"recipient@example.com"));
        assert!(recipients.contains(&"cc@example.com"));
        assert!(recipients.contains(&"bcc@example.com"));
    }

    #[test]
    fn test_email_format_message() {
        let mut msg = sample_email();
        msg.cc = vec!["cc@example.com".to_string()];
        msg.html_body = Some("<p>HTML body</p>".to_string());

        let formatted = msg.format_message();
        assert!(formatted.contains("From: sender@example.com"));
        assert!(formatted.contains("To: recipient@example.com"));
        assert!(formatted.contains("Cc: cc@example.com"));
        assert!(formatted.contains("Subject: Test Subject"));
        assert!(formatted.contains("Test body"));
        assert!(formatted.contains("<p>HTML body</p>"));
    }

    #[test]
    fn test_email_format_message_minimal() {
        let msg = sample_email();
        let formatted = msg.format_message();
        assert!(formatted.contains("From:"));
        assert!(formatted.contains("To:"));
        assert!(formatted.contains("Subject:"));
        assert!(!formatted.contains("Cc:"));
        assert!(!formatted.contains("Bcc:"));
    }

    // ── SmtpBackend tests ─────────────────────────────────────────────

    #[test]
    fn test_smtp_backend_new() {
        let backend = SmtpBackend::new("smtp.example.com", 587);
        assert_eq!(backend.host, "smtp.example.com");
        assert_eq!(backend.port, 587);
        assert!(!backend.use_tls);
        assert!(backend.username.is_none());
    }

    #[test]
    fn test_smtp_backend_with_tls() {
        let backend = SmtpBackend::new("smtp.example.com", 465).with_tls();
        assert!(backend.use_tls);
    }

    #[test]
    fn test_smtp_backend_with_credentials() {
        let backend = SmtpBackend::new("smtp.example.com", 587).with_credentials("user", "pass");
        assert_eq!(backend.username.as_deref(), Some("user"));
        assert_eq!(backend.password.as_deref(), Some("pass"));
    }

    #[tokio::test]
    async fn test_smtp_backend_send() {
        let backend = SmtpBackend::new("localhost", 25);
        let msg = sample_email();
        let result = backend.send(&msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_smtp_backend_send_no_recipients() {
        let backend = SmtpBackend::new("localhost", 25);
        let msg = EmailMessage::new("Subject", "Body", "from@test.com", vec![]);
        let result = backend.send(&msg).await;
        assert!(result.is_err());
    }

    // ── ConsoleBackend tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_console_backend_send() {
        let backend = ConsoleBackend;
        let msg = sample_email();
        let result = backend.send(&msg).await;
        assert!(result.is_ok());
    }

    // ── FileBackend tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_file_backend_send() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());

        let msg = sample_email();
        backend.send(&msg).await.unwrap();

        // Verify a file was created
        let mut entries = tokio::fs::read_dir(dir.path()).await.unwrap();
        let entry = entries.next_entry().await.unwrap();
        assert!(entry.is_some());

        let path = entry.unwrap().path();
        assert!(path.extension().is_some_and(|ext| ext == "eml"));

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("Test Subject"));
        assert!(content.contains("Test body"));
    }

    #[tokio::test]
    async fn test_file_backend_multiple_sends() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());

        backend.send(&sample_email()).await.unwrap();
        // Small delay to ensure different timestamps
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        backend.send(&sample_email()).await.unwrap();

        let mut count = 0;
        let mut entries = tokio::fs::read_dir(dir.path()).await.unwrap();
        while entries.next_entry().await.unwrap().is_some() {
            count += 1;
        }
        assert_eq!(count, 2);
    }

    // ── InMemoryBackend tests ─────────────────────────────────────────

    #[tokio::test]
    async fn test_inmemory_backend_send() {
        let backend = InMemoryBackend::new();

        let msg = sample_email();
        backend.send(&msg).await.unwrap();

        assert_eq!(backend.message_count().await, 1);
        let messages = backend.get_messages().await;
        assert_eq!(messages[0].subject, "Test Subject");
    }

    #[tokio::test]
    async fn test_inmemory_backend_send_many() {
        let backend = InMemoryBackend::new();

        let msg1 = EmailMessage::new(
            "Subject 1",
            "Body 1",
            "from@test.com",
            vec!["to@test.com".to_string()],
        );
        let msg2 = EmailMessage::new(
            "Subject 2",
            "Body 2",
            "from@test.com",
            vec!["to@test.com".to_string()],
        );

        let count = backend.send_many(&[msg1, msg2]).await.unwrap();
        assert_eq!(count, 2);
        assert_eq!(backend.message_count().await, 2);
    }

    #[tokio::test]
    async fn test_inmemory_backend_send_no_recipients() {
        let backend = InMemoryBackend::new();
        let msg = EmailMessage::new("Subject", "Body", "from@test.com", vec![]);
        let result = backend.send(&msg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_inmemory_backend_clear() {
        let backend = InMemoryBackend::new();
        backend.send(&sample_email()).await.unwrap();
        assert_eq!(backend.message_count().await, 1);

        backend.clear().await;
        assert_eq!(backend.message_count().await, 0);
    }

    #[tokio::test]
    async fn test_inmemory_backend_default() {
        let backend = InMemoryBackend::default();
        assert_eq!(backend.message_count().await, 0);
    }

    // ── send_many default implementation ──────────────────────────────

    #[tokio::test]
    async fn test_send_many_with_failures() {
        let backend = InMemoryBackend::new();

        let good = sample_email();
        let bad = EmailMessage::new("Subject", "Body", "from@test.com", vec![]);

        let count = backend.send_many(&[good, bad]).await.unwrap();
        assert_eq!(count, 1);
    }

    // ── Attachment tests ─────────────────────────────────────────────

    #[test]
    fn test_attachment_new() {
        let att = Attachment::new("report.pdf", vec![1, 2, 3], "application/pdf");
        assert_eq!(att.filename, "report.pdf");
        assert_eq!(att.content, vec![1, 2, 3]);
        assert_eq!(att.mimetype, "application/pdf");
    }

    #[test]
    fn test_email_with_attachment() {
        let msg = sample_email().with_attachment(Attachment::new(
            "file.txt",
            b"hello".to_vec(),
            "text/plain",
        ));
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].filename, "file.txt");
    }

    #[test]
    fn test_email_with_html_body() {
        let msg = sample_email().with_html_body("<h1>Hello</h1>");
        assert_eq!(msg.html_body.as_deref(), Some("<h1>Hello</h1>"));
    }

    #[test]
    fn test_format_message_with_attachments() {
        let msg = sample_email().with_attachment(Attachment::new(
            "doc.pdf",
            vec![0; 1024],
            "application/pdf",
        ));
        let formatted = msg.format_message();
        assert!(formatted.contains("--- Attachments ---"));
        assert!(formatted.contains("doc.pdf"));
        assert!(formatted.contains("1024 bytes"));
    }

    // ── send_mail tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_send_mail() {
        let backend = InMemoryBackend::new();
        send_mail(
            "Hello",
            "World",
            "from@test.com",
            &["to@test.com".to_string()],
            &backend,
        )
        .await
        .unwrap();

        assert_eq!(backend.message_count().await, 1);
        let messages = backend.get_messages().await;
        assert_eq!(messages[0].subject, "Hello");
        assert_eq!(messages[0].body, "World");
    }

    #[tokio::test]
    async fn test_send_mail_no_recipients() {
        let backend = InMemoryBackend::new();
        let result = send_mail("Hello", "World", "from@test.com", &[], &backend).await;
        assert!(result.is_err());
    }

    // ── send_mass_mail tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_send_mass_mail() {
        let backend = InMemoryBackend::new();
        let data = vec![
            (
                "Subject 1".to_string(),
                "Body 1".to_string(),
                "from@test.com".to_string(),
                vec!["to1@test.com".to_string()],
            ),
            (
                "Subject 2".to_string(),
                "Body 2".to_string(),
                "from@test.com".to_string(),
                vec!["to2@test.com".to_string()],
            ),
        ];

        let count = send_mass_mail(&data, &backend).await.unwrap();
        assert_eq!(count, 2);
        assert_eq!(backend.message_count().await, 2);
    }

    #[tokio::test]
    async fn test_send_mass_mail_empty() {
        let backend = InMemoryBackend::new();
        let count = send_mass_mail(&[], &backend).await.unwrap();
        assert_eq!(count, 0);
    }

    // ── get_connection tests ────────────────────────────────────────

    #[test]
    fn test_get_connection() {
        let settings = django_rs_core::Settings {
            email_host: "smtp.example.com".to_string(),
            email_port: 587,
            ..django_rs_core::Settings::default()
        };

        let backend = get_connection(&settings);
        assert_eq!(backend.host, "smtp.example.com");
        assert_eq!(backend.port, 587);
    }
}
