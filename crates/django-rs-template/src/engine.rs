//! Template engine — loading, parsing, and rendering templates.
//!
//! The [`Engine`] struct is the central entry point for the template system.
//! It manages template loaders, caches parsed templates, and renders templates
//! with a given context.

use std::collections::HashMap;
use std::path::PathBuf;

use django_rs_core::error::DjangoError;

use crate::context::Context;
use crate::lexer;
use crate::loaders::{FileSystemLoader, StringLoader, TemplateLoader};
use crate::parser::{self, Node, Template};

/// A trait for rendering templates, used to break circular dependencies
/// between the parser/renderer and the engine.
pub trait TemplateRenderer: Send + Sync {
    /// Renders a named template with the given context.
    fn render_template(&self, name: &str, context: &mut Context) -> Result<String, DjangoError>;
}

/// The template engine. Manages loaders, caches, and rendering.
///
/// # Examples
///
/// ```
/// use django_rs_template::engine::Engine;
/// use django_rs_template::context::{Context, ContextValue};
///
/// let mut engine = Engine::new();
/// engine.add_string_template("hello.html", "Hello {{ name }}!");
///
/// let mut ctx = Context::new();
/// ctx.set("name", ContextValue::from("World"));
///
/// let result = engine.render_to_string("hello.html", &mut ctx).unwrap();
/// assert_eq!(result, "Hello World!");
/// ```
pub struct Engine {
    /// Template search directories.
    dirs: Vec<PathBuf>,
    /// Registered template loaders.
    loaders: Vec<Box<dyn TemplateLoader>>,
    /// Whether auto-escaping is enabled by default.
    auto_escape: bool,
    /// Whether debug mode is enabled.
    debug: bool,
    /// An in-memory string loader for programmatically added templates.
    string_loader: StringLoader,
}

impl Engine {
    /// Creates a new engine with default settings.
    pub fn new() -> Self {
        Self {
            dirs: Vec::new(),
            loaders: Vec::new(),
            auto_escape: true,
            debug: false,
            string_loader: StringLoader::new(),
        }
    }

    /// Creates an engine from the given settings.
    pub fn from_settings(settings: &django_rs_core::settings::TemplateSettings) -> Self {
        let mut engine = Self::new();
        engine.dirs = settings.dirs.clone();
        engine.loaders.push(Box::new(FileSystemLoader::new(settings.dirs.clone())));

        if let Some(auto_escape) = settings.options.get("auto_escape") {
            engine.auto_escape = auto_escape.as_bool().unwrap_or(true);
        }

        engine
    }

    /// Sets the template search directories.
    pub fn set_dirs(&mut self, dirs: Vec<PathBuf>) {
        self.dirs = dirs.clone();
        // Insert filesystem loader at the beginning
        self.loaders.insert(0, Box::new(FileSystemLoader::new(dirs)));
    }

    /// Adds a template loader.
    pub fn add_loader(&mut self, loader: Box<dyn TemplateLoader>) {
        self.loaders.push(loader);
    }

    /// Sets whether auto-escaping is enabled.
    pub fn set_auto_escape(&mut self, enabled: bool) {
        self.auto_escape = enabled;
    }

    /// Sets whether debug mode is enabled.
    pub fn set_debug(&mut self, enabled: bool) {
        self.debug = enabled;
    }

    /// Adds an in-memory template.
    pub fn add_string_template(&self, name: &str, source: &str) {
        self.string_loader.add(name, source);
    }

    /// Loads the source of a template by name.
    fn load_source(&self, name: &str) -> Result<String, DjangoError> {
        // Check string loader first
        if let Ok(source) = self.string_loader.load(name) {
            return Ok(source);
        }

        // Check registered loaders
        for loader in &self.loaders {
            if let Ok(source) = loader.load(name) {
                return Ok(source);
            }
        }

        Err(DjangoError::TemplateDoesNotExist(format!(
            "Template '{name}' could not be found"
        )))
    }

    /// Loads and parses a template by name.
    pub fn get_template(&self, name: &str) -> Result<Template, DjangoError> {
        let source = self.load_source(name)?;
        let tokens = lexer::tokenize(&source)?;
        parser::parse(name, &tokens)
    }

    /// Renders a template by name with the given context.
    pub fn render_to_string(
        &self,
        name: &str,
        context: &mut Context,
    ) -> Result<String, DjangoError> {
        context.set_auto_escape(self.auto_escape);
        let template = self.get_template(name)?;
        self.render_template_obj(&template, context)
    }

    /// Renders a parsed template with the given context.
    fn render_template_obj(
        &self,
        template: &Template,
        context: &mut Context,
    ) -> Result<String, DjangoError> {
        if let Some(ref parent_name) = template.parent {
            // Template inheritance
            self.render_with_inheritance(template, parent_name, context)
        } else {
            parser::render_nodes(&template.nodes, context, self)
        }
    }

    /// Renders a template with inheritance (extends).
    fn render_with_inheritance(
        &self,
        child: &Template,
        parent_name: &str,
        context: &mut Context,
    ) -> Result<String, DjangoError> {
        // Collect block definitions from the child
        let child_blocks = collect_blocks(&child.nodes);

        // Load and parse the parent
        let parent = self.get_template(parent_name)?;

        if let Some(ref grandparent_name) = parent.parent {
            // Multi-level inheritance: merge blocks and recurse
            let parent_blocks = collect_blocks(&parent.nodes);
            let merged = merge_blocks(parent_blocks, child_blocks);
            self.render_inherited_template(&parent, grandparent_name, &merged, context)
        } else {
            // Direct parent: render parent with child's block overrides
            self.render_parent_with_blocks(&parent.nodes, &child_blocks, context)
        }
    }

    /// Renders a template through multi-level inheritance.
    fn render_inherited_template(
        &self,
        _template: &Template,
        parent_name: &str,
        blocks: &HashMap<String, Vec<&Node>>,
        context: &mut Context,
    ) -> Result<String, DjangoError> {
        let parent = self.get_template(parent_name)?;

        if let Some(ref grandparent_name) = parent.parent {
            let parent_blocks = collect_blocks(&parent.nodes);
            let merged = merge_block_refs(parent_blocks, blocks);
            self.render_inherited_template(&parent, grandparent_name, &merged, context)
        } else {
            self.render_parent_with_blocks(&parent.nodes, blocks, context)
        }
    }

    /// Renders parent nodes, replacing block definitions with child overrides.
    fn render_parent_with_blocks(
        &self,
        parent_nodes: &[Node],
        child_blocks: &HashMap<String, Vec<&Node>>,
        context: &mut Context,
    ) -> Result<String, DjangoError> {
        let mut output = String::new();

        for node in parent_nodes {
            match node {
                Node::BlockDefNode { name, content } => {
                    if let Some(child_content) = child_blocks.get(name) {
                        // Check if child content uses block.super
                        let parent_rendered = parser::render_nodes(content, context, self)?;
                        context.push();
                        context.set(
                            "block",
                            crate::context::ContextValue::Dict({
                                let mut map = HashMap::new();
                                map.insert(
                                    "super".to_string(),
                                    crate::context::ContextValue::SafeString(parent_rendered),
                                );
                                map
                            }),
                        );
                        for child_node in child_content {
                            output.push_str(&render_single_node(child_node, context, self)?);
                        }
                        context.pop();
                    } else {
                        // No override — render parent's default content
                        output.push_str(&parser::render_nodes(content, context, self)?);
                    }
                }
                Node::ExtendsNode { .. } => {
                    // Skip extends nodes in rendering
                }
                _ => {
                    output.push_str(&render_single_node(node, context, self)?);
                }
            }
        }

        Ok(output)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateRenderer for Engine {
    fn render_template(&self, name: &str, context: &mut Context) -> Result<String, DjangoError> {
        let template = self.get_template(name)?;
        self.render_template_obj(&template, context)
    }
}

/// Renders a single node.
fn render_single_node(
    node: &Node,
    context: &mut Context,
    engine: &dyn TemplateRenderer,
) -> Result<String, DjangoError> {
    parser::render_nodes(std::slice::from_ref(node), context, engine)
}

/// Collects block definitions from a node list into a name->nodes map.
fn collect_blocks(nodes: &[Node]) -> HashMap<String, Vec<&Node>> {
    let mut blocks = HashMap::new();
    for node in nodes {
        if let Node::BlockDefNode { name, content } = node {
            blocks.insert(name.clone(), content.iter().collect());
        }
    }
    blocks
}

/// Merges child blocks over parent blocks (child wins).
fn merge_blocks<'a>(
    parent: HashMap<String, Vec<&'a Node>>,
    child: HashMap<String, Vec<&'a Node>>,
) -> HashMap<String, Vec<&'a Node>> {
    let mut merged = parent;
    for (name, nodes) in child {
        merged.insert(name, nodes);
    }
    merged
}

/// Merges block references (for multi-level inheritance).
fn merge_block_refs<'a>(
    parent: HashMap<String, Vec<&'a Node>>,
    child: &HashMap<String, Vec<&'a Node>>,
) -> HashMap<String, Vec<&'a Node>> {
    let mut merged = parent;
    for (name, nodes) in child {
        merged.insert(name.clone(), nodes.clone());
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ContextValue;

    #[test]
    fn test_engine_basic_render() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "Hello World!");

        let mut ctx = Context::new();
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_engine_variable_render() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "Hello {{ name }}!");

        let mut ctx = Context::new();
        ctx.set("name", ContextValue::from("Django"));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "Hello Django!");
    }

    #[test]
    fn test_engine_auto_escape() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "{{ content }}");

        let mut ctx = Context::new();
        ctx.set("content", ContextValue::from("<script>alert('xss')</script>"));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert!(result.contains("&lt;script&gt;"));
        assert!(!result.contains("<script>"));
    }

    #[test]
    fn test_engine_safe_filter() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "{{ content|safe }}");

        let mut ctx = Context::new();
        ctx.set("content", ContextValue::from("<b>bold</b>"));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "<b>bold</b>");
    }

    #[test]
    fn test_engine_if_tag() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "{% if show %}visible{% endif %}");

        let mut ctx = Context::new();
        ctx.set("show", ContextValue::Bool(true));
        assert_eq!(
            engine.render_to_string("test.html", &mut ctx).unwrap(),
            "visible"
        );

        ctx.set("show", ContextValue::Bool(false));
        assert_eq!(
            engine.render_to_string("test.html", &mut ctx).unwrap(),
            ""
        );
    }

    #[test]
    fn test_engine_if_else() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "{% if show %}yes{% else %}no{% endif %}");

        let mut ctx = Context::new();
        ctx.set("show", ContextValue::Bool(true));
        assert_eq!(
            engine.render_to_string("test.html", &mut ctx).unwrap(),
            "yes"
        );

        ctx.set("show", ContextValue::Bool(false));
        assert_eq!(
            engine.render_to_string("test.html", &mut ctx).unwrap(),
            "no"
        );
    }

    #[test]
    fn test_engine_if_elif() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% if x == 1 %}one{% elif x == 2 %}two{% else %}other{% endif %}",
        );

        let mut ctx = Context::new();
        ctx.set("x", ContextValue::Integer(1));
        assert_eq!(engine.render_to_string("test.html", &mut ctx).unwrap(), "one");

        ctx.set("x", ContextValue::Integer(2));
        assert_eq!(engine.render_to_string("test.html", &mut ctx).unwrap(), "two");

        ctx.set("x", ContextValue::Integer(3));
        assert_eq!(engine.render_to_string("test.html", &mut ctx).unwrap(), "other");
    }

    #[test]
    fn test_engine_for_tag() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "{% for item in items %}{{ item }} {% endfor %}");

        let mut ctx = Context::new();
        ctx.set(
            "items",
            ContextValue::List(vec![
                ContextValue::from("a"),
                ContextValue::from("b"),
                ContextValue::from("c"),
            ]),
        );
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "a b c ");
    }

    #[test]
    fn test_engine_for_empty() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% for item in items %}{{ item }}{% empty %}nothing{% endfor %}",
        );

        let mut ctx = Context::new();
        ctx.set("items", ContextValue::List(vec![]));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "nothing");
    }

    #[test]
    fn test_engine_for_loop_counter() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% for item in items %}{{ forloop.counter }}{% endfor %}",
        );

        let mut ctx = Context::new();
        ctx.set(
            "items",
            ContextValue::List(vec![
                ContextValue::from("a"),
                ContextValue::from("b"),
                ContextValue::from("c"),
            ]),
        );
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "123");
    }

    #[test]
    fn test_engine_for_loop_counter0() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% for item in items %}{{ forloop.counter0 }}{% endfor %}",
        );

        let mut ctx = Context::new();
        ctx.set(
            "items",
            ContextValue::List(vec![ContextValue::from("a"), ContextValue::from("b")]),
        );
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "01");
    }

    #[test]
    fn test_engine_for_loop_first_last() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% for item in items %}{% if forloop.first %}F{% endif %}{% if forloop.last %}L{% endif %}{% endfor %}",
        );

        let mut ctx = Context::new();
        ctx.set(
            "items",
            ContextValue::List(vec![
                ContextValue::from("a"),
                ContextValue::from("b"),
                ContextValue::from("c"),
            ]),
        );
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "FL");
    }

    #[test]
    fn test_engine_for_loop_revcounter() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% for item in items %}{{ forloop.revcounter }}{% endfor %}",
        );

        let mut ctx = Context::new();
        ctx.set(
            "items",
            ContextValue::List(vec![
                ContextValue::from("a"),
                ContextValue::from("b"),
                ContextValue::from("c"),
            ]),
        );
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "321");
    }

    #[test]
    fn test_engine_with_tag() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            r#"{% with greeting="Hello" %}{{ greeting }}{% endwith %}"#,
        );

        let mut ctx = Context::new();
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_engine_include() {
        let engine = Engine::new();
        engine.add_string_template("header.html", "HEADER");
        engine.add_string_template("test.html", r#"{% include "header.html" %}BODY"#);

        let mut ctx = Context::new();
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "HEADERBODY");
    }

    #[test]
    fn test_engine_include_with_context() {
        let engine = Engine::new();
        engine.add_string_template("greeting.html", "Hello {{ who }}!");
        engine.add_string_template(
            "test.html",
            r#"{% include "greeting.html" with who="World" %}"#,
        );

        let mut ctx = Context::new();
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_engine_include_only() {
        let engine = Engine::new();
        engine.add_string_template("partial.html", "{{ x }}{{ y }}");
        engine.add_string_template(
            "test.html",
            r#"{% include "partial.html" with x="A" only %}"#,
        );

        let mut ctx = Context::new();
        ctx.set("y", ContextValue::from("B"));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "A");
    }

    #[test]
    fn test_engine_template_inheritance() {
        let engine = Engine::new();
        engine.add_string_template(
            "base.html",
            "BASE{% block content %}default{% endblock %}END",
        );
        engine.add_string_template(
            "child.html",
            r#"{% extends "base.html" %}{% block content %}override{% endblock %}"#,
        );

        let mut ctx = Context::new();
        let result = engine.render_to_string("child.html", &mut ctx).unwrap();
        assert_eq!(result, "BASEoverrideEND");
    }

    #[test]
    fn test_engine_template_inheritance_default() {
        let engine = Engine::new();
        engine.add_string_template(
            "base.html",
            "BASE{% block content %}default{% endblock %}END",
        );
        engine.add_string_template(
            "child.html",
            r#"{% extends "base.html" %}"#,
        );

        let mut ctx = Context::new();
        let result = engine.render_to_string("child.html", &mut ctx).unwrap();
        assert_eq!(result, "BASEdefaultEND");
    }

    #[test]
    fn test_engine_block_super() {
        let engine = Engine::new();
        engine.add_string_template(
            "base.html",
            "{% block content %}parent{% endblock %}",
        );
        engine.add_string_template(
            "child.html",
            r#"{% extends "base.html" %}{% block content %}{{ block.super }}-child{% endblock %}"#,
        );

        let mut ctx = Context::new();
        let result = engine.render_to_string("child.html", &mut ctx).unwrap();
        assert_eq!(result, "parent-child");
    }

    #[test]
    fn test_engine_multi_level_inheritance() {
        let engine = Engine::new();
        engine.add_string_template(
            "grandparent.html",
            "GP{% block content %}gp-default{% endblock %}GP",
        );
        engine.add_string_template(
            "parent.html",
            r#"{% extends "grandparent.html" %}{% block content %}parent{% endblock %}"#,
        );
        engine.add_string_template(
            "child.html",
            r#"{% extends "parent.html" %}{% block content %}child{% endblock %}"#,
        );

        let mut ctx = Context::new();
        let result = engine.render_to_string("child.html", &mut ctx).unwrap();
        assert_eq!(result, "GPchildGP");
    }

    #[test]
    fn test_engine_comment_tag() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "before{% comment %}hidden{% endcomment %}after",
        );

        let mut ctx = Context::new();
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "beforeafter");
    }

    #[test]
    fn test_engine_verbatim_tag() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% verbatim %}{{ not_parsed }}{% endverbatim %}",
        );

        let mut ctx = Context::new();
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "{{ not_parsed }}");
    }

    #[test]
    fn test_engine_csrf_token() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "{% csrf_token %}");

        let mut ctx = Context::new();
        ctx.set("csrf_token", ContextValue::from("abc123"));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert!(result.contains("abc123"));
        assert!(result.contains("csrfmiddlewaretoken"));
    }

    #[test]
    fn test_engine_spaceless() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% spaceless %}<p>hello</p>  <p>world</p>{% endspaceless %}",
        );

        let mut ctx = Context::new();
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "<p>hello</p><p>world</p>");
    }

    #[test]
    fn test_engine_filter_chain() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "{{ name|upper|truncatechars:5 }}");

        let mut ctx = Context::new();
        ctx.set("name", ContextValue::from("hello world"));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "HE...");
    }

    #[test]
    fn test_engine_dot_notation() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "{{ user.name }}");

        let mut ctx = Context::new();
        let mut user = HashMap::new();
        user.insert("name".to_string(), ContextValue::from("Alice"));
        ctx.set("user", ContextValue::Dict(user));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "Alice");
    }

    #[test]
    fn test_engine_missing_template() {
        let engine = Engine::new();
        let mut ctx = Context::new();
        let result = engine.render_to_string("missing.html", &mut ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_engine_static_tag() {
        let engine = Engine::new();
        engine.add_string_template("test.html", r#"{% static "css/style.css" %}"#);

        let mut ctx = Context::new();
        ctx.set("STATIC_URL", ContextValue::from("/static/"));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "/static/css/style.css");
    }

    #[test]
    fn test_engine_firstof() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "{% firstof a b c %}");

        let mut ctx = Context::new();
        ctx.set("a", ContextValue::None);
        ctx.set("b", ContextValue::from("second"));
        ctx.set("c", ContextValue::from("third"));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "second");
    }

    #[test]
    fn test_engine_load_tag_noop() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "{% load static %}OK");

        let mut ctx = Context::new();
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "OK");
    }

    #[test]
    fn test_engine_ifequal() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            r#"{% ifequal x "hello" %}match{% else %}no{% endifequal %}"#,
        );

        let mut ctx = Context::new();
        ctx.set("x", ContextValue::from("hello"));
        assert_eq!(engine.render_to_string("test.html", &mut ctx).unwrap(), "match");

        ctx.set("x", ContextValue::from("bye"));
        assert_eq!(engine.render_to_string("test.html", &mut ctx).unwrap(), "no");
    }

    #[test]
    fn test_engine_autoescape_off() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% autoescape off %}{{ html }}{% endautoescape %}",
        );

        let mut ctx = Context::new();
        ctx.set("html", ContextValue::from("<b>bold</b>"));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "<b>bold</b>");
    }

    #[test]
    fn test_engine_nested_for_parentloop() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% for outer in outers %}{% for inner in inners %}{{ forloop.parentloop.counter }}{% endfor %}{% endfor %}",
        );

        let mut ctx = Context::new();
        ctx.set(
            "outers",
            ContextValue::List(vec![ContextValue::from("a"), ContextValue::from("b")]),
        );
        ctx.set(
            "inners",
            ContextValue::List(vec![ContextValue::from("x"), ContextValue::from("y")]),
        );
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "1122");
    }

    #[test]
    fn test_engine_auto_escape_disabled() {
        let mut engine = Engine::new();
        engine.set_auto_escape(false);
        engine.add_string_template("test.html", "{{ content }}");

        let mut ctx = Context::new();
        ctx.set("content", ContextValue::from("<b>bold</b>"));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "<b>bold</b>");
    }

    #[test]
    fn test_engine_comment_inline() {
        let engine = Engine::new();
        engine.add_string_template("test.html", "before{# comment #}after");

        let mut ctx = Context::new();
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "beforeafter");
    }

    #[test]
    fn test_engine_for_with_dict() {
        let engine = Engine::new();
        engine.add_string_template(
            "test.html",
            "{% for key in data %}{{ key }}{% endfor %}",
        );

        let mut ctx = Context::new();
        let mut data = HashMap::new();
        data.insert("a".to_string(), ContextValue::Integer(1));
        ctx.set("data", ContextValue::Dict(data));
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "a");
    }

    #[test]
    fn test_engine_default_filter() {
        let engine = Engine::new();
        engine.add_string_template("test.html", r#"{{ val|default:"fallback" }}"#);

        let mut ctx = Context::new();
        let result = engine.render_to_string("test.html", &mut ctx).unwrap();
        assert_eq!(result, "fallback");
    }
}
