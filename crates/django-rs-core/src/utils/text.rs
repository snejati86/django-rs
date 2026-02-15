//! String utility functions.
//!
//! These functions mirror common Django text utilities like `slugify`,
//! `Truncator.chars`, `Truncator.words`, `capfirst`, and `strip_tags`.

use regex::Regex;
use std::sync::OnceLock;

/// Converts a string to a URL-friendly slug.
///
/// Converts to lowercase, removes non-alphanumeric characters (except hyphens
/// and spaces), replaces spaces with hyphens, and collapses consecutive hyphens.
///
/// # Examples
///
/// ```
/// use django_rs_core::utils::text::slugify;
///
/// assert_eq!(slugify("Hello World!"), "hello-world");
/// assert_eq!(slugify("  Spaced  Out  "), "spaced-out");
/// assert_eq!(slugify("already-slugged"), "already-slugged");
/// ```
pub fn slugify(s: &str) -> String {
    static NON_ALNUM: OnceLock<Regex> = OnceLock::new();
    static MULTI_HYPHEN: OnceLock<Regex> = OnceLock::new();

    let non_alnum = NON_ALNUM.get_or_init(|| Regex::new(r"[^\w\s-]").unwrap());
    let multi_hyphen = MULTI_HYPHEN.get_or_init(|| Regex::new(r"[-\s]+").unwrap());

    let s = s.to_lowercase();
    let s = non_alnum.replace_all(&s, "");
    let s = multi_hyphen.replace_all(&s, "-");
    let s = s.trim_matches('-');
    s.to_string()
}

/// Truncates a string to at most `n` characters.
///
/// If the string is longer than `n`, it is truncated and "..." is appended.
/// The total length including the ellipsis will be `n` (if `n >= 3`).
///
/// # Examples
///
/// ```
/// use django_rs_core::utils::text::truncate_chars;
///
/// assert_eq!(truncate_chars("Hello, World!", 5), "He...");
/// assert_eq!(truncate_chars("Hi", 10), "Hi");
/// ```
pub fn truncate_chars(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        return s.to_string();
    }
    if n <= 3 {
        return ".".repeat(n);
    }
    let mut result: String = chars[..n - 3].iter().collect();
    result.push_str("...");
    result
}

/// Truncates a string to at most `n` words.
///
/// If the string has more than `n` words, it is truncated and "..." is appended.
///
/// # Examples
///
/// ```
/// use django_rs_core::utils::text::truncate_words;
///
/// assert_eq!(truncate_words("one two three four", 2), "one two ...");
/// assert_eq!(truncate_words("short", 5), "short");
/// ```
pub fn truncate_words(s: &str, n: usize) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.len() <= n {
        return s.to_string();
    }
    if n == 0 {
        return "...".to_string();
    }
    let mut result = words[..n].join(" ");
    result.push_str(" ...");
    result
}

/// Capitalizes the first character of a string.
///
/// # Examples
///
/// ```
/// use django_rs_core::utils::text::capfirst;
///
/// assert_eq!(capfirst("hello"), "Hello");
/// assert_eq!(capfirst(""), "");
/// assert_eq!(capfirst("HELLO"), "HELLO");
/// ```
pub fn capfirst(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        let mut result = c.to_uppercase().to_string();
        result.extend(chars);
        result
    })
}

/// Strips HTML tags from a string, returning only the text content.
///
/// This uses a simple regex-based approach. For untrusted input that needs
/// security guarantees, use a proper HTML sanitizer.
///
/// # Examples
///
/// ```
/// use django_rs_core::utils::text::strip_tags;
///
/// assert_eq!(strip_tags("<p>Hello <b>world</b></p>"), "Hello world");
/// assert_eq!(strip_tags("no tags here"), "no tags here");
/// ```
pub fn strip_tags(s: &str) -> String {
    static TAG_RE: OnceLock<Regex> = OnceLock::new();
    let tag_re = TAG_RE.get_or_init(|| Regex::new(r"<[^>]*>").unwrap());
    tag_re.replace_all(s, "").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── slugify ──────────────────────────────────────────────────────

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
    }

    #[test]
    fn test_slugify_multiple_spaces() {
        assert_eq!(slugify("  spaced   out  "), "spaced-out");
    }

    #[test]
    fn test_slugify_already_slug() {
        assert_eq!(slugify("already-slugged"), "already-slugged");
    }

    #[test]
    fn test_slugify_empty() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn test_slugify_numbers() {
        assert_eq!(slugify("Item 42"), "item-42");
    }

    #[test]
    fn test_slugify_underscores() {
        assert_eq!(slugify("hello_world"), "hello_world");
    }

    // ── truncate_chars ───────────────────────────────────────────────

    #[test]
    fn test_truncate_chars_short() {
        assert_eq!(truncate_chars("Hi", 10), "Hi");
    }

    #[test]
    fn test_truncate_chars_exact() {
        assert_eq!(truncate_chars("Hello", 5), "Hello");
    }

    #[test]
    fn test_truncate_chars_longer() {
        assert_eq!(truncate_chars("Hello, World!", 5), "He...");
    }

    #[test]
    fn test_truncate_chars_tiny_n() {
        assert_eq!(truncate_chars("Hello", 2), "..");
    }

    #[test]
    fn test_truncate_chars_zero() {
        assert_eq!(truncate_chars("Hello", 0), "");
    }

    // ── truncate_words ───────────────────────────────────────────────

    #[test]
    fn test_truncate_words_short() {
        assert_eq!(truncate_words("one", 5), "one");
    }

    #[test]
    fn test_truncate_words_longer() {
        assert_eq!(truncate_words("one two three four", 2), "one two ...");
    }

    #[test]
    fn test_truncate_words_zero() {
        assert_eq!(truncate_words("one two", 0), "...");
    }

    // ── capfirst ─────────────────────────────────────────────────────

    #[test]
    fn test_capfirst_lower() {
        assert_eq!(capfirst("hello"), "Hello");
    }

    #[test]
    fn test_capfirst_upper() {
        assert_eq!(capfirst("HELLO"), "HELLO");
    }

    #[test]
    fn test_capfirst_empty() {
        assert_eq!(capfirst(""), "");
    }

    #[test]
    fn test_capfirst_single() {
        assert_eq!(capfirst("a"), "A");
    }

    // ── strip_tags ───────────────────────────────────────────────────

    #[test]
    fn test_strip_tags_basic() {
        assert_eq!(strip_tags("<p>Hello</p>"), "Hello");
    }

    #[test]
    fn test_strip_tags_nested() {
        assert_eq!(strip_tags("<p>Hello <b>world</b></p>"), "Hello world");
    }

    #[test]
    fn test_strip_tags_no_tags() {
        assert_eq!(strip_tags("no tags"), "no tags");
    }

    #[test]
    fn test_strip_tags_self_closing() {
        assert_eq!(strip_tags("line<br/>break"), "linebreak");
    }

    #[test]
    fn test_strip_tags_attributes() {
        assert_eq!(
            strip_tags(r#"<a href="http://example.com">link</a>"#),
            "link"
        );
    }
}
