use super::{Allocation, Assertion};
use crate::Source;
use linter::{Evidence, Finding, Rule, Span};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
use tree_sitter::Node;

#[derive(Clone)]
struct Value {
    origin: Range<usize>,
    safe: bool,
}
type State = BTreeMap<String, Value>;
struct Flow<'a> {
    source: &'a Source,
    assertion: &'a Assertion,
    findings: &'a mut Vec<Finding>,
    reported: BTreeSet<usize>,
}

pub(super) fn inspect(source: &Source, assertion: &Assertion, findings: &mut Vec<Finding>) {
    let mut flow = Flow {
        source,
        assertion,
        findings,
        reported: BTreeSet::new(),
    };
    flow.functions(source.syntax.root_node());
}

impl Flow<'_> {
    fn text(&self, node: Node<'_>) -> &str {
        &self.source.text[node.byte_range()]
    }
    fn functions(&mut self, node: Node<'_>) {
        if node.kind() == "function_definition" {
            if let Some(body) = node.child_by_field_name("body") {
                self.statement(body, &mut State::new());
            }
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.functions(child);
        }
    }
    fn statement(&mut self, node: Node<'_>, state: &mut State) -> bool {
        match node.kind() {
            "return_statement" | "break_statement" | "continue_statement" | "goto_statement" => {
                self.expression(node, state);
                false
            }
            "if_statement" => self.branch(node, state),
            "compound_statement" => self.block(node, state),
            "while_statement" | "for_statement" | "do_statement" | "switch_statement" => {
                if let Some(initializer) = node.child_by_field_name("initializer") {
                    self.expression(initializer, state);
                }
                let before = state.clone();
                let mut inner = before.clone();
                if let Some(condition) = node.child_by_field_name("condition") {
                    self.expression(condition, &mut inner);
                    self.refine(condition, true, &mut inner);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.statement(body, &mut inner);
                }
                if let Some(update) = node.child_by_field_name("update") {
                    self.expression(update, &mut inner);
                }
                *state = merge(&before, &inner);
                true
            }
            _ => {
                self.expression(node, state);
                true
            }
        }
    }
    fn block(&mut self, node: Node<'_>, state: &mut State) -> bool {
        let saved = state.clone();
        let mut declared = Vec::new();
        let mut live = true;
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if !live {
                break;
            }
            if child.kind() == "declaration" {
                collect_declarations(child, self.source, &mut declared);
            }
            live = self.statement(child, state);
        }
        for name in declared {
            if let Some(value) = saved.get(&name) {
                state.insert(name, value.clone());
            } else {
                state.remove(&name);
            }
        }
        live
    }
    fn branch(&mut self, node: Node<'_>, state: &mut State) -> bool {
        let Some(condition) = node.child_by_field_name("condition") else {
            return true;
        };
        self.expression(condition, state);
        let mut yes = state.clone();
        let mut no = state.clone();
        self.refine(condition, true, &mut yes);
        self.refine(condition, false, &mut no);
        let yes_live = node
            .child_by_field_name("consequence")
            .is_none_or(|body| self.statement(body, &mut yes));
        let no_live = node.child_by_field_name("alternative").is_none_or(|body| {
            let body = if body.kind() == "else_clause" {
                body.named_child(0).unwrap_or(body)
            } else {
                body
            };
            self.statement(body, &mut no)
        });
        *state = match (yes_live, no_live) {
            (true, true) => merge(&yes, &no),
            (true, false) => yes,
            (false, true) => no,
            (false, false) => State::new(),
        };
        yes_live || no_live
    }
    fn expression(&mut self, node: Node<'_>, state: &mut State) {
        if node.kind() == "sizeof_expression" {
            return;
        }
        if node.kind() == "declaration" {
            self.declaration(node, state);
        }
        if self.assignment(node, state) || self.short_circuit(node, state) {
            return;
        }
        let dereference = match node.kind() {
            "subscript_expression" => node.child_by_field_name("argument"),
            "field_expression"
                if node
                    .child_by_field_name("operator")
                    .is_some_and(|op| self.text(op) == "->") =>
            {
                node.child_by_field_name("argument")
            }
            "pointer_expression"
                if node
                    .child_by_field_name("operator")
                    .is_some_and(|op| self.text(op) == "*") =>
            {
                node.child_by_field_name("argument")
            }
            _ => None,
        };
        if let Some(argument) = dereference {
            let argument = unwrap(argument);
            if argument.kind() == "identifier"
                && let Some(value) = state.get(self.text(argument))
                && !value.safe
            {
                self.report(node, argument, value);
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.expression(child, state);
        }
    }
    fn declaration(&self, node: Node<'_>, state: &mut State) {
        let mut cursor = node.walk();
        for child in node
            .named_children(&mut cursor)
            .filter(|child| child.kind() != "init_declarator")
        {
            if let Some(name) = identifier(child, self.source) {
                state.remove(&name);
            }
        }
    }
    fn assignment(&mut self, node: Node<'_>, state: &mut State) -> bool {
        if !matches!(node.kind(), "init_declarator" | "assignment_expression") {
            return false;
        }
        let left = node
            .child_by_field_name("declarator")
            .or_else(|| node.child_by_field_name("left"));
        let right = node
            .child_by_field_name("value")
            .or_else(|| node.child_by_field_name("right"));
        let (Some(left), Some(right)) = (left, right) else {
            return false;
        };
        self.expression(right, state);
        if node.kind() == "assignment_expression" && left.kind() != "identifier" {
            self.expression(left, state);
            return true;
        }
        let Some(name) = identifier(left, self.source) else {
            return true;
        };
        if let Some(value) = self.value(right, state, node.byte_range()) {
            state.insert(name, value);
        } else {
            state.remove(&name);
        }
        true
    }
    fn short_circuit(&mut self, node: Node<'_>, state: &mut State) -> bool {
        if node.kind() != "binary_expression" {
            return false;
        }
        let op = node.child_by_field_name("operator").map(|op| self.text(op));
        if !matches!(op, Some("&&" | "||")) {
            return false;
        }
        let truth = op == Some("&&");
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) else {
            return false;
        };
        self.expression(left, state);
        let mut branch = state.clone();
        self.refine(left, truth, &mut branch);
        self.expression(right, &mut branch);
        *state = merge(state, &branch);
        true
    }
    fn value(&self, node: Node<'_>, state: &State, origin: Range<usize>) -> Option<Value> {
        let node = unwrap(node);
        if node.kind() == "call_expression"
            && node
                .child_by_field_name("function")
                .is_some_and(|function| {
                    function.kind() == "identifier"
                        && self.assertion.functions.contains(self.text(function))
                })
        {
            return Some(Value {
                origin,
                safe: false,
            });
        }
        if node.kind() == "identifier" {
            return state.get(self.text(node)).cloned();
        }
        None
    }
    fn refine(&self, node: Node<'_>, truth: bool, state: &mut State) {
        let node = unwrap(node);
        if node.kind() == "identifier" && truth {
            self.prove(self.text(node), state);
            return;
        }
        let operator = node.child_by_field_name("operator").map(|op| self.text(op));
        if operator == Some("!")
            && let Some(argument) = node.child_by_field_name("argument")
        {
            self.refine(argument, !truth, state);
            return;
        }
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) else {
            return;
        };
        if (operator == Some("&&") && truth) || (operator == Some("||") && !truth) {
            self.refine(left, truth, state);
            self.refine(right, truth, state);
        }
        if (operator == Some("!=") && truth) || (operator == Some("==") && !truth) {
            self.comparison(left, right, state);
        }
    }
    fn comparison(&self, left: Node<'_>, right: Node<'_>, state: &mut State) {
        for (value, zero) in [(left, right), (right, left)] {
            let value = unwrap(value);
            if value.kind() == "identifier"
                && matches!(self.text(unwrap(zero)), "NULL" | "0" | "nullptr")
            {
                self.prove(self.text(value), state);
            }
        }
    }

    fn prove(&self, name: &str, state: &mut State) {
        if let Some(origin) = state.get(name).map(|value| value.origin.clone()) {
            for value in state.values_mut().filter(|value| value.origin == origin) {
                value.safe = true;
            }
        }
    }
    fn report(&mut self, node: Node<'_>, argument: Node<'_>, value: &Value) {
        if !self.reported.insert(value.origin.start) {
            return;
        }
        self.findings.push(Finding {
            rule: Allocation::ID,
            path: self.source.path.clone(),
            configuration: self.assertion.setting.clone(),
            span: Some(Span::new(&self.source.text, value.origin.clone())),
            related: vec![Evidence {
                path: self.source.path.clone(),
                span: Some(Span::new(&self.source.text, node.byte_range())),
                message: "\
                Dereference without an established prior non-null guard."
                    .into(),
            }],
            message: format!(
                "nullable allocation used through '{}' is dereferenced with\
                out an established prior null guard",
                self.text(argument)
            ),
            instruction: "Check the allocation before dereferencing it, and exit the nul\
                l branch or keep uses inside a proven non-null branch."
                .into(),
        });
    }
}
fn unwrap(mut node: Node<'_>) -> Node<'_> {
    loop {
        let child = match node.kind() {
            "parenthesized_expression" => node.named_child(0),
            "cast_expression" => node.child_by_field_name("value"),
            _ => None,
        };
        match child {
            Some(child) => node = child,
            None => return node,
        }
    }
}
fn identifier(node: Node<'_>, source: &Source) -> Option<String> {
    if node.kind() == "identifier" {
        return Some(source.text[node.byte_range()].into());
    }
    node.child_by_field_name("declarator")
        .and_then(|child| identifier(child, source))
}
fn collect_declarations(node: Node<'_>, source: &Source, output: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(name) = identifier(child, source) {
            output.push(name);
        }
    }
}
fn merge(left: &State, right: &State) -> State {
    let mut result = left.clone();
    for (name, value) in right {
        result
            .entry(name.clone())
            .and_modify(|other| other.merge(value))
            .or_insert_with(|| value.clone());
    }
    result
}

impl Value {
    fn merge(&mut self, other: &Self) {
        if self.safe && !other.safe {
            *self = other.clone();
        } else {
            self.safe = self.origin == other.origin && self.safe && other.safe;
        }
    }
}
