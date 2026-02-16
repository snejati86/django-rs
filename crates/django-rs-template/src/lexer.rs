//! Template lexer (tokenizer).
//!
//! Converts raw template source text into a stream of [`Token`]s representing
//! text literals, variable references (`{{ }}`), block tags (`{% %}`), and
//! comments (`{# #}`).

use django_rs_core::error::DjangoError;

/// A token produced by the template lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A literal text segment.
    Text(String),
    /// A variable expression: `{{ expression }}`.
    Variable(String),
    /// A block tag: `{% tag arg1 arg2 %}`. Contains the tag name and its arguments.
    Block(String, Vec<String>),
    /// A comment: `{# comment text #}`.
    Comment(String),
}

/// Tokenizes a Django template source string into a sequence of [`Token`]s.
///
/// Handles `{{ variable }}`, `{% tag %}`, and `{# comment #}` syntax.
///
/// # Errors
///
/// Returns a `TemplateSyntaxError` if a tag or variable is opened but never closed.
pub fn tokenize(source: &str) -> Result<Vec<Token>, DjangoError> {
    let mut tokens = Vec::new();
    let mut remaining = source;

    while !remaining.is_empty() {
        // Find the next template tag opening
        let next_open = find_next_open(remaining);

        match next_open {
            None => {
                // Rest is plain text
                if !remaining.is_empty() {
                    tokens.push(Token::Text(remaining.to_string()));
                }
                break;
            }
            Some((pos, tag_type)) => {
                // Push any text before the tag
                if pos > 0 {
                    tokens.push(Token::Text(remaining[..pos].to_string()));
                }

                let after_open = &remaining[pos + 2..]; // skip the 2-char opener

                match tag_type {
                    TagType::Variable => {
                        // Look for }}
                        if let Some(end) = after_open.find("}}") {
                            let content = after_open[..end].trim().to_string();
                            tokens.push(Token::Variable(content));
                            remaining = &after_open[end + 2..];
                        } else {
                            return Err(DjangoError::TemplateSyntaxError(
                                "Unclosed variable tag: expected '}}' ".to_string(),
                            ));
                        }
                    }
                    TagType::Block => {
                        // Look for %}
                        if let Some(end) = after_open.find("%}") {
                            let content = after_open[..end].trim();
                            let token = parse_block_content(content);
                            tokens.push(token);
                            remaining = &after_open[end + 2..];
                        } else {
                            return Err(DjangoError::TemplateSyntaxError(
                                "Unclosed block tag: expected '%}'".to_string(),
                            ));
                        }
                    }
                    TagType::Comment => {
                        // Look for #}
                        if let Some(end) = after_open.find("#}") {
                            let content = after_open[..end].trim().to_string();
                            tokens.push(Token::Comment(content));
                            remaining = &after_open[end + 2..];
                        } else {
                            return Err(DjangoError::TemplateSyntaxError(
                                "Unclosed comment tag: expected '#}'".to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(tokens)
}

#[derive(Debug, Clone, Copy)]
enum TagType {
    Variable, // {{
    Block,    // {%
    Comment,  // {#
}

/// Finds the next template tag opening in the source.
fn find_next_open(s: &str) -> Option<(usize, TagType)> {
    let mut best: Option<(usize, TagType)> = None;

    for (tag_str, tag_type) in [
        ("{{", TagType::Variable),
        ("{%", TagType::Block),
        ("{#", TagType::Comment),
    ] {
        if let Some(pos) = s.find(tag_str) {
            match best {
                None => best = Some((pos, tag_type)),
                Some((best_pos, _)) if pos < best_pos => best = Some((pos, tag_type)),
                _ => {}
            }
        }
    }

    best
}

/// Parses the content inside `{% ... %}` into a `Token::Block`.
///
/// Handles quoted strings as single arguments.
fn parse_block_content(content: &str) -> Token {
    let parts = split_block_args(content);
    if parts.is_empty() {
        Token::Block(String::new(), Vec::new())
    } else {
        Token::Block(parts[0].clone(), parts[1..].to_vec())
    }
}

/// Splits block tag content into parts, respecting quoted strings.
fn split_block_args(content: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let chars = content.chars();

    for ch in chars {
        match ch {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                current.push(ch);
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                current.push(ch);
            }
            ' ' | '\t' | '\n' | '\r' if !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let tokens = tokenize("Hello world").unwrap();
        assert_eq!(tokens, vec![Token::Text("Hello world".to_string())]);
    }

    #[test]
    fn test_variable_tag() {
        let tokens = tokenize("{{ name }}").unwrap();
        assert_eq!(tokens, vec![Token::Variable("name".to_string())]);
    }

    #[test]
    fn test_variable_with_filter() {
        let tokens = tokenize("{{ name|lower }}").unwrap();
        assert_eq!(tokens, vec![Token::Variable("name|lower".to_string())]);
    }

    #[test]
    fn test_block_tag() {
        let tokens = tokenize("{% if condition %}").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Block(
                "if".to_string(),
                vec!["condition".to_string()]
            )]
        );
    }

    #[test]
    fn test_comment_tag() {
        let tokens = tokenize("{# this is a comment #}").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Comment("this is a comment".to_string())]
        );
    }

    #[test]
    fn test_mixed_content() {
        let tokens = tokenize("Hello {{ name }}! {% if show %}visible{% endif %}").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Text("Hello ".to_string()),
                Token::Variable("name".to_string()),
                Token::Text("! ".to_string()),
                Token::Block("if".to_string(), vec!["show".to_string()]),
                Token::Text("visible".to_string()),
                Token::Block("endif".to_string(), vec![]),
            ]
        );
    }

    #[test]
    fn test_unclosed_variable() {
        let result = tokenize("{{ name ");
        assert!(result.is_err());
    }

    #[test]
    fn test_unclosed_block() {
        let result = tokenize("{% if ");
        assert!(result.is_err());
    }

    #[test]
    fn test_unclosed_comment() {
        let result = tokenize("{# comment ");
        assert!(result.is_err());
    }

    #[test]
    fn test_block_with_quoted_string() {
        let tokens = tokenize(r#"{% extends "base.html" %}"#).unwrap();
        assert_eq!(
            tokens,
            vec![Token::Block(
                "extends".to_string(),
                vec!["\"base.html\"".to_string()]
            )]
        );
    }

    #[test]
    fn test_empty_template() {
        let tokens = tokenize("").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_only_text() {
        let tokens = tokenize("no tags here").unwrap();
        assert_eq!(tokens, vec![Token::Text("no tags here".to_string())]);
    }

    #[test]
    fn test_adjacent_tags() {
        let tokens = tokenize("{{ a }}{{ b }}").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Variable("a".to_string()),
                Token::Variable("b".to_string()),
            ]
        );
    }

    #[test]
    fn test_for_block() {
        let tokens = tokenize("{% for item in items %}").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Block(
                "for".to_string(),
                vec!["item".to_string(), "in".to_string(), "items".to_string()]
            )]
        );
    }

    #[test]
    fn test_block_with_multiple_args() {
        let tokens = tokenize("{% include \"header.html\" with title=\"Home\" %}").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Block(
                "include".to_string(),
                vec![
                    "\"header.html\"".to_string(),
                    "with".to_string(),
                    "title=\"Home\"".to_string(),
                ]
            )]
        );
    }

    #[test]
    fn test_variable_whitespace_trimming() {
        let tokens = tokenize("{{   name   }}").unwrap();
        assert_eq!(tokens, vec![Token::Variable("name".to_string())]);
    }

    #[test]
    fn test_comment_whitespace_trimming() {
        let tokens = tokenize("{#   comment   #}").unwrap();
        assert_eq!(tokens, vec![Token::Comment("comment".to_string())]);
    }

    #[test]
    fn test_multiple_blocks() {
        let tokens = tokenize("{% if x %}yes{% else %}no{% endif %}").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Block("if".to_string(), vec!["x".to_string()]),
                Token::Text("yes".to_string()),
                Token::Block("else".to_string(), vec![]),
                Token::Text("no".to_string()),
                Token::Block("endif".to_string(), vec![]),
            ]
        );
    }

    #[test]
    fn test_text_with_braces() {
        // A single brace should be treated as text
        let tokens = tokenize("a { b } c").unwrap();
        assert_eq!(tokens, vec![Token::Text("a { b } c".to_string())]);
    }
}
