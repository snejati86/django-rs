//! Template inheritance support.
//!
//! This module provides the public API for Django-style template inheritance:
//! `{% extends "base.html" %}`, `{% block name %}...{% endblock %}`, and
//! `{{ block.super }}`.
//!
//! The core inheritance logic is implemented in [`crate::engine::Engine`]. This
//! module re-exports the relevant node types and provides documentation.
//!
//! ## How Inheritance Works
//!
//! 1. A child template declares `{% extends "parent.html" %}` as its first tag.
//! 2. The child defines `{% block name %}...{% endblock %}` sections that override
//!    the parent's blocks of the same name.
//! 3. Blocks not overridden by the child use the parent's default content.
//! 4. `{{ block.super }}` inside a child block includes the parent's content for
//!    that block.
//! 5. Inheritance can be multi-level: child extends parent extends grandparent.
//!
//! ## Example
//!
//! ```text
//! {# base.html #}
//! <html>
//! <body>
//! {% block content %}Default content{% endblock %}
//! </body>
//! </html>
//!
//! {# page.html #}
//! {% extends "base.html" %}
//! {% block content %}
//! <h1>Page Title</h1>
//! <p>{{ block.super }}</p>
//! {% endblock %}
//! ```
//!
//! Rendering `page.html` produces:
//! ```text
//! <html>
//! <body>
//! <h1>Page Title</h1>
//! <p>Default content</p>
//! </body>
//! </html>
//! ```

/// Checks whether a template source uses `{% extends %}`.
///
/// Returns the parent template name if the template extends another template,
/// or `None` if it is a standalone template.
pub fn find_extends(source: &str) -> Option<String> {
    let trimmed = source.trim_start();
    if trimmed.starts_with("{%") {
        if let Some(end) = trimmed.find("%}") {
            let tag_content = trimmed[2..end].trim();
            let parts: Vec<&str> = tag_content.split_whitespace().collect();
            if parts.first() == Some(&"extends") {
                if let Some(name) = parts.get(1) {
                    return Some(crate::parser::strip_quotes(name));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_extends_present() {
        let source = r#"{% extends "base.html" %}
{% block content %}Hello{% endblock %}"#;
        assert_eq!(find_extends(source), Some("base.html".to_string()));
    }

    #[test]
    fn test_find_extends_absent() {
        let source = "<html><body>Hello</body></html>";
        assert_eq!(find_extends(source), None);
    }

    #[test]
    fn test_find_extends_single_quotes() {
        let source = "{% extends 'base.html' %}";
        assert_eq!(find_extends(source), Some("base.html".to_string()));
    }

    #[test]
    fn test_find_extends_with_whitespace() {
        let source = "  {% extends \"base.html\" %}";
        assert_eq!(find_extends(source), Some("base.html".to_string()));
    }

    #[test]
    fn test_find_extends_not_first_tag() {
        // extends must be the first tag in Django
        let source = "Some text {% extends \"base.html\" %}";
        assert_eq!(find_extends(source), None);
    }
}
