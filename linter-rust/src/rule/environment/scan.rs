use super::{
    EnvironmentAccess,
    config::{Assertion, Scope},
};
use crate::{Source, declaration::Index, imports::Imports};
use linter::{Evidence, Finding, Rule, Span};
use tree_sitter::Node;

pub(super) fn inspect(
    source: &Source,
    tests: &[bool],
    assertion: &Assertion,
    index: &Index<'_>,
    findings: &mut Vec<Finding>,
) {
    Scan {
        source,
        tests,
        assertion,
        index,
        findings,
    }
    .visit(source.syntax.root_node(), &mut Imports::default());
}
struct Scan<'a, 'b> {
    source: &'a Source,
    tests: &'b [bool],
    assertion: &'b Assertion,
    index: &'b Index<'a>,
    findings: &'b mut Vec<Finding>,
}
impl Scan<'_, '_> {
    fn text(&self, node: Node<'_>) -> &str {
        &self.source.text[node.byte_range()]
    }
    fn selected(&self, node: Node<'_>) -> bool {
        let selected = match self.assertion.scope {
            Scope::Production => !self.tests[node.start_byte()],
            Scope::Tests => self.tests[node.start_byte()],
            Scope::All => true,
        };
        selected
            && !self.assertion.allowed_modules.iter().any(|module| {
                self.index
                    .identity(self.source, node)
                    .module
                    .starts_with(module)
            })
    }
    fn visit(&mut self, node: Node<'_>, imports: &mut Imports) {
        match node.kind() {
            "line_comment" | "block_comment" | "string_literal" | "raw_string_literal"
            | "char_literal" | "macro_definition" => return,
            "mod_item" => {
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body, &mut Imports::default());
                }
                return;
            }
            "source_file" | "declaration_list" | "block" => {
                let mut scoped = imports.clone();
                scoped.items(node, &self.source.text);
                self.children(node, &mut scoped);
                return;
            }
            "function_item" | "closure_expression" => {
                let mut scoped = imports.clone();
                if let Some(parameters) = node.child_by_field_name("parameters") {
                    hide(parameters, &self.source.text, &mut scoped);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body, &mut scoped);
                }
                return;
            }
            "let_declaration" => {
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit(value, imports);
                }
                if let Some(pattern) = node.child_by_field_name("pattern") {
                    hide(pattern, &self.source.text, imports);
                }
                return;
            }
            "call_expression" if self.selected(node) => {
                if let Some(path) = node
                    .child_by_field_name("function")
                    .and_then(|function| imports.resolve(function, &self.source.text))
                    && self.assertion.functions.contains(&path)
                {
                    self.report(
                        node,
                        &format!(
                            "ambient process operation '{path}' is outside an approved boundary"
                        ),
                        None,
                    );
                }
            }
            "macro_invocation" => {
                if self.selected(node)
                    && let Some(path) = node
                        .child_by_field_name("macro")
                        .and_then(|name| imports.resolve(name, &self.source.text))
                    && self.assertion.macros.contains(&path)
                {
                    self.report(
                        node,
                        &format!(
                            "configured compile-time environment macr\
                o '{path}' is outside an approved boundary"
                        ),
                        None,
                    );
                }
                return;
            }
            "static_item" if self.selected(node) => self.global(node, imports),
            _ => {}
        }
        self.children(node, imports);
    }
    fn children(&mut self, node: Node<'_>, imports: &mut Imports) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, imports);
        }
    }
    fn global(&mut self, node: Node<'_>, imports: &Imports) {
        let Some(ty) = node.child_by_field_name("type") else {
            return;
        };
        let name = node
            .child_by_field_name("name")
            .map(|name| self.text(name))
            .unwrap_or("");
        let mut names = vec![name.to_owned()];
        identifiers(ty, &self.source.text, &mut names);
        let semantic = names.iter().any(|name| {
            words(name)
                .iter()
                .any(|word| self.assertion.global_words.contains(word))
        });
        if !semantic {
            return;
        }
        let mut outer = ty;
        while outer.kind() == "generic_type" {
            let Some(inner) = outer.child_by_field_name("type") else {
                break;
            };
            outer = inner;
        }
        if imports
            .resolve(outer, &self.source.text)
            .is_some_and(|path| self.assertion.global_types.contains(&path))
        {
            self.report(
                node,
                &format!(
                    "ambient configuration/state global '{name}' hide\
                s process-wide input and lifecycle"
                ),
                Some(ty),
            );
        }
    }
    fn report(&mut self, node: Node<'_>, message: &str, evidence: Option<Node<'_>>) {
        let mut owner = node;
        while let Some(parent) = owner.parent() {
            if matches!(owner.kind(), "function_item" | "static_item") {
                break;
            }
            owner = parent;
        }
        let evidence = evidence.unwrap_or(owner);
        self.findings.push(Finding {
            rule: EnvironmentAccess::ID,
            path: self.source.path.clone(),
            configuration: self.assertion.setting.clone(),
            span: Some(Span::new(&self.source.text, node.byte_range())),
            related: vec![Evidence {
                path: self.source.path.clone(),
                span: Some(Span::new(&self.source.text, evidence.byte_range())),
                message: "\
                Owning scope or configured global type requires explicit input ownership\
                ."
                .into(),
            }],
            message: message.into(),
            instruction: "Capture and validate process input at\
                \u{20}a configured composition or platform boundary, then inject owned c\
                onfiguration or a capability."
                .into(),
        });
    }
}
fn hide(node: Node<'_>, text: &str, imports: &mut Imports) {
    if node.kind() == "parameter" {
        if let Some(pattern) = node.child_by_field_name("pattern") {
            hide(pattern, text, imports);
        }
        return;
    }
    if node.kind() == "identifier" {
        imports.aliases.insert(text[node.byte_range()].into(), None);
    }
    if node.kind().ends_with("type") {
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        hide(child, text, imports);
    }
}
fn identifiers(node: Node<'_>, text: &str, names: &mut Vec<String>) {
    if node.kind() == "type_identifier" {
        names.push(text[node.byte_range()].into());
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        identifiers(child, text, names);
    }
}
fn words(name: &str) -> Vec<String> {
    let chars: Vec<_> = name.chars().collect();
    let mut text = String::new();
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch.is_uppercase()
            && index > 0
            && (chars[index - 1].is_lowercase()
                || chars.get(index + 1).is_some_and(|ch| ch.is_lowercase()))
        {
            text.push(' ');
        }
        text.extend(ch.to_lowercase());
    }
    text.split(|ch: char| !ch.is_alphabetic())
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}
