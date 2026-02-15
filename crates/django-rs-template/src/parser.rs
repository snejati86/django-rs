//! Template parser.
//!
//! Converts a stream of lexer [`Token`]s into a tree of [`Node`]s that can be
//! rendered by the engine. Handles expression parsing, filter chains, and
//! delegation to tag parsers.

use django_rs_core::error::DjangoError;

use crate::context::{escape_html, Context, ContextValue};
use crate::lexer::Token;

/// A parsed filter call with a name and optional arguments.
#[derive(Debug, Clone)]
pub struct FilterCall {
    /// The filter name (e.g., `lower`, `truncatechars`).
    pub name: String,
    /// Arguments to the filter (e.g., the `30` in `truncatechars:30`).
    pub args: Vec<Expression>,
}

/// A parsed expression — a variable name or a literal value.
#[derive(Debug, Clone)]
pub enum Expression {
    /// A variable reference, possibly dot-separated (e.g., `user.name`).
    Variable(String),
    /// A string literal (e.g., `"hello"` or `'hello'`).
    StringLiteral(String),
    /// A numeric literal.
    NumericLiteral(f64),
}

impl Expression {
    /// Resolves this expression against a context, returning a `ContextValue`.
    pub fn resolve(&self, context: &Context) -> ContextValue {
        match self {
            Self::Variable(name) => context
                .get(name)
                .cloned()
                .unwrap_or(ContextValue::None),
            Self::StringLiteral(s) => ContextValue::String(s.clone()),
            Self::NumericLiteral(n) => {
                if n.fract() == 0.0 {
                    ContextValue::Integer(*n as i64)
                } else {
                    ContextValue::Float(*n)
                }
            }
        }
    }
}

/// Parses a variable expression string (the content inside `{{ }}`).
///
/// Supports filter chaining: `name|lower|truncatechars:30`
pub fn parse_variable_expression(expr: &str) -> Result<(Expression, Vec<FilterCall>), DjangoError> {
    let parts: Vec<&str> = split_on_pipes(expr);

    if parts.is_empty() {
        return Err(DjangoError::TemplateSyntaxError(
            "Empty variable expression".to_string(),
        ));
    }

    let base_expr = parse_expression(parts[0].trim())?;

    let mut filters = Vec::new();
    for part in &parts[1..] {
        let filter = parse_filter_call(part.trim())?;
        filters.push(filter);
    }

    Ok((base_expr, filters))
}

/// Splits on `|` but not inside strings.
fn split_on_pipes(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut in_single = false;
    let mut in_double = false;

    for (i, ch) in s.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '|' if !in_single && !in_double => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&s[start..]);
    result
}

/// Parses a single expression (variable reference or literal).
pub fn parse_expression(s: &str) -> Result<Expression, DjangoError> {
    let s = s.trim();

    if s.is_empty() {
        return Err(DjangoError::TemplateSyntaxError(
            "Empty expression".to_string(),
        ));
    }

    // String literal
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        let inner = &s[1..s.len() - 1];
        return Ok(Expression::StringLiteral(inner.to_string()));
    }

    // Numeric literal
    if let Ok(n) = s.parse::<f64>() {
        return Ok(Expression::NumericLiteral(n));
    }

    // Variable reference
    Ok(Expression::Variable(s.to_string()))
}

/// Parses a filter call like `truncatechars:30` or `default:"N/A"`.
fn parse_filter_call(s: &str) -> Result<FilterCall, DjangoError> {
    if let Some(colon_pos) = find_filter_colon(s) {
        let name = s[..colon_pos].trim().to_string();
        let arg_str = s[colon_pos + 1..].trim();
        let arg = parse_expression(arg_str)?;
        Ok(FilterCall {
            name,
            args: vec![arg],
        })
    } else {
        Ok(FilterCall {
            name: s.to_string(),
            args: Vec::new(),
        })
    }
}

/// Finds the first colon that is not inside quotes.
fn find_filter_colon(s: &str) -> Option<usize> {
    let mut in_single = false;
    let mut in_double = false;

    for (i, ch) in s.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ':' if !in_single && !in_double => return Some(i),
            _ => {}
        }
    }
    None
}

/// A node in the parsed template tree.
pub enum Node {
    /// A literal text segment.
    Text(String),
    /// A variable with optional filter chain.
    Variable {
        /// The base expression.
        expression: Expression,
        /// Filter calls to apply in order.
        filters: Vec<FilterCall>,
    },
    /// An `{% extends "parent.html" %}` directive.
    ExtendsNode {
        /// The parent template name.
        parent: String,
    },
    /// A `{% block name %}...{% endblock %}` section.
    BlockDefNode {
        /// The block name.
        name: String,
        /// The default content nodes.
        content: Vec<Node>,
    },
    /// An `{% if %}` conditional.
    IfNode {
        /// Conditions and their node lists: `(condition_expr, nodes)`.
        /// The last entry may have an empty condition string for `else`.
        branches: Vec<(IfCondition, Vec<Node>)>,
    },
    /// A `{% for var in iterable %}` loop.
    ForNode {
        /// The loop variable name(s).
        loop_vars: Vec<String>,
        /// The iterable expression.
        iterable: Expression,
        /// Body nodes.
        body: Vec<Node>,
        /// Nodes to render when the iterable is empty (`{% empty %}`).
        empty_body: Vec<Node>,
    },
    /// A `{% with %}` scope.
    WithNode {
        /// Variable assignments.
        assignments: Vec<(String, Expression)>,
        /// Body nodes.
        body: Vec<Node>,
    },
    /// An `{% include "template.html" %}` directive.
    IncludeNode {
        /// Template name to include.
        template_name: Expression,
        /// Extra context assignments (`with key=value`).
        extra_context: Vec<(String, Expression)>,
        /// If true, only the extra context is available (no parent context).
        only: bool,
    },
    /// `{% comment %}...{% endcomment %}` — suppressed output.
    CommentNode,
    /// `{% verbatim %}...{% endverbatim %}` — raw text without parsing.
    VerbatimNode {
        /// The raw text content.
        content: String,
    },
    /// `{% csrf_token %}` — outputs a CSRF hidden input.
    CsrfTokenNode,
    /// `{% spaceless %}...{% endspaceless %}` — removes whitespace between HTML tags.
    SpacelessNode {
        /// Body nodes.
        body: Vec<Node>,
    },
    /// `{% cycle %}` — cycles through values.
    CycleNode {
        /// Values to cycle through.
        values: Vec<Expression>,
        /// Optional variable name to assign the value to.
        as_var: Option<String>,
    },
    /// `{% firstof %}` — outputs the first truthy value.
    FirstOfNode {
        /// Values to check.
        values: Vec<Expression>,
    },
    /// `{% now "format" %}` — outputs the current date/time.
    NowNode {
        /// The format string.
        format: String,
    },
    /// `{% url "name" args %}` — reverses a URL (stub).
    UrlNode {
        /// The URL pattern name.
        name: Expression,
        /// Positional arguments.
        args: Vec<Expression>,
    },
    /// `{% static "path" %}` — outputs a static file URL.
    StaticNode {
        /// The static file path.
        path: Expression,
    },
    /// `{% ifequal a b %}...{% endifequal %}` — deprecated equality check.
    IfEqualNode {
        /// Left expression.
        left: Expression,
        /// Right expression.
        right: Expression,
        /// Body for equal case.
        body: Vec<Node>,
        /// Body for else case.
        else_body: Vec<Node>,
    },
    /// `{% ifchanged %}...{% endifchanged %}` — renders only when value changes.
    IfChangedNode {
        /// Expressions to check for changes.
        expressions: Vec<Expression>,
        /// Body nodes.
        body: Vec<Node>,
        /// Else body.
        else_body: Vec<Node>,
    },
    /// `{% load %}` — loads a template tag library (no-op in this implementation).
    LoadNode,
    /// `{% lorem %}` — generates lorem ipsum text.
    LoremNode {
        /// Number of paragraphs/words.
        count: usize,
        /// Method: "w" for words, "p" for paragraphs, "b" for plain-text paragraphs.
        method: String,
    },
    /// `{% debug %}` — outputs debug information about context.
    DebugNode,
    /// `{% autoescape on|off %}...{% endautoescape %}`.
    AutoescapeNode {
        /// Whether auto-escaping is enabled.
        enabled: bool,
        /// Body nodes.
        body: Vec<Node>,
    },
}

/// A condition in an `{% if %}` branch.
#[derive(Debug, Clone)]
pub enum IfCondition {
    /// A simple expression truthiness test.
    Expr(Expression),
    /// Negation of a condition.
    Not(Box<IfCondition>),
    /// Logical AND of two conditions.
    And(Box<IfCondition>, Box<IfCondition>),
    /// Logical OR of two conditions.
    Or(Box<IfCondition>, Box<IfCondition>),
    /// Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`.
    Compare(Expression, String, Expression),
    /// The `else` clause (always true).
    Else,
    /// `in` operator.
    In(Expression, Expression),
    /// `not in` operator.
    NotIn(Expression, Expression),
}

impl IfCondition {
    /// Evaluates this condition against a context.
    pub fn evaluate(&self, context: &Context) -> bool {
        match self {
            Self::Expr(expr) => expr.resolve(context).is_truthy(),
            Self::Not(inner) => !inner.evaluate(context),
            Self::And(left, right) => left.evaluate(context) && right.evaluate(context),
            Self::Or(left, right) => left.evaluate(context) || right.evaluate(context),
            Self::Compare(left, op, right) => {
                let l = left.resolve(context);
                let r = right.resolve(context);
                compare_values(&l, op, &r)
            }
            Self::Else => true,
            Self::In(needle, haystack) => {
                let n = needle.resolve(context);
                let h = haystack.resolve(context);
                value_in(&n, &h)
            }
            Self::NotIn(needle, haystack) => {
                let n = needle.resolve(context);
                let h = haystack.resolve(context);
                !value_in(&n, &h)
            }
        }
    }
}

fn value_in(needle: &ContextValue, haystack: &ContextValue) -> bool {
    match haystack {
        ContextValue::List(items) => items.iter().any(|item| item == needle),
        ContextValue::String(s) | ContextValue::SafeString(s) => {
            if let Some(n) = needle.as_str() {
                s.contains(n)
            } else {
                false
            }
        }
        ContextValue::Dict(map) => {
            if let Some(key) = needle.as_str() {
                map.contains_key(key)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn compare_values(left: &ContextValue, op: &str, right: &ContextValue) -> bool {
    match op {
        "==" => left == right,
        "!=" => left != right,
        "<" | ">" | "<=" | ">=" => {
            if let (Some(l), Some(r)) = (left.as_float(), right.as_float()) {
                match op {
                    "<" => l < r,
                    ">" => l > r,
                    "<=" => l <= r,
                    ">=" => l >= r,
                    _ => false,
                }
            } else {
                let l = left.to_display_string();
                let r = right.to_display_string();
                match op {
                    "<" => l < r,
                    ">" => l > r,
                    "<=" => l <= r,
                    ">=" => l >= r,
                    _ => false,
                }
            }
        }
        _ => false,
    }
}

/// A parsed template.
pub struct Template {
    /// The template name (usually the file path).
    pub name: String,
    /// The parsed node tree.
    pub nodes: Vec<Node>,
    /// The parent template name, if this template uses `{% extends %}`.
    pub parent: Option<String>,
}

/// Parses a list of tokens into a `Template`.
///
/// # Errors
///
/// Returns `TemplateSyntaxError` for invalid tag structures.
pub fn parse(name: &str, tokens: &[Token]) -> Result<Template, DjangoError> {
    let mut parser = ParserState::new(tokens);
    let nodes = parser.parse_nodes(&[])?;

    Ok(Template {
        name: name.to_string(),
        parent: parser.parent.clone(),
        nodes,
    })
}

struct ParserState<'a> {
    tokens: &'a [Token],
    pos: usize,
    parent: Option<String>,
}

impl<'a> ParserState<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            pos: 0,
            parent: None,
        }
    }

    fn parse_nodes(&mut self, end_tags: &[&str]) -> Result<Vec<Node>, DjangoError> {
        let mut nodes = Vec::new();

        while self.pos < self.tokens.len() {
            let token = &self.tokens[self.pos];

            match token {
                Token::Text(text) => {
                    nodes.push(Node::Text(text.clone()));
                    self.pos += 1;
                }
                Token::Comment(_) => {
                    self.pos += 1;
                }
                Token::Variable(expr) => {
                    let (expression, filters) = parse_variable_expression(expr)?;
                    nodes.push(Node::Variable {
                        expression,
                        filters,
                    });
                    self.pos += 1;
                }
                Token::Block(tag_name, args) => {
                    // Check if this is an end tag we're looking for
                    if end_tags.contains(&tag_name.as_str()) {
                        break;
                    }

                    let node = self.parse_block_tag(tag_name, args)?;
                    if let Some(n) = node {
                        nodes.push(n);
                    }
                }
            }
        }

        Ok(nodes)
    }

    fn parse_block_tag(
        &mut self,
        tag_name: &str,
        args: &[String],
    ) -> Result<Option<Node>, DjangoError> {
        match tag_name {
            "extends" => {
                let parent = strip_quotes(args.first().ok_or_else(|| {
                    DjangoError::TemplateSyntaxError(
                        "{% extends %} requires a template name".to_string(),
                    )
                })?);
                self.parent = Some(parent.clone());
                self.pos += 1;
                Ok(Some(Node::ExtendsNode { parent }))
            }
            "block" => {
                let name = args.first().ok_or_else(|| {
                    DjangoError::TemplateSyntaxError(
                        "{% block %} requires a name".to_string(),
                    )
                })?.clone();
                self.pos += 1;
                let content = self.parse_nodes(&["endblock"])?;
                self.pos += 1; // skip endblock
                Ok(Some(Node::BlockDefNode { name, content }))
            }
            "if" => self.parse_if(args),
            "for" => self.parse_for(args),
            "with" => self.parse_with(args),
            "include" => self.parse_include(args),
            "comment" => {
                self.pos += 1;
                let _ = self.parse_nodes(&["endcomment"])?;
                self.pos += 1; // skip endcomment
                Ok(Some(Node::CommentNode))
            }
            "verbatim" => self.parse_verbatim(),
            "csrf_token" => {
                self.pos += 1;
                Ok(Some(Node::CsrfTokenNode))
            }
            "spaceless" => {
                self.pos += 1;
                let body = self.parse_nodes(&["endspaceless"])?;
                self.pos += 1;
                Ok(Some(Node::SpacelessNode { body }))
            }
            "cycle" => self.parse_cycle(args),
            "firstof" => self.parse_firstof(args),
            "now" => {
                let format = strip_quotes(args.first().unwrap_or(&String::new()));
                self.pos += 1;
                Ok(Some(Node::NowNode { format }))
            }
            "url" => self.parse_url(args),
            "static" => {
                let path = if let Some(arg) = args.first() {
                    parse_expression(arg)?
                } else {
                    return Err(DjangoError::TemplateSyntaxError(
                        "{% static %} requires a path".to_string(),
                    ));
                };
                self.pos += 1;
                Ok(Some(Node::StaticNode { path }))
            }
            "ifequal" => self.parse_ifequal(args),
            "ifchanged" => self.parse_ifchanged(args),
            "load" => {
                self.pos += 1;
                Ok(Some(Node::LoadNode))
            }
            "lorem" => self.parse_lorem(args),
            "debug" => {
                self.pos += 1;
                Ok(Some(Node::DebugNode))
            }
            "autoescape" => self.parse_autoescape(args),
            _ => Err(DjangoError::TemplateSyntaxError(format!(
                "Unknown tag: '{tag_name}'"
            ))),
        }
    }

    fn parse_if(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        let condition = parse_if_condition(args)?;
        self.pos += 1;

        let mut branches = Vec::new();

        // Parse body until elif/else/endif
        let body = self.parse_nodes(&["elif", "else", "endif"])?;
        branches.push((condition, body));

        // Handle elif/else chains
        while self.pos < self.tokens.len() {
            if let Token::Block(tag, tag_args) = &self.tokens[self.pos] {
                match tag.as_str() {
                    "elif" => {
                        let condition = parse_if_condition(tag_args)?;
                        self.pos += 1;
                        let body = self.parse_nodes(&["elif", "else", "endif"])?;
                        branches.push((condition, body));
                    }
                    "else" => {
                        self.pos += 1;
                        let body = self.parse_nodes(&["endif"])?;
                        branches.push((IfCondition::Else, body));
                        // endif
                        self.pos += 1;
                        break;
                    }
                    "endif" => {
                        self.pos += 1;
                        break;
                    }
                    _ => break,
                }
            } else {
                break;
            }
        }

        Ok(Some(Node::IfNode { branches }))
    }

    fn parse_for(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        // {% for x in items %} or {% for x, y in items %}
        let in_pos = args.iter().position(|a| a == "in").ok_or_else(|| {
            DjangoError::TemplateSyntaxError("{% for %} requires 'in' keyword".to_string())
        })?;

        let loop_vars: Vec<String> = args[..in_pos]
            .iter()
            .map(|v| v.trim_end_matches(',').to_string())
            .collect();

        let iterable_parts: Vec<&str> = args[in_pos + 1..].iter().map(|s| s.as_str()).collect();
        let iterable_str = iterable_parts.join(" ");
        let iterable = parse_expression(&iterable_str)?;

        self.pos += 1;
        let body = self.parse_nodes(&["empty", "endfor"])?;

        let empty_body = if self.pos < self.tokens.len() {
            if let Token::Block(tag, _) = &self.tokens[self.pos] {
                if tag == "empty" {
                    self.pos += 1;
                    let empty = self.parse_nodes(&["endfor"])?;
                    self.pos += 1;
                    empty
                } else {
                    self.pos += 1;
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Ok(Some(Node::ForNode {
            loop_vars,
            iterable,
            body,
            empty_body,
        }))
    }

    fn parse_with(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        let mut assignments = Vec::new();

        for arg in args {
            if let Some(eq_pos) = arg.find('=') {
                let key = arg[..eq_pos].to_string();
                let val_str = &arg[eq_pos + 1..];
                let val = parse_expression(val_str)?;
                assignments.push((key, val));
            }
        }

        self.pos += 1;
        let body = self.parse_nodes(&["endwith"])?;
        self.pos += 1;

        Ok(Some(Node::WithNode { assignments, body }))
    }

    fn parse_include(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        let template_name = if let Some(arg) = args.first() {
            parse_expression(arg)?
        } else {
            return Err(DjangoError::TemplateSyntaxError(
                "{% include %} requires a template name".to_string(),
            ));
        };

        let mut extra_context = Vec::new();
        let mut only = false;
        let mut i = 1;

        while i < args.len() {
            if args[i] == "with" {
                i += 1;
                while i < args.len() && args[i] != "only" {
                    if let Some(eq_pos) = args[i].find('=') {
                        let key = args[i][..eq_pos].to_string();
                        let val_str = &args[i][eq_pos + 1..];
                        let val = parse_expression(val_str)?;
                        extra_context.push((key, val));
                    }
                    i += 1;
                }
            } else if args[i] == "only" {
                only = true;
                i += 1;
            } else {
                i += 1;
            }
        }

        self.pos += 1;
        Ok(Some(Node::IncludeNode {
            template_name,
            extra_context,
            only,
        }))
    }

    fn parse_verbatim(&mut self) -> Result<Option<Node>, DjangoError> {
        self.pos += 1;
        let mut content = String::new();

        while self.pos < self.tokens.len() {
            match &self.tokens[self.pos] {
                Token::Block(tag, _) if tag == "endverbatim" => {
                    self.pos += 1;
                    return Ok(Some(Node::VerbatimNode { content }));
                }
                Token::Text(text) => {
                    content.push_str(text);
                    self.pos += 1;
                }
                Token::Variable(expr) => {
                    content.push_str(&format!("{{{{ {expr} }}}}"));
                    self.pos += 1;
                }
                Token::Block(tag, args) => {
                    let args_str = if args.is_empty() {
                        tag.clone()
                    } else {
                        format!("{} {}", tag, args.join(" "))
                    };
                    content.push_str(&format!("{{% {args_str} %}}"));
                    self.pos += 1;
                }
                Token::Comment(c) => {
                    content.push_str(&format!("{{# {c} #}}"));
                    self.pos += 1;
                }
            }
        }

        Err(DjangoError::TemplateSyntaxError(
            "Unclosed {% verbatim %} tag".to_string(),
        ))
    }

    fn parse_cycle(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        let mut values = Vec::new();
        let mut as_var = None;

        let mut i = 0;
        while i < args.len() {
            if args[i] == "as" && i + 1 < args.len() {
                as_var = Some(args[i + 1].clone());
                break;
            }
            values.push(parse_expression(&args[i])?);
            i += 1;
        }

        self.pos += 1;
        Ok(Some(Node::CycleNode { values, as_var }))
    }

    fn parse_firstof(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        let values: Result<Vec<Expression>, _> =
            args.iter().map(|a| parse_expression(a)).collect();
        self.pos += 1;
        Ok(Some(Node::FirstOfNode {
            values: values?,
        }))
    }

    fn parse_url(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        let name = if let Some(arg) = args.first() {
            parse_expression(arg)?
        } else {
            return Err(DjangoError::TemplateSyntaxError(
                "{% url %} requires a URL name".to_string(),
            ));
        };

        let url_args: Result<Vec<Expression>, _> =
            args[1..].iter().map(|a| parse_expression(a)).collect();

        self.pos += 1;
        Ok(Some(Node::UrlNode {
            name,
            args: url_args?,
        }))
    }

    fn parse_ifequal(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        if args.len() < 2 {
            return Err(DjangoError::TemplateSyntaxError(
                "{% ifequal %} requires two arguments".to_string(),
            ));
        }

        let left = parse_expression(&args[0])?;
        let right = parse_expression(&args[1])?;

        self.pos += 1;
        let body = self.parse_nodes(&["else", "endifequal"])?;

        let else_body = if self.pos < self.tokens.len() {
            if let Token::Block(tag, _) = &self.tokens[self.pos] {
                if tag == "else" {
                    self.pos += 1;
                    let else_nodes = self.parse_nodes(&["endifequal"])?;
                    self.pos += 1;
                    else_nodes
                } else {
                    self.pos += 1;
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Ok(Some(Node::IfEqualNode {
            left,
            right,
            body,
            else_body,
        }))
    }

    fn parse_ifchanged(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        let expressions: Result<Vec<Expression>, _> =
            args.iter().map(|a| parse_expression(a)).collect();

        self.pos += 1;
        let body = self.parse_nodes(&["else", "endifchanged"])?;

        let else_body = if self.pos < self.tokens.len() {
            if let Token::Block(tag, _) = &self.tokens[self.pos] {
                if tag == "else" {
                    self.pos += 1;
                    let else_nodes = self.parse_nodes(&["endifchanged"])?;
                    self.pos += 1;
                    else_nodes
                } else {
                    self.pos += 1;
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Ok(Some(Node::IfChangedNode {
            expressions: expressions?,
            body,
            else_body,
        }))
    }

    fn parse_lorem(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        let count = args.first().and_then(|a| a.parse().ok()).unwrap_or(1);
        let method = args.get(1).cloned().unwrap_or_else(|| "p".to_string());

        self.pos += 1;
        Ok(Some(Node::LoremNode { count, method }))
    }

    fn parse_autoescape(&mut self, args: &[String]) -> Result<Option<Node>, DjangoError> {
        let enabled = args.first().map_or(true, |a| a != "off");
        self.pos += 1;
        let body = self.parse_nodes(&["endautoescape"])?;
        self.pos += 1;
        Ok(Some(Node::AutoescapeNode { enabled, body }))
    }
}

/// Parses an if-condition from block tag arguments.
fn parse_if_condition(args: &[String]) -> Result<IfCondition, DjangoError> {
    if args.is_empty() {
        return Err(DjangoError::TemplateSyntaxError(
            "{% if %} requires a condition".to_string(),
        ));
    }

    parse_or_condition(args, &mut 0)
}

fn parse_or_condition(args: &[String], pos: &mut usize) -> Result<IfCondition, DjangoError> {
    let left = parse_and_condition(args, pos)?;

    if *pos < args.len() && args[*pos] == "or" {
        *pos += 1;
        let right = parse_or_condition(args, pos)?;
        Ok(IfCondition::Or(Box::new(left), Box::new(right)))
    } else {
        Ok(left)
    }
}

fn parse_and_condition(args: &[String], pos: &mut usize) -> Result<IfCondition, DjangoError> {
    let left = parse_not_condition(args, pos)?;

    if *pos < args.len() && args[*pos] == "and" {
        *pos += 1;
        let right = parse_and_condition(args, pos)?;
        Ok(IfCondition::And(Box::new(left), Box::new(right)))
    } else {
        Ok(left)
    }
}

fn parse_not_condition(args: &[String], pos: &mut usize) -> Result<IfCondition, DjangoError> {
    if *pos < args.len() && args[*pos] == "not" {
        *pos += 1;
        // Check for "not in"
        if *pos < args.len() && args[*pos] == "in" {
            // Back up — this is handled in comparison
            *pos -= 1;
            return parse_comparison(args, pos);
        }
        let inner = parse_not_condition(args, pos)?;
        Ok(IfCondition::Not(Box::new(inner)))
    } else {
        parse_comparison(args, pos)
    }
}

fn parse_comparison(args: &[String], pos: &mut usize) -> Result<IfCondition, DjangoError> {
    if *pos >= args.len() {
        return Err(DjangoError::TemplateSyntaxError(
            "Unexpected end of if-condition".to_string(),
        ));
    }

    let left_expr = parse_expression(&args[*pos])?;
    *pos += 1;

    // Check for comparison operators
    if *pos < args.len() {
        let op = &args[*pos];
        match op.as_str() {
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                *pos += 1;
                if *pos >= args.len() {
                    return Err(DjangoError::TemplateSyntaxError(
                        "Expected value after comparison operator".to_string(),
                    ));
                }
                let right_expr = parse_expression(&args[*pos])?;
                *pos += 1;
                Ok(IfCondition::Compare(left_expr, op.clone(), right_expr))
            }
            "in" => {
                *pos += 1;
                if *pos >= args.len() {
                    return Err(DjangoError::TemplateSyntaxError(
                        "Expected value after 'in'".to_string(),
                    ));
                }
                let right_expr = parse_expression(&args[*pos])?;
                *pos += 1;
                Ok(IfCondition::In(left_expr, right_expr))
            }
            "not" => {
                // "not in"
                if *pos + 1 < args.len() && args[*pos + 1] == "in" {
                    *pos += 2; // skip "not" and "in"
                    if *pos >= args.len() {
                        return Err(DjangoError::TemplateSyntaxError(
                            "Expected value after 'not in'".to_string(),
                        ));
                    }
                    let right_expr = parse_expression(&args[*pos])?;
                    *pos += 1;
                    Ok(IfCondition::NotIn(left_expr, right_expr))
                } else {
                    Ok(IfCondition::Expr(left_expr))
                }
            }
            _ => Ok(IfCondition::Expr(left_expr)),
        }
    } else {
        Ok(IfCondition::Expr(left_expr))
    }
}

/// Strips surrounding quotes from a string.
pub fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Renders a node tree to a string.
pub fn render_nodes(
    nodes: &[Node],
    context: &mut Context,
    engine: &dyn crate::engine::TemplateRenderer,
) -> Result<String, DjangoError> {
    let mut output = String::new();

    for node in nodes {
        output.push_str(&render_node(node, context, engine)?);
    }

    Ok(output)
}

/// Renders a single node to a string.
#[allow(clippy::too_many_lines)]
fn render_node(
    node: &Node,
    context: &mut Context,
    engine: &dyn crate::engine::TemplateRenderer,
) -> Result<String, DjangoError> {
    match node {
        Node::Text(text) => Ok(text.clone()),
        Node::Variable {
            expression,
            filters,
        } => {
            let mut value = expression.resolve(context);

            // Apply filters
            let registry = crate::filters::default_registry();
            for filter in filters {
                let filter_args: Vec<ContextValue> =
                    filter.args.iter().map(|a| a.resolve(context)).collect();
                value = registry.apply(&filter.name, &value, &filter_args)?;
            }

            // Auto-escape if needed
            if context.auto_escape() && !value.is_safe() {
                let s = value.to_display_string();
                Ok(escape_html(&s))
            } else {
                Ok(value.to_display_string())
            }
        }
        Node::ExtendsNode { .. } => {
            // Extends is handled at the engine level, not here
            Ok(String::new())
        }
        Node::BlockDefNode { content, .. } => {
            // When rendering inline (no inheritance), just render content
            render_nodes(content, context, engine)
        }
        Node::IfNode { branches } => {
            for (condition, body) in branches {
                if condition.evaluate(context) {
                    return render_nodes(body, context, engine);
                }
            }
            Ok(String::new())
        }
        Node::ForNode {
            loop_vars,
            iterable,
            body,
            empty_body,
        } => render_for_node(loop_vars, iterable, body, empty_body, context, engine),
        Node::WithNode { assignments, body } => {
            context.push();
            for (key, expr) in assignments {
                let value = expr.resolve(context);
                context.set(key, value);
            }
            let result = render_nodes(body, context, engine)?;
            context.pop();
            Ok(result)
        }
        Node::IncludeNode {
            template_name,
            extra_context,
            only,
        } => {
            let name_val = template_name.resolve(context);
            let name = name_val.to_display_string();

            if *only {
                let mut new_ctx = Context::new();
                new_ctx.set_auto_escape(context.auto_escape());
                for (key, expr) in extra_context {
                    let value = expr.resolve(context);
                    new_ctx.set(key, value);
                }
                engine.render_template(&name, &mut new_ctx)
            } else {
                context.push();
                for (key, expr) in extra_context {
                    let value = expr.resolve(context);
                    context.set(key, value);
                }
                let result = engine.render_template(&name, context)?;
                context.pop();
                Ok(result)
            }
        }
        Node::CommentNode => Ok(String::new()),
        Node::VerbatimNode { content } => Ok(content.clone()),
        Node::CsrfTokenNode => {
            let token = context
                .get("csrf_token")
                .map(|v| v.to_display_string())
                .unwrap_or_default();
            Ok(format!(
                r#"<input type="hidden" name="csrfmiddlewaretoken" value="{token}">"#
            ))
        }
        Node::SpacelessNode { body } => {
            let content = render_nodes(body, context, engine)?;
            Ok(remove_whitespace_between_tags(&content))
        }
        Node::CycleNode { values, as_var } => {
            // Use a counter from context for cycling
            let counter_key = format!("__cycle_{}", values.len());
            let counter = context
                .get(&counter_key)
                .and_then(|v| v.as_integer())
                .unwrap_or(0) as usize;

            let idx = counter % values.len();
            let value = values[idx].resolve(context);
            let display = value.to_display_string();

            context.set(&counter_key, ContextValue::Integer((counter + 1) as i64));

            if let Some(var) = as_var {
                context.set(var, value);
                Ok(String::new())
            } else {
                Ok(display)
            }
        }
        Node::FirstOfNode { values } => {
            for expr in values {
                let val = expr.resolve(context);
                if val.is_truthy() {
                    let s = val.to_display_string();
                    return if context.auto_escape() && !val.is_safe() {
                        Ok(escape_html(&s))
                    } else {
                        Ok(s)
                    };
                }
            }
            Ok(String::new())
        }
        Node::NowNode { format } => {
            let now = chrono::Local::now();
            Ok(format_django_date(&now, format))
        }
        Node::UrlNode { name, .. } => {
            // Stub: just output the name for now
            let name_val = name.resolve(context);
            Ok(name_val.to_display_string())
        }
        Node::StaticNode { path } => {
            let path_val = path.resolve(context);
            let static_url = context
                .get("STATIC_URL")
                .map(|v| v.to_display_string())
                .unwrap_or_else(|| "/static/".to_string());
            Ok(format!("{}{}", static_url, path_val.to_display_string()))
        }
        Node::IfEqualNode {
            left,
            right,
            body,
            else_body,
        } => {
            let l = left.resolve(context);
            let r = right.resolve(context);
            if l == r {
                render_nodes(body, context, engine)
            } else {
                render_nodes(else_body, context, engine)
            }
        }
        Node::IfChangedNode {
            expressions,
            body,
            else_body,
        } => {
            let current_values: Vec<String> = if expressions.is_empty() {
                vec![render_nodes(body, context, engine)?]
            } else {
                expressions
                    .iter()
                    .map(|e| e.resolve(context).to_display_string())
                    .collect()
            };

            let key = "__ifchanged_last".to_string();
            let last_str = context.get(&key).map(|v| v.to_display_string());
            let current_str = current_values.join(",");

            if last_str.as_deref() != Some(&current_str) {
                context.set(&key, ContextValue::String(current_str));
                if expressions.is_empty() {
                    // We already rendered body above
                    Ok(current_values.into_iter().next().unwrap_or_default())
                } else {
                    render_nodes(body, context, engine)
                }
            } else {
                render_nodes(else_body, context, engine)
            }
        }
        Node::LoadNode => Ok(String::new()),
        Node::LoremNode { count, method } => Ok(generate_lorem(*count, method)),
        Node::DebugNode => {
            let flat = context.flatten();
            let mut output = String::from("Context:\n");
            for (k, v) in &flat {
                if !k.starts_with("__") {
                    output.push_str(&format!("  {k}: {}\n", v.to_display_string()));
                }
            }
            Ok(output)
        }
        Node::AutoescapeNode { enabled, body } => {
            let prev = context.auto_escape();
            context.set_auto_escape(*enabled);
            let result = render_nodes(body, context, engine)?;
            context.set_auto_escape(prev);
            Ok(result)
        }
    }
}

/// Renders a for-loop node.
fn render_for_node(
    loop_vars: &[String],
    iterable: &Expression,
    body: &[Node],
    empty_body: &[Node],
    context: &mut Context,
    engine: &dyn crate::engine::TemplateRenderer,
) -> Result<String, DjangoError> {
    let items = iterable.resolve(context);

    let list = match &items {
        ContextValue::List(list) => list.clone(),
        ContextValue::Dict(map) => {
            // Iterating over a dict yields its keys
            map.keys()
                .map(|k| ContextValue::String(k.clone()))
                .collect()
        }
        _ => Vec::new(),
    };

    if list.is_empty() {
        return render_nodes(empty_body, context, engine);
    }

    let total = list.len();
    let mut output = String::new();

    // Save parent loop if there is one
    let parent_loop = context.get("forloop").cloned();

    for (idx, item) in list.iter().enumerate() {
        context.push();

        // Set loop variable(s)
        if loop_vars.len() == 1 {
            context.set(&loop_vars[0], item.clone());
        } else if let ContextValue::List(inner) = item {
            for (j, var) in loop_vars.iter().enumerate() {
                context.set(
                    var,
                    inner.get(j).cloned().unwrap_or(ContextValue::None),
                );
            }
        } else if loop_vars.len() == 2 {
            if let ContextValue::Dict(_) = &items {
                // For dict unpacking: key, value
                if let ContextValue::String(key) = item {
                    context.set(&loop_vars[0], item.clone());
                    if let ContextValue::Dict(map) = &items {
                        context.set(
                            &loop_vars[1],
                            map.get(key).cloned().unwrap_or(ContextValue::None),
                        );
                    }
                }
            }
        }

        // Build forloop context
        let mut forloop = HashMap::new();
        forloop.insert("counter".to_string(), ContextValue::Integer((idx + 1) as i64));
        forloop.insert("counter0".to_string(), ContextValue::Integer(idx as i64));
        forloop.insert(
            "revcounter".to_string(),
            ContextValue::Integer((total - idx) as i64),
        );
        forloop.insert(
            "revcounter0".to_string(),
            ContextValue::Integer((total - idx - 1) as i64),
        );
        forloop.insert("first".to_string(), ContextValue::Bool(idx == 0));
        forloop.insert("last".to_string(), ContextValue::Bool(idx == total - 1));

        if let Some(ref parent) = parent_loop {
            forloop.insert("parentloop".to_string(), parent.clone());
        }

        context.set("forloop", ContextValue::Dict(forloop));

        output.push_str(&render_nodes(body, context, engine)?);

        context.pop();
    }

    Ok(output)
}

/// Removes whitespace between HTML tags.
fn remove_whitespace_between_tags(s: &str) -> String {
    let re = regex::Regex::new(r">\s+<").unwrap();
    re.replace_all(s, "><").to_string()
}

/// Formats a date/time using Django-style format characters.
#[allow(clippy::format_push_string)]
fn format_django_date(dt: &chrono::DateTime<chrono::Local>, format: &str) -> String {
    // Map common Django format characters to chrono format
    use std::fmt::Write;
    let mut result = String::new();
    let mut chars = format.chars();

    while let Some(ch) = chars.next() {
        match ch {
            'Y' => result.push_str(&dt.format("%Y").to_string()),
            'y' => result.push_str(&dt.format("%y").to_string()),
            'm' => result.push_str(&dt.format("%m").to_string()),
            'n' => result.push_str(&dt.format("%-m").to_string()),
            'd' => result.push_str(&dt.format("%d").to_string()),
            'j' => result.push_str(&dt.format("%-d").to_string()),
            'H' => result.push_str(&dt.format("%H").to_string()),
            'i' => result.push_str(&dt.format("%M").to_string()),
            's' => result.push_str(&dt.format("%S").to_string()),
            'F' => result.push_str(&dt.format("%B").to_string()),
            'M' => result.push_str(&dt.format("%b").to_string()),
            'N' => result.push_str(&dt.format("%b.").to_string()),
            'D' => result.push_str(&dt.format("%a").to_string()),
            'l' => result.push_str(&dt.format("%A").to_string()),
            'P' => {
                let hour = dt.format("%I").to_string().trim_start_matches('0').to_string();
                let minute = dt.format("%M").to_string();
                let ampm = dt.format("%P").to_string();
                if minute == "00" {
                    let _ = write!(result, "{hour} {ampm}");
                } else {
                    let _ = write!(result, "{hour}:{minute} {ampm}");
                }
            }
            'A' => result.push_str(&dt.format("%p").to_string()),
            'f' => {
                let hour = dt.format("%I").to_string().trim_start_matches('0').to_string();
                let minute = dt.format("%M").to_string();
                if minute == "00" {
                    result.push_str(&hour);
                } else {
                    let _ = write!(result, "{hour}:{minute}");
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            }
            _ => result.push(ch),
        }
    }

    result
}

/// Public wrapper for `format_django_date` used by the filters module.
pub fn format_django_date_pub(dt: &chrono::DateTime<chrono::Local>, format: &str) -> String {
    format_django_date(dt, format)
}

/// Generates lorem ipsum text.
fn generate_lorem(count: usize, method: &str) -> String {
    const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
        Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
        Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris \
        nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in \
        reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
        pariatur. Excepteur sint occaecat cupidatat non proident, sunt in \
        culpa qui officia deserunt mollit anim id est laborum.";

    match method {
        "w" => {
            let words: Vec<&str> = LOREM.split_whitespace().collect();
            let selected: Vec<&str> = words.iter().copied().cycle().take(count).collect();
            selected.join(" ")
        }
        "p" => {
            let mut paragraphs = Vec::new();
            for _ in 0..count {
                paragraphs.push(format!("<p>{LOREM}</p>"));
            }
            paragraphs.join("\n\n")
        }
        "b" => {
            let mut paragraphs = Vec::new();
            for _ in 0..count {
                paragraphs.push(LOREM.to_string());
            }
            paragraphs.join("\n\n")
        }
        _ => LOREM.to_string(),
    }
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    #[test]
    fn test_parse_expression_variable() {
        let expr = parse_expression("name").unwrap();
        assert!(matches!(expr, Expression::Variable(ref s) if s == "name"));
    }

    #[test]
    fn test_parse_expression_string_literal() {
        let expr = parse_expression("\"hello\"").unwrap();
        assert!(matches!(expr, Expression::StringLiteral(ref s) if s == "hello"));
    }

    #[test]
    fn test_parse_expression_numeric() {
        let expr = parse_expression("42").unwrap();
        assert!(matches!(expr, Expression::NumericLiteral(n) if (n - 42.0).abs() < f64::EPSILON));
    }

    #[test]
    fn test_parse_variable_expression_simple() {
        let (expr, filters) = parse_variable_expression("name").unwrap();
        assert!(matches!(expr, Expression::Variable(ref s) if s == "name"));
        assert!(filters.is_empty());
    }

    #[test]
    fn test_parse_variable_expression_with_filters() {
        let (expr, filters) = parse_variable_expression("name|lower|truncatechars:30").unwrap();
        assert!(matches!(expr, Expression::Variable(ref s) if s == "name"));
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0].name, "lower");
        assert_eq!(filters[1].name, "truncatechars");
        assert_eq!(filters[1].args.len(), 1);
    }

    #[test]
    fn test_parse_filter_with_string_arg() {
        let (_, filters) = parse_variable_expression("val|default:\"N/A\"").unwrap();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].name, "default");
    }

    #[test]
    fn test_parse_template_text_only() {
        let tokens = tokenize("Hello world").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::Text(s) if s == "Hello world"));
    }

    #[test]
    fn test_parse_template_variable() {
        let tokens = tokenize("{{ name }}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::Variable { .. }));
    }

    #[test]
    fn test_parse_if_block() {
        let tokens = tokenize("{% if show %}visible{% endif %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::IfNode { .. }));
    }

    #[test]
    fn test_parse_if_else_block() {
        let tokens = tokenize("{% if show %}yes{% else %}no{% endif %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        if let Node::IfNode { branches } = &template.nodes[0] {
            assert_eq!(branches.len(), 2);
        } else {
            panic!("Expected IfNode");
        }
    }

    #[test]
    fn test_parse_for_block() {
        let tokens = tokenize("{% for item in items %}{{ item }}{% endfor %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::ForNode { .. }));
    }

    #[test]
    fn test_parse_extends() {
        let tokens = tokenize(r#"{% extends "base.html" %}"#).unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.parent, Some("base.html".to_string()));
    }

    #[test]
    fn test_parse_block_def() {
        let tokens =
            tokenize("{% block content %}Hello{% endblock %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::BlockDefNode { name, .. } if name == "content"));
    }

    #[test]
    fn test_strip_quotes() {
        assert_eq!(strip_quotes("\"hello\""), "hello");
        assert_eq!(strip_quotes("'hello'"), "hello");
        assert_eq!(strip_quotes("hello"), "hello");
    }

    #[test]
    fn test_parse_with_block() {
        let tokens = tokenize("{% with name=\"World\" %}{{ name }}{% endwith %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::WithNode { .. }));
    }

    #[test]
    fn test_parse_include() {
        let tokens = tokenize(r#"{% include "header.html" %}"#).unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::IncludeNode { .. }));
    }

    #[test]
    fn test_parse_comment_block() {
        let tokens = tokenize("{% comment %}hidden{% endcomment %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::CommentNode));
    }

    #[test]
    fn test_parse_csrf_token() {
        let tokens = tokenize("{% csrf_token %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::CsrfTokenNode));
    }

    #[test]
    fn test_parse_verbatim() {
        let tokens = tokenize("{% verbatim %}{{ not_parsed }}{% endverbatim %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        if let Node::VerbatimNode { content } = &template.nodes[0] {
            assert_eq!(content, "{{ not_parsed }}");
        } else {
            panic!("Expected VerbatimNode");
        }
    }

    #[test]
    fn test_parse_spaceless() {
        let tokens = tokenize("{% spaceless %}<p> hi </p>{% endspaceless %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::SpacelessNode { .. }));
    }

    #[test]
    fn test_parse_load() {
        let tokens = tokenize("{% load static %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::LoadNode));
    }

    #[test]
    fn test_parse_debug() {
        let tokens = tokenize("{% debug %}").unwrap();
        let template = parse("test.html", &tokens).unwrap();
        assert_eq!(template.nodes.len(), 1);
        assert!(matches!(&template.nodes[0], Node::DebugNode));
    }

    #[test]
    fn test_remove_whitespace_between_tags() {
        assert_eq!(
            remove_whitespace_between_tags("<p>hello</p>  <p>world</p>"),
            "<p>hello</p><p>world</p>"
        );
    }

    #[test]
    fn test_if_condition_comparison() {
        let cond = parse_if_condition(&["x".into(), "==".into(), "1".into()]).unwrap();
        let mut ctx = Context::new();
        ctx.set("x", ContextValue::Integer(1));
        assert!(cond.evaluate(&ctx));
    }

    #[test]
    fn test_if_condition_and() {
        let cond =
            parse_if_condition(&["a".into(), "and".into(), "b".into()]).unwrap();
        let mut ctx = Context::new();
        ctx.set("a", ContextValue::Bool(true));
        ctx.set("b", ContextValue::Bool(true));
        assert!(cond.evaluate(&ctx));
    }

    #[test]
    fn test_if_condition_or() {
        let cond =
            parse_if_condition(&["a".into(), "or".into(), "b".into()]).unwrap();
        let mut ctx = Context::new();
        ctx.set("a", ContextValue::Bool(false));
        ctx.set("b", ContextValue::Bool(true));
        assert!(cond.evaluate(&ctx));
    }

    #[test]
    fn test_if_condition_not() {
        let cond =
            parse_if_condition(&["not".into(), "a".into()]).unwrap();
        let mut ctx = Context::new();
        ctx.set("a", ContextValue::Bool(false));
        assert!(cond.evaluate(&ctx));
    }

    #[test]
    fn test_if_condition_in() {
        let cond =
            parse_if_condition(&["x".into(), "in".into(), "items".into()]).unwrap();
        let mut ctx = Context::new();
        ctx.set("x", ContextValue::from("a"));
        ctx.set(
            "items",
            ContextValue::List(vec![ContextValue::from("a"), ContextValue::from("b")]),
        );
        assert!(cond.evaluate(&ctx));
    }

    #[test]
    fn test_if_condition_not_in() {
        let cond = parse_if_condition(&[
            "x".into(),
            "not".into(),
            "in".into(),
            "items".into(),
        ])
        .unwrap();
        let mut ctx = Context::new();
        ctx.set("x", ContextValue::from("c"));
        ctx.set(
            "items",
            ContextValue::List(vec![ContextValue::from("a"), ContextValue::from("b")]),
        );
        assert!(cond.evaluate(&ctx));
    }
}
