//! Sitemap generation framework.
//!
//! Generates XML sitemaps following the [sitemaps.org protocol](https://www.sitemaps.org/protocol.html).
//! Mirrors Django's `django.contrib.sitemaps`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How frequently a page is likely to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeFreq {
    /// The page changes every time it is accessed.
    Always,
    /// The page changes hourly.
    Hourly,
    /// The page changes daily.
    Daily,
    /// The page changes weekly.
    Weekly,
    /// The page changes monthly.
    Monthly,
    /// The page changes yearly.
    Yearly,
    /// The page is archived and will not change.
    Never,
}

impl ChangeFreq {
    /// Returns the XML string representation.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
            Self::Never => "never",
        }
    }
}

impl std::fmt::Display for ChangeFreq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single entry in a sitemap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SitemapEntry {
    /// The URL of the page.
    pub location: String,
    /// The date the page was last modified.
    pub lastmod: Option<DateTime<Utc>>,
    /// How frequently the page changes.
    pub changefreq: Option<ChangeFreq>,
    /// The priority of this URL relative to other URLs on the site (0.0 to 1.0).
    pub priority: Option<f32>,
}

impl SitemapEntry {
    /// Creates a new sitemap entry with just a location.
    pub fn new(location: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            lastmod: None,
            changefreq: None,
            priority: None,
        }
    }

    /// Sets the last modification date.
    #[must_use]
    pub const fn lastmod(mut self, dt: DateTime<Utc>) -> Self {
        self.lastmod = Some(dt);
        self
    }

    /// Sets the change frequency.
    #[must_use]
    pub const fn changefreq(mut self, freq: ChangeFreq) -> Self {
        self.changefreq = Some(freq);
        self
    }

    /// Sets the priority.
    #[must_use]
    pub const fn priority(mut self, priority: f32) -> Self {
        self.priority = Some(priority);
        self
    }
}

/// A collection of sitemap entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sitemap {
    /// The entries in this sitemap.
    pub entries: Vec<SitemapEntry>,
}

impl Sitemap {
    /// Creates a new empty sitemap.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an entry to the sitemap.
    pub fn add(&mut self, entry: SitemapEntry) {
        self.entries.push(entry);
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the sitemap has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Renders a sitemap as XML following the sitemaps.org protocol.
///
/// # Examples
///
/// ```
/// use django_rs_admin::contrib::sitemaps::{Sitemap, SitemapEntry, ChangeFreq, render_sitemap_xml};
///
/// let mut sitemap = Sitemap::new();
/// sitemap.add(SitemapEntry::new("https://example.com/")
///     .changefreq(ChangeFreq::Daily)
///     .priority(1.0));
///
/// let xml = render_sitemap_xml(&sitemap);
/// assert!(xml.contains("<loc>https://example.com/</loc>"));
/// ```
pub fn render_sitemap_xml(sitemap: &Sitemap) -> String {
    use std::fmt::Write;

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );

    for entry in &sitemap.entries {
        xml.push_str("  <url>\n");
        let _ = writeln!(xml, "    <loc>{}</loc>", escape_xml(&entry.location));

        if let Some(ref lastmod) = entry.lastmod {
            let _ = writeln!(
                xml,
                "    <lastmod>{}</lastmod>",
                lastmod.format("%Y-%m-%dT%H:%M:%S+00:00")
            );
        }

        if let Some(ref changefreq) = entry.changefreq {
            let _ = writeln!(xml, "    <changefreq>{changefreq}</changefreq>");
        }

        if let Some(priority) = entry.priority {
            let _ = writeln!(xml, "    <priority>{priority:.1}</priority>");
        }

        xml.push_str("  </url>\n");
    }

    xml.push_str("</urlset>\n");
    xml
}

/// Escapes special XML characters in a string.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Renders a sitemap index XML for multiple sitemaps.
///
/// Used when the site has more than 50,000 URLs and needs to split
/// across multiple sitemap files.
pub fn render_sitemap_index_xml(sitemap_urls: &[&str]) -> String {
    use std::fmt::Write;

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );

    for url in sitemap_urls {
        xml.push_str("  <sitemap>\n");
        let _ = writeln!(xml, "    <loc>{}</loc>", escape_xml(url));
        xml.push_str("  </sitemap>\n");
    }

    xml.push_str("</sitemapindex>\n");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_change_freq_as_str() {
        assert_eq!(ChangeFreq::Always.as_str(), "always");
        assert_eq!(ChangeFreq::Hourly.as_str(), "hourly");
        assert_eq!(ChangeFreq::Daily.as_str(), "daily");
        assert_eq!(ChangeFreq::Weekly.as_str(), "weekly");
        assert_eq!(ChangeFreq::Monthly.as_str(), "monthly");
        assert_eq!(ChangeFreq::Yearly.as_str(), "yearly");
        assert_eq!(ChangeFreq::Never.as_str(), "never");
    }

    #[test]
    fn test_change_freq_display() {
        assert_eq!(ChangeFreq::Daily.to_string(), "daily");
    }

    #[test]
    fn test_sitemap_entry_new() {
        let entry = SitemapEntry::new("https://example.com/");
        assert_eq!(entry.location, "https://example.com/");
        assert!(entry.lastmod.is_none());
        assert!(entry.changefreq.is_none());
        assert!(entry.priority.is_none());
    }

    #[test]
    fn test_sitemap_entry_builder() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let entry = SitemapEntry::new("https://example.com/page")
            .lastmod(dt)
            .changefreq(ChangeFreq::Weekly)
            .priority(0.8);
        assert_eq!(entry.lastmod, Some(dt));
        assert_eq!(entry.changefreq, Some(ChangeFreq::Weekly));
        assert_eq!(entry.priority, Some(0.8));
    }

    #[test]
    fn test_sitemap_new() {
        let sitemap = Sitemap::new();
        assert!(sitemap.is_empty());
        assert_eq!(sitemap.len(), 0);
    }

    #[test]
    fn test_sitemap_add() {
        let mut sitemap = Sitemap::new();
        sitemap.add(SitemapEntry::new("https://example.com/"));
        assert_eq!(sitemap.len(), 1);
        assert!(!sitemap.is_empty());
    }

    #[test]
    fn test_render_sitemap_xml_empty() {
        let sitemap = Sitemap::new();
        let xml = render_sitemap_xml(&sitemap);
        assert!(xml.contains("<?xml"));
        assert!(xml.contains("<urlset"));
        assert!(xml.contains("</urlset>"));
        assert!(!xml.contains("<url>"));
    }

    #[test]
    fn test_render_sitemap_xml_basic() {
        let mut sitemap = Sitemap::new();
        sitemap.add(SitemapEntry::new("https://example.com/"));
        let xml = render_sitemap_xml(&sitemap);
        assert!(xml.contains("<loc>https://example.com/</loc>"));
    }

    #[test]
    fn test_render_sitemap_xml_with_lastmod() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let mut sitemap = Sitemap::new();
        sitemap.add(SitemapEntry::new("https://example.com/").lastmod(dt));
        let xml = render_sitemap_xml(&sitemap);
        assert!(xml.contains("<lastmod>2024-01-15T12:00:00+00:00</lastmod>"));
    }

    #[test]
    fn test_render_sitemap_xml_with_changefreq() {
        let mut sitemap = Sitemap::new();
        sitemap.add(SitemapEntry::new("https://example.com/").changefreq(ChangeFreq::Daily));
        let xml = render_sitemap_xml(&sitemap);
        assert!(xml.contains("<changefreq>daily</changefreq>"));
    }

    #[test]
    fn test_render_sitemap_xml_with_priority() {
        let mut sitemap = Sitemap::new();
        sitemap.add(SitemapEntry::new("https://example.com/").priority(0.8));
        let xml = render_sitemap_xml(&sitemap);
        assert!(xml.contains("<priority>0.8</priority>"));
    }

    #[test]
    fn test_render_sitemap_xml_full_entry() {
        let dt = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        let mut sitemap = Sitemap::new();
        sitemap.add(
            SitemapEntry::new("https://example.com/blog/")
                .lastmod(dt)
                .changefreq(ChangeFreq::Weekly)
                .priority(0.7),
        );
        let xml = render_sitemap_xml(&sitemap);
        assert!(xml.contains("<loc>https://example.com/blog/</loc>"));
        assert!(xml.contains("<lastmod>"));
        assert!(xml.contains("<changefreq>weekly</changefreq>"));
        assert!(xml.contains("<priority>0.7</priority>"));
    }

    #[test]
    fn test_render_sitemap_xml_multiple_entries() {
        let mut sitemap = Sitemap::new();
        sitemap.add(SitemapEntry::new("https://example.com/page1"));
        sitemap.add(SitemapEntry::new("https://example.com/page2"));
        sitemap.add(SitemapEntry::new("https://example.com/page3"));
        let xml = render_sitemap_xml(&sitemap);
        assert!(xml.contains("page1"));
        assert!(xml.contains("page2"));
        assert!(xml.contains("page3"));
        assert_eq!(xml.matches("<url>").count(), 3);
    }

    #[test]
    fn test_render_sitemap_xml_escapes_xml() {
        let mut sitemap = Sitemap::new();
        sitemap.add(SitemapEntry::new("https://example.com/?a=1&b=2"));
        let xml = render_sitemap_xml(&sitemap);
        assert!(xml.contains("&amp;"));
        assert!(!xml.contains("?a=1&b"));
    }

    #[test]
    fn test_render_sitemap_index_xml() {
        let urls = vec![
            "https://example.com/sitemap1.xml",
            "https://example.com/sitemap2.xml",
        ];
        let xml = render_sitemap_index_xml(&urls);
        assert!(xml.contains("<sitemapindex"));
        assert!(xml.contains("sitemap1.xml"));
        assert!(xml.contains("sitemap2.xml"));
        assert_eq!(xml.matches("<sitemap>").count(), 2);
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("a&b"), "a&amp;b");
        assert_eq!(escape_xml("<tag>"), "&lt;tag&gt;");
        assert_eq!(escape_xml("\"hello\""), "&quot;hello&quot;");
        assert_eq!(escape_xml("it's"), "it&apos;s");
    }

    #[test]
    fn test_sitemap_entry_serialization() {
        let entry = SitemapEntry::new("https://example.com/")
            .changefreq(ChangeFreq::Daily)
            .priority(0.5);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"location\":\"https://example.com/\""));
    }
}
