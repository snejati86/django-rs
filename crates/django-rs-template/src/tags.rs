//! Built-in template tags.
//!
//! This module documents all built-in Django template tags supported by the
//! engine. Tags are parsed directly by the [`crate::parser`] module and
//! rendered as [`crate::parser::Node`] variants.
//!
//! ## Supported Tags
//!
//! ### Control Flow
//! - `{% if %}` / `{% elif %}` / `{% else %}` / `{% endif %}` — conditional rendering
//! - `{% for %}` / `{% empty %}` / `{% endfor %}` — iteration with `forloop` context
//! - `{% ifchanged %}` / `{% endifchanged %}` — render only when value changes
//! - `{% with %}` / `{% endwith %}` — create scoped variable assignments
//!
//! ### Template Composition
//! - `{% extends "parent.html" %}` — inherit from a parent template
//! - `{% block name %}` / `{% endblock %}` — define overridable blocks
//! - `{% include "partial.html" %}` — include another template
//!
//! ### Output
//! - `{% csrf_token %}` — output a CSRF hidden input field
//! - `{% url "name" %}` — reverse a URL by name (stub)
//! - `{% static "path" %}` — output a static file URL
//! - `{% spaceless %}` / `{% endspaceless %}` — remove whitespace between HTML tags
//!
//! ### Utility
//! - `{% comment %}` / `{% endcomment %}` — suppress output
//! - `{% verbatim %}` / `{% endverbatim %}` — output raw template syntax
//! - `{% cycle %}` — cycle through values in a loop
//! - `{% firstof %}` — output the first truthy value
//! - `{% now "format" %}` — output the current date/time
//! - `{% lorem %}` — generate lorem ipsum text
//! - `{% debug %}` — output debug context information
//! - `{% load %}` — load a template tag library (no-op)
//! - `{% autoescape on|off %}` / `{% endautoescape %}` — toggle auto-escaping
//!
//! ### Internationalization
//! - `{% trans "text" %}` — translate a string using i18n
//! - `{% blocktrans %}...{% endblocktrans %}` — translate a block of text
//!
//! ### Logic (deprecated)
//! - `{% ifequal %}` / `{% endifequal %}` — compare two values for equality
//!
//! ## The `forloop` Context Variable
//!
//! Inside a `{% for %}` loop, the `forloop` variable is automatically set:
//!
//! | Variable | Description |
//! |---|---|
//! | `forloop.counter` | 1-indexed iteration count |
//! | `forloop.counter0` | 0-indexed iteration count |
//! | `forloop.revcounter` | Reverse count from length |
//! | `forloop.revcounter0` | Reverse count from length-1 |
//! | `forloop.first` | `True` on first iteration |
//! | `forloop.last` | `True` on last iteration |
//! | `forloop.parentloop` | The parent `forloop` in nested loops |

/// Returns a list of all supported built-in tag names.
pub fn builtin_tag_names() -> Vec<&'static str> {
    vec![
        "if",
        "elif",
        "else",
        "endif",
        "for",
        "empty",
        "endfor",
        "with",
        "endwith",
        "extends",
        "block",
        "endblock",
        "include",
        "csrf_token",
        "url",
        "static",
        "spaceless",
        "endspaceless",
        "comment",
        "endcomment",
        "verbatim",
        "endverbatim",
        "cycle",
        "firstof",
        "now",
        "lorem",
        "debug",
        "load",
        "ifequal",
        "endifequal",
        "ifchanged",
        "endifchanged",
        "autoescape",
        "endautoescape",
        "trans",
        "blocktrans",
        "endblocktrans",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_tag_names() {
        let names = builtin_tag_names();
        assert!(names.contains(&"if"));
        assert!(names.contains(&"for"));
        assert!(names.contains(&"extends"));
        assert!(names.contains(&"block"));
        assert!(names.contains(&"include"));
        assert!(names.contains(&"csrf_token"));
        assert!(names.contains(&"with"));
        assert!(names.contains(&"comment"));
        assert!(names.contains(&"verbatim"));
        assert!(names.contains(&"cycle"));
        assert!(names.contains(&"firstof"));
        assert!(names.contains(&"now"));
        assert!(names.contains(&"lorem"));
        assert!(names.contains(&"debug"));
        assert!(names.contains(&"load"));
        assert!(names.contains(&"autoescape"));
        assert!(names.contains(&"trans"));
        assert!(names.contains(&"blocktrans"));
        assert!(names.contains(&"endblocktrans"));
    }

    #[test]
    fn test_tag_count() {
        let names = builtin_tag_names();
        // We should have a good number of tags (30 base + 3 i18n)
        assert!(names.len() >= 33);
    }
}
