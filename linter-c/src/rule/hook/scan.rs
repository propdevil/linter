use super::corpus::{Context, Corpus, Site};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use tree_sitter::Node;
/// Resolves the file-scope name an lvalue reaches through subscripts, members, and indirection.
///
/// `state[index].field` and `*state` both name `state`; anything else names nothing this rule
/// tracks, because only file-scope declarations enter `Corpus::state`.
fn base_identifier(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        match current.kind() {
            "identifier" => return Some(current),
            "subscript_expression" | "field_expression" => {
                current = current
                    .child_by_field_name("argument")
                    .or_else(|| current.named_child(0))?;
            }
            "parenthesized_expression" => current = current.named_child(0)?,
            "pointer_expression" => current = current.child_by_field_name("argument")?,
            "cast_expression" => current = current.child_by_field_name("value")?,
            _ => return None,
        }
    }
}

pub(super) struct FileScan<'a> {
    pub(super) path: &'a Path,
    pub(super) source: &'a [u8],
    pub(super) macros: &'a BTreeSet<String>,
    pub(super) corpus: &'a mut Corpus,
    pub(super) names: &'a BTreeMap<String, String>,
    pub(super) locals: Vec<BTreeSet<String>>,
}

impl FileScan<'_> {
    fn text(&self, node: Node<'_>) -> String {
        node.utf8_text(self.source).unwrap_or_default().to_owned()
    }

    fn site(&self, node: Node<'_>, context: &Context<'_>) -> Site {
        Site {
            path: self.path.to_owned(),
            span: linter::Span::new(
                std::str::from_utf8(self.source).unwrap_or_default(),
                node.byte_range(),
            ),
            function: context.function.map(ToOwned::to_owned),
            test_only: context.test_only,
        }
    }

    pub(super) fn walk(&mut self, node: Node<'_>, context: &Context<'_>) {
        if matches!(node.kind(), "compound_statement" | "for_statement") {
            self.locals.push(BTreeSet::new());
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                self.walk(
                    child,
                    &Context {
                        predicate: context.predicate || condition_field(node, child),
                        ..*context
                    },
                );
            }
            self.locals.pop();
            return;
        }
        if node.kind() == "declaration" && context.function.is_some() {
            let mut cursor = node.walk();
            let external = node.named_children(&mut cursor).any(|child| {
                child.kind() == "storage_class_specifier" && self.text(child) == "extern"
            });
            for child in node.children_by_field_name("declarator", &mut cursor) {
                if !external
                    && !is_function_declarator(child)
                    && let Some(identifier) = declared_identifier(child)
                {
                    let name = self.text(identifier);
                    if let Some(scope) = self.locals.last_mut() {
                        scope.insert(name);
                    }
                }
            }
        }
        match node.kind() {
            "preproc_if" | "preproc_ifdef" | "preproc_elif" | "preproc_elifdef" => {
                self.walk_conditional(node, context);
                return;
            }
            "function_definition" => {
                self.walk_definition(node, context);
                return;
            }
            "declaration" if context.function.is_none() => {
                self.record_file_scope_declaration(node, context);
            }
            "assignment_expression" => {
                self.record_assignment(node, context);
            }
            "update_expression" => {
                if let Some(argument) = node.child_by_field_name("argument")
                    && let Some(base) = base_identifier(argument)
                {
                    self.record_write(base, context);
                }
            }
            // Handing a callee the address of file-scope state is a write: the value the
            // production build observes afterwards is whatever the callee stored. The engine
            // fills `g_rprocs` exclusively through `ckpt_vector_reserve((void **)&g_rprocs, ...)`
            // and `g_rprocs[i].field = ...`, so an assignment-only model reported every restore
            // as unwritten and named the test-only fixture as the sole writer.
            "pointer_expression" => {
                if let Some(operator) = node.child_by_field_name("operator")
                    && self.text(operator) == "&"
                    && let Some(argument) = node.child_by_field_name("argument")
                    && let Some(base) = base_identifier(argument)
                {
                    self.record_write(base, context);
                }
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function")
                    && function.kind() == "identifier"
                {
                    let spelling = self.text(function);
                    if self.shadowed(&spelling) {
                        return;
                    }
                    let name = self.names.get(&spelling).cloned().unwrap_or(spelling);
                    let site = self.site(function, context);
                    self.corpus.calls.entry(name).or_default().push(site);
                }
            }
            "identifier" if context.predicate && !context.test_only => {
                let spelling = self.text(node);
                if !self.shadowed(&spelling)
                    && let Some(name) = self.names.get(&spelling)
                {
                    let site = self.site(node, context);
                    self.corpus
                        .predicate_reads
                        .entry(name.clone())
                        .or_default()
                        .push(site);
                }
            }
            _ => {}
        }
        let inherited = Context {
            predicate: context.predicate || is_predicate(node),
            ..*context
        };
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let child_context = Context {
                predicate: inherited.predicate || condition_field(node, child),
                ..inherited
            };
            self.walk(child, &child_context);
        }
    }

    fn record_assignment(&mut self, node: Node<'_>, context: &Context<'_>) {
        let Some(left) = node.child_by_field_name("left") else {
            return;
        };
        let Some(base) = base_identifier(left) else {
            return;
        };
        self.record_write(base, context);
    }

    fn record_write(&mut self, node: Node<'_>, context: &Context<'_>) {
        let spelling = self.text(node);
        if self.shadowed(&spelling) {
            return;
        }
        if let Some(name) = self.names.get(&spelling) {
            let site = self.site(node, context);
            self.corpus
                .writes
                .entry(name.clone())
                .or_default()
                .push(site);
        }
    }

    fn shadowed(&self, name: &str) -> bool {
        self.locals.iter().rev().any(|scope| scope.contains(name))
    }

    fn walk_conditional(&mut self, node: Node<'_>, context: &Context<'_>) {
        let truth = super::condition::production(node, self.source, self.macros);
        let alternative = node.child_by_field_name("alternative");
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if Some(child) == node.child_by_field_name("condition")
                || Some(child) == node.child_by_field_name("name")
            {
                continue;
            }
            let unavailable = if Some(child) == alternative {
                truth == Some(true)
            } else {
                truth == Some(false)
            };
            self.walk(
                child,
                &Context {
                    test_only: context.test_only || unavailable,
                    ..*context
                },
            );
        }
    }

    fn walk_definition(&mut self, node: Node<'_>, context: &Context<'_>) {
        let Some(name) = node
            .child_by_field_name("declarator")
            .and_then(declared_identifier)
            .map(|identifier| self.text(identifier))
        else {
            return;
        };
        let name = self.names.get(&name).cloned().unwrap_or(name);
        self.corpus.defined.insert(name.clone());
        if context.test_only {
            self.corpus.test_only_definitions.insert(name.clone());
        } else {
            self.corpus.production_definitions.insert(name.clone());
        }
        let body_context = Context {
            function: Some(&name),
            predicate: false,
            test_only: context.test_only,
        };
        let mut parameters = BTreeSet::new();
        if let Some(declarator) = node.child_by_field_name("declarator") {
            parameter_names(declarator, self.source, &mut parameters);
        }
        self.locals.push(parameters);
        if let Some(body) = node.child_by_field_name("body") {
            self.walk(body, &body_context);
        }
        self.locals.pop();
    }

    /// Records a file-scope object and any initializer that establishes real state.
    ///
    /// A zero initializer is the absence of a writer, not a production one: it is the
    /// value the shipped build is stuck with when every assignment is test-only.
    fn record_file_scope_declaration(&mut self, node: Node<'_>, context: &Context<'_>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "pointer_declarator" | "array_declarator" => {
                    if let Some(identifier) = declared_identifier(child)
                        && !is_function_declarator(child)
                    {
                        let spelling = self.text(identifier);
                        if let Some(name) = self.names.get(&spelling) {
                            self.corpus
                                .state
                                .insert(name.clone(), self.site(identifier, context));
                        }
                    }
                }
                "init_declarator" => {
                    let Some(identifier) = child
                        .child_by_field_name("declarator")
                        .and_then(declared_identifier)
                    else {
                        continue;
                    };
                    let spelling = self.text(identifier);
                    let Some(name) = self.names.get(&spelling).cloned() else {
                        continue;
                    };
                    self.corpus
                        .state
                        .insert(name.clone(), self.site(identifier, context));
                    let initializer = child.child_by_field_name("value");
                    if initializer.is_some_and(|value| !self.is_zero(value)) {
                        let site = self.site(identifier, context);
                        self.corpus.writes.entry(name).or_default().push(site);
                    }
                }
                _ => {}
            }
        }
    }

    fn is_zero(&self, node: Node<'_>) -> bool {
        matches!(
            self.text(node).trim(),
            "0" | "0u" | "0U" | "NULL" | "false" | "{0}" | "{}"
        )
    }
}

fn is_function_declarator(node: Node<'_>) -> bool {
    node.kind() == "function_declarator"
        || node
            .child_by_field_name("declarator")
            .is_some_and(is_function_declarator)
}

pub(super) fn declared_identifier(node: Node<'_>) -> Option<Node<'_>> {
    match node.kind() {
        "identifier" => Some(node),
        _ => node
            .child_by_field_name("declarator")
            .and_then(declared_identifier),
    }
}

/// Reports whether the node is itself a test of a value.
fn is_predicate(node: Node<'_>) -> bool {
    match node.kind() {
        "unary_expression" => node
            .child_by_field_name("operator")
            .is_some_and(|operator| operator.kind() == "!"),
        "binary_expression" => node
            .child_by_field_name("operator")
            .is_some_and(|operator| {
                matches!(
                    operator.kind(),
                    "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
                )
            }),
        _ => false,
    }
}

/// Reports whether the child occupies the condition position of its parent.
fn condition_field(parent: Node<'_>, child: Node<'_>) -> bool {
    matches!(
        parent.kind(),
        "if_statement"
            | "while_statement"
            | "do_statement"
            | "for_statement"
            | "conditional_expression"
    ) && parent.child_by_field_name("condition") == Some(child)
}

fn parameter_names(node: Node<'_>, source: &[u8], names: &mut BTreeSet<String>) {
    if node.kind() == "parameter_declaration" {
        if let Some(identifier) = node
            .child_by_field_name("declarator")
            .and_then(declared_identifier)
        {
            names.insert(identifier.utf8_text(source).unwrap_or_default().into());
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        parameter_names(child, source, names);
    }
}
