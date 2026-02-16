//! Syndication framework for RSS and Atom feed generation.
//!
//! Provides the [`Feed`] trait for defining feed content and functions to
//! generate valid RSS 2.0 and Atom 1.0 XML. A feed view renders the feed
//! as an HTTP response with the appropriate content type.
//!
//! This mirrors Django's `django.contrib.syndication.views` module.
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_views::contrib::syndication::{Feed, FeedItem, generate_rss, generate_atom};
//!
//! struct BlogFeed;
//!
//! impl Feed for BlogFeed {
//!     fn title(&self) -> String { "My Blog".to_string() }
//!     fn link(&self) -> String { "https://example.com/".to_string() }
//!     fn description(&self) -> String { "Latest blog posts".to_string() }
//!     fn items(&self) -> Vec<FeedItem> {
//!         vec![FeedItem {
//!             title: "First Post".to_string(),
//!             link: "https://example.com/post/1/".to_string(),
//!             description: "My first post.".to_string(),
//!             pub_date: None,
//!             author: None,
//!             guid: None,
//!             categories: Vec::new(),
//!         }]
//!     }
//! }
//!
//! let rss = generate_rss(&BlogFeed);
//! assert!(rss.contains("<rss version=\"2.0\""));
//! assert!(rss.contains("<title>My Blog</title>"));
//!
//! let atom = generate_atom(&BlogFeed);
//! assert!(atom.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\""));
//! ```

use std::fmt::Write;

use django_rs_http::HttpResponse;

/// A single item/entry in a feed.
#[derive(Debug, Clone)]
pub struct FeedItem {
    /// The title of this item.
    pub title: String,
    /// The URL link for this item.
    pub link: String,
    /// A description or summary of this item.
    pub description: String,
    /// The publication date in RFC 2822 format (e.g., "Sat, 15 Jun 2024 12:00:00 +0000").
    pub pub_date: Option<String>,
    /// The author name or email.
    pub author: Option<String>,
    /// A unique identifier for this item. If not provided, the link is used.
    pub guid: Option<String>,
    /// Category labels for this item.
    pub categories: Vec<String>,
}

impl FeedItem {
    /// Creates a new feed item with required fields.
    pub fn new(
        title: impl Into<String>,
        link: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            link: link.into(),
            description: description.into(),
            pub_date: None,
            author: None,
            guid: None,
            categories: Vec::new(),
        }
    }

    /// Sets the publication date.
    #[must_use]
    pub fn with_pub_date(mut self, date: impl Into<String>) -> Self {
        self.pub_date = Some(date.into());
        self
    }

    /// Sets the author.
    #[must_use]
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Sets the GUID.
    #[must_use]
    pub fn with_guid(mut self, guid: impl Into<String>) -> Self {
        self.guid = Some(guid.into());
        self
    }

    /// Sets the categories.
    #[must_use]
    pub fn with_categories(mut self, categories: Vec<String>) -> Self {
        self.categories = categories;
        self
    }
}

/// Trait for defining feed content.
///
/// Implement this trait to define the metadata and items for an RSS or Atom feed.
/// This mirrors Django's `Feed` class.
pub trait Feed {
    /// The feed title.
    fn title(&self) -> String;

    /// The feed link (URL of the site).
    fn link(&self) -> String;

    /// The feed description/subtitle.
    fn description(&self) -> String;

    /// The items in this feed.
    fn items(&self) -> Vec<FeedItem>;

    /// The feed language code (optional, e.g., "en-us").
    fn language(&self) -> Option<String> {
        None
    }

    /// The feed author name (for Atom feeds).
    fn author_name(&self) -> Option<String> {
        None
    }

    /// The feed copyright.
    fn copyright(&self) -> Option<String> {
        None
    }

    /// The URL of the feed itself (for Atom self-link).
    fn feed_url(&self) -> Option<String> {
        None
    }
}

/// Generates an RSS 2.0 XML document from a feed.
///
/// Produces a well-formed XML document conforming to the RSS 2.0 specification.
///
/// # Examples
///
/// ```
/// use django_rs_views::contrib::syndication::{Feed, FeedItem, generate_rss};
///
/// struct MyFeed;
/// impl Feed for MyFeed {
///     fn title(&self) -> String { "Test".to_string() }
///     fn link(&self) -> String { "https://example.com/".to_string() }
///     fn description(&self) -> String { "A test feed".to_string() }
///     fn items(&self) -> Vec<FeedItem> { vec![] }
/// }
///
/// let xml = generate_rss(&MyFeed);
/// assert!(xml.starts_with("<?xml"));
/// assert!(xml.contains("<rss version=\"2.0\""));
/// ```
pub fn generate_rss(feed: &dyn Feed) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    xml.push_str("<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">\n");
    xml.push_str("  <channel>\n");
    let _ = writeln!(xml, "    <title>{}</title>", xml_escape(&feed.title()));
    let _ = writeln!(xml, "    <link>{}</link>", xml_escape(&feed.link()));
    let _ = writeln!(
        xml,
        "    <description>{}</description>",
        xml_escape(&feed.description())
    );

    if let Some(lang) = feed.language() {
        let _ = writeln!(xml, "    <language>{}</language>", xml_escape(&lang));
    }

    if let Some(copyright) = feed.copyright() {
        let _ = writeln!(xml, "    <copyright>{}</copyright>", xml_escape(&copyright));
    }

    if let Some(feed_url) = feed.feed_url() {
        let _ = writeln!(
            xml,
            "    <atom:link href=\"{}\" rel=\"self\" type=\"application/rss+xml\"/>",
            xml_escape(&feed_url)
        );
    }

    for item in &feed.items() {
        xml.push_str("    <item>\n");
        let _ = writeln!(xml, "      <title>{}</title>", xml_escape(&item.title));
        let _ = writeln!(xml, "      <link>{}</link>", xml_escape(&item.link));
        let _ = writeln!(
            xml,
            "      <description>{}</description>",
            xml_escape(&item.description)
        );

        if let Some(ref pub_date) = item.pub_date {
            let _ = writeln!(xml, "      <pubDate>{}</pubDate>", xml_escape(pub_date));
        }

        if let Some(ref author) = item.author {
            let _ = writeln!(xml, "      <author>{}</author>", xml_escape(author));
        }

        let guid = item.guid.as_deref().unwrap_or(&item.link);
        let _ = writeln!(xml, "      <guid>{}</guid>", xml_escape(guid));

        for category in &item.categories {
            let _ = writeln!(xml, "      <category>{}</category>", xml_escape(category));
        }

        xml.push_str("    </item>\n");
    }

    xml.push_str("  </channel>\n");
    xml.push_str("</rss>\n");
    xml
}

/// Generates an Atom 1.0 XML document from a feed.
///
/// Produces a well-formed XML document conforming to the Atom 1.0 specification (RFC 4287).
///
/// # Examples
///
/// ```
/// use django_rs_views::contrib::syndication::{Feed, FeedItem, generate_atom};
///
/// struct MyFeed;
/// impl Feed for MyFeed {
///     fn title(&self) -> String { "Test".to_string() }
///     fn link(&self) -> String { "https://example.com/".to_string() }
///     fn description(&self) -> String { "A test feed".to_string() }
///     fn items(&self) -> Vec<FeedItem> { vec![] }
/// }
///
/// let xml = generate_atom(&MyFeed);
/// assert!(xml.starts_with("<?xml"));
/// assert!(xml.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\""));
/// ```
pub fn generate_atom(feed: &dyn Feed) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    xml.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">\n");
    let _ = writeln!(xml, "  <title>{}</title>", xml_escape(&feed.title()));
    let _ = writeln!(xml, "  <link href=\"{}\"/>", xml_escape(&feed.link()));
    let _ = writeln!(
        xml,
        "  <subtitle>{}</subtitle>",
        xml_escape(&feed.description())
    );

    if let Some(feed_url) = feed.feed_url() {
        let _ = writeln!(
            xml,
            "  <link href=\"{}\" rel=\"self\"/>",
            xml_escape(&feed_url)
        );
    }

    // Generate a feed ID from the link
    let _ = writeln!(xml, "  <id>{}</id>", xml_escape(&feed.link()));

    // Updated timestamp (use current time if not available)
    let _ = writeln!(
        xml,
        "  <updated>{}</updated>",
        chrono::Utc::now().to_rfc3339()
    );

    if let Some(author) = feed.author_name() {
        xml.push_str("  <author>\n");
        let _ = writeln!(xml, "    <name>{}</name>", xml_escape(&author));
        xml.push_str("  </author>\n");
    }

    if let Some(copyright) = feed.copyright() {
        let _ = writeln!(xml, "  <rights>{}</rights>", xml_escape(&copyright));
    }

    for item in &feed.items() {
        xml.push_str("  <entry>\n");
        let _ = writeln!(xml, "    <title>{}</title>", xml_escape(&item.title));
        let _ = writeln!(xml, "    <link href=\"{}\"/>", xml_escape(&item.link));

        let id = item.guid.as_deref().unwrap_or(&item.link);
        let _ = writeln!(xml, "    <id>{}</id>", xml_escape(id));
        let _ = writeln!(
            xml,
            "    <summary>{}</summary>",
            xml_escape(&item.description)
        );

        if let Some(ref pub_date) = item.pub_date {
            let _ = writeln!(xml, "    <updated>{}</updated>", xml_escape(pub_date));
        }

        if let Some(ref author) = item.author {
            xml.push_str("    <author>\n");
            let _ = writeln!(xml, "      <name>{}</name>", xml_escape(author));
            xml.push_str("    </author>\n");
        }

        for category in &item.categories {
            let _ = writeln!(xml, "    <category term=\"{}\"/>", xml_escape(category));
        }

        xml.push_str("  </entry>\n");
    }

    xml.push_str("</feed>\n");
    xml
}

/// Creates an HTTP response with RSS content.
///
/// Sets the content type to `application/rss+xml`.
pub fn rss_response(feed: &dyn Feed) -> HttpResponse {
    let xml = generate_rss(feed);
    let mut response = HttpResponse::ok(xml);
    response.set_content_type("application/rss+xml");
    response
}

/// Creates an HTTP response with Atom content.
///
/// Sets the content type to `application/atom+xml`.
pub fn atom_response(feed: &dyn Feed) -> HttpResponse {
    let xml = generate_atom(feed);
    let mut response = HttpResponse::ok(xml);
    response.set_content_type("application/atom+xml");
    response
}

/// Escapes special XML characters.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFeed;

    impl Feed for TestFeed {
        fn title(&self) -> String {
            "Test Blog".to_string()
        }
        fn link(&self) -> String {
            "https://example.com/".to_string()
        }
        fn description(&self) -> String {
            "A test blog feed".to_string()
        }
        fn items(&self) -> Vec<FeedItem> {
            vec![
                FeedItem::new(
                    "First Post",
                    "https://example.com/post/1/",
                    "This is my first post.",
                )
                .with_pub_date("Sat, 15 Jun 2024 12:00:00 +0000")
                .with_author("author@example.com")
                .with_categories(vec!["Tech".to_string(), "Rust".to_string()]),
                FeedItem::new(
                    "Second Post",
                    "https://example.com/post/2/",
                    "This is my second post.",
                )
                .with_guid("unique-id-2"),
            ]
        }
        fn language(&self) -> Option<String> {
            Some("en-us".to_string())
        }
        fn author_name(&self) -> Option<String> {
            Some("Test Author".to_string())
        }
        fn copyright(&self) -> Option<String> {
            Some("2024 Test".to_string())
        }
        fn feed_url(&self) -> Option<String> {
            Some("https://example.com/feed/".to_string())
        }
    }

    struct MinimalFeed;

    impl Feed for MinimalFeed {
        fn title(&self) -> String {
            "Minimal".to_string()
        }
        fn link(&self) -> String {
            "https://example.com/".to_string()
        }
        fn description(&self) -> String {
            "Minimal feed".to_string()
        }
        fn items(&self) -> Vec<FeedItem> {
            Vec::new()
        }
    }

    // ── RSS tests ────────────────────────────────────────────────────

    #[test]
    fn test_generate_rss_xml_declaration() {
        let xml = generate_rss(&TestFeed);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>"));
    }

    #[test]
    fn test_generate_rss_root_element() {
        let xml = generate_rss(&TestFeed);
        assert!(xml.contains("<rss version=\"2.0\""));
        assert!(xml.contains("</rss>"));
    }

    #[test]
    fn test_generate_rss_channel_metadata() {
        let xml = generate_rss(&TestFeed);
        assert!(xml.contains("<title>Test Blog</title>"));
        assert!(xml.contains("<link>https://example.com/</link>"));
        assert!(xml.contains("<description>A test blog feed</description>"));
        assert!(xml.contains("<language>en-us</language>"));
        assert!(xml.contains("<copyright>2024 Test</copyright>"));
    }

    #[test]
    fn test_generate_rss_self_link() {
        let xml = generate_rss(&TestFeed);
        assert!(xml.contains("atom:link href=\"https://example.com/feed/\""));
        assert!(xml.contains("rel=\"self\""));
    }

    #[test]
    fn test_generate_rss_items() {
        let xml = generate_rss(&TestFeed);
        assert!(xml.contains("<item>"));
        assert!(xml.contains("<title>First Post</title>"));
        assert!(xml.contains("<link>https://example.com/post/1/</link>"));
        assert!(xml.contains("<description>This is my first post.</description>"));
        assert!(xml.contains("<pubDate>Sat, 15 Jun 2024 12:00:00 +0000</pubDate>"));
        assert!(xml.contains("<author>author@example.com</author>"));
        assert!(xml.contains("<category>Tech</category>"));
        assert!(xml.contains("<category>Rust</category>"));
    }

    #[test]
    fn test_generate_rss_item_guid() {
        let xml = generate_rss(&TestFeed);
        // First item has no explicit GUID, uses link
        assert!(xml.contains("<guid>https://example.com/post/1/</guid>"));
        // Second item has explicit GUID
        assert!(xml.contains("<guid>unique-id-2</guid>"));
    }

    #[test]
    fn test_generate_rss_second_item() {
        let xml = generate_rss(&TestFeed);
        assert!(xml.contains("<title>Second Post</title>"));
        assert!(xml.contains("<link>https://example.com/post/2/</link>"));
    }

    #[test]
    fn test_generate_rss_minimal() {
        let xml = generate_rss(&MinimalFeed);
        assert!(xml.contains("<title>Minimal</title>"));
        assert!(!xml.contains("<item>"));
        assert!(!xml.contains("<language>"));
        assert!(!xml.contains("<copyright>"));
    }

    // ── Atom tests ───────────────────────────────────────────────────

    #[test]
    fn test_generate_atom_xml_declaration() {
        let xml = generate_atom(&TestFeed);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>"));
    }

    #[test]
    fn test_generate_atom_root_element() {
        let xml = generate_atom(&TestFeed);
        assert!(xml.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\""));
        assert!(xml.contains("</feed>"));
    }

    #[test]
    fn test_generate_atom_metadata() {
        let xml = generate_atom(&TestFeed);
        assert!(xml.contains("<title>Test Blog</title>"));
        assert!(xml.contains("<link href=\"https://example.com/\"/>"));
        assert!(xml.contains("<subtitle>A test blog feed</subtitle>"));
        assert!(xml.contains("<id>https://example.com/</id>"));
        assert!(xml.contains("<updated>"));
        assert!(xml.contains("<rights>2024 Test</rights>"));
    }

    #[test]
    fn test_generate_atom_author() {
        let xml = generate_atom(&TestFeed);
        assert!(xml.contains("<author>"));
        assert!(xml.contains("<name>Test Author</name>"));
    }

    #[test]
    fn test_generate_atom_self_link() {
        let xml = generate_atom(&TestFeed);
        assert!(xml.contains("href=\"https://example.com/feed/\" rel=\"self\""));
    }

    #[test]
    fn test_generate_atom_entries() {
        let xml = generate_atom(&TestFeed);
        assert!(xml.contains("<entry>"));
        assert!(xml.contains("<title>First Post</title>"));
        assert!(xml.contains("<link href=\"https://example.com/post/1/\"/>"));
        assert!(xml.contains("<summary>This is my first post.</summary>"));
        assert!(xml.contains("<updated>Sat, 15 Jun 2024 12:00:00 +0000</updated>"));
    }

    #[test]
    fn test_generate_atom_entry_author() {
        let xml = generate_atom(&TestFeed);
        // The first entry has an author
        assert!(xml.contains("<name>author@example.com</name>"));
    }

    #[test]
    fn test_generate_atom_entry_categories() {
        let xml = generate_atom(&TestFeed);
        assert!(xml.contains("<category term=\"Tech\"/>"));
        assert!(xml.contains("<category term=\"Rust\"/>"));
    }

    #[test]
    fn test_generate_atom_entry_id() {
        let xml = generate_atom(&TestFeed);
        assert!(xml.contains("<id>https://example.com/post/1/</id>"));
        assert!(xml.contains("<id>unique-id-2</id>"));
    }

    #[test]
    fn test_generate_atom_minimal() {
        let xml = generate_atom(&MinimalFeed);
        assert!(xml.contains("<title>Minimal</title>"));
        assert!(!xml.contains("<entry>"));
        assert!(!xml.contains("<rights>"));
    }

    // ── XML escaping ─────────────────────────────────────────────────

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("a & b"), "a &amp; b");
        assert_eq!(xml_escape("<script>"), "&lt;script&gt;");
        assert_eq!(xml_escape("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(xml_escape("it's"), "it&apos;s");
    }

    #[test]
    fn test_rss_escapes_special_chars() {
        struct XssFeed;
        impl Feed for XssFeed {
            fn title(&self) -> String {
                "Feed & <More>".to_string()
            }
            fn link(&self) -> String {
                "https://example.com/".to_string()
            }
            fn description(&self) -> String {
                "Test \"description\"".to_string()
            }
            fn items(&self) -> Vec<FeedItem> {
                Vec::new()
            }
        }

        let xml = generate_rss(&XssFeed);
        assert!(xml.contains("Feed &amp; &lt;More&gt;"));
        assert!(xml.contains("Test &quot;description&quot;"));
    }

    // ── FeedItem builder ─────────────────────────────────────────────

    #[test]
    fn test_feed_item_new() {
        let item = FeedItem::new("Title", "https://example.com/", "Desc");
        assert_eq!(item.title, "Title");
        assert_eq!(item.link, "https://example.com/");
        assert_eq!(item.description, "Desc");
        assert!(item.pub_date.is_none());
        assert!(item.author.is_none());
        assert!(item.guid.is_none());
        assert!(item.categories.is_empty());
    }

    #[test]
    fn test_feed_item_builder_chain() {
        let item = FeedItem::new("Title", "https://example.com/", "Desc")
            .with_pub_date("Mon, 01 Jan 2024 00:00:00 +0000")
            .with_author("author@test.com")
            .with_guid("guid-123")
            .with_categories(vec!["A".to_string(), "B".to_string()]);

        assert_eq!(item.pub_date.unwrap(), "Mon, 01 Jan 2024 00:00:00 +0000");
        assert_eq!(item.author.unwrap(), "author@test.com");
        assert_eq!(item.guid.unwrap(), "guid-123");
        assert_eq!(item.categories, vec!["A", "B"]);
    }

    #[test]
    fn test_feed_item_clone() {
        let item = FeedItem::new("Title", "https://example.com/", "Desc").with_author("author");
        let cloned = item.clone();
        assert_eq!(item.title, cloned.title);
        assert_eq!(item.author, cloned.author);
    }

    // ── Response helpers ─────────────────────────────────────────────

    #[test]
    fn test_rss_response() {
        let response = rss_response(&TestFeed);
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(response.content_type(), "application/rss+xml");
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("<rss version=\"2.0\""));
    }

    #[test]
    fn test_atom_response() {
        let response = atom_response(&TestFeed);
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(response.content_type(), "application/atom+xml");
        let body = String::from_utf8(response.content_bytes().unwrap()).unwrap();
        assert!(body.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\""));
    }

    // ── Default Feed trait methods ───────────────────────────────────

    #[test]
    fn test_feed_defaults() {
        assert!(MinimalFeed.language().is_none());
        assert!(MinimalFeed.author_name().is_none());
        assert!(MinimalFeed.copyright().is_none());
        assert!(MinimalFeed.feed_url().is_none());
    }
}
