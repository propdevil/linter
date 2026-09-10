use super::{
    AsyncBlocking,
    config::{Assertion, Scope},
    environment::{Binding, Environment, name},
};
use crate::Source;
use linter::{Evidence, Finding, Rule, Span};
use std::{collections::BTreeSet, ops::Range};
use tree_sitter::Node;

pub(super) fn inspect(
    source: &Source,
    tests: &[bool],
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) {
    Scan {
        source,
        tests,
        assertion,
        findings,
        reported: BTreeSet::new(),
    }
    .visit(
        source.syntax.root_node(),
        &mut Environment::default(),
        false,
    );
}
struct Scan<'a> {
    source: &'a Source,
    tests: &'a [bool],
    assertion: &'a Assertion,
    findings: &'a mut Vec<Finding>,
    reported: BTreeSet<usize>,
}
impl Scan<'_> {
    fn text(&self, node: Node<'_>) -> &str {
        &self.source.text[node.byte_range()]
    }
    fn selected(&self, node: Node<'_>, active: bool) -> bool {
        active
            && match self.assertion.scope {
                Scope::Production => !self.tests[node.start_byte()],
                Scope::Tests => self.tests[node.start_byte()],
                Scope::All => true,
            }
    }
    fn visit(&mut self, node: Node<'_>, env: &mut Environment, active: bool) {
        match node.kind() {
            "source_file" | "declaration_list" => {
                env.items(node, &self.source.text);
                self.children(node, env, active);
            }
            "mod_item" => {
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body, &mut Environment::default(), false);
                }
            }
            "function_item" => {
                let mut function = env.clone();
                function.bindings.clear();
                function.hidden.clear();
                self.parameters(node, &mut function);
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body, &mut function, is_async(node));
                }
            }
            "block" => self.block(node, env, active),
            "async_block" | "closure_expression" => {
                let mut inner = env.clone();
                self.parameters(node, &mut inner);
                let future = node.kind() == "async_block" || is_async(node);
                if future {
                    inner.hidden.clear();
                    for (name, binding) in &mut inner.bindings {
                        if !references(node, name, &self.source.text) {
                            binding.guard = None;
                        }
                    }
                }
                if let Some(body) = node
                    .child_by_field_name("body")
                    .or_else(|| node.named_child(node.named_child_count().saturating_sub(1)))
                {
                    self.visit(body, &mut inner, active || future);
                }
            }
            "let_declaration" => self.local(node, env, active),
            "call_expression" => self.call(node, env, active),
            "await_expression" => {
                self.children(node, env, active);
                if self.selected(node, active) {
                    self.guards(node, env);
                }
            }
            "if_expression" => self.branch(node, env, active),
            "while_expression" | "for_expression" | "loop_expression" | "match_expression" => {
                let previous = env.clone();
                self.children(node, env, active);
                for (name, binding) in previous.bindings {
                    if binding.guard.is_some() {
                        env.bindings.entry(name).or_default().guard = binding.guard;
                    }
                }
                env.hidden.extend(previous.hidden);
            }
            "assignment_expression" => {
                self.children(node, env, active);
                if let Some(left) = node
                    .child_by_field_name("left")
                    .and_then(|node| name(node, &self.source.text))
                {
                    let value = node
                        .child_by_field_name("right")
                        .map(|right| self.binding(right, env))
                        .unwrap_or_default();
                    env.bindings.insert(left, value);
                }
            }
            _ => self.children(node, env, active),
        }
    }
    fn children(&mut self, node: Node<'_>, env: &mut Environment, active: bool) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, env, active);
        }
    }
    fn block(&mut self, node: Node<'_>, env: &mut Environment, active: bool) {
        let previous = env.clone();
        let mut locals = BTreeSet::new();
        env.items(node, &self.source.text);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "let_declaration"
                && let Some(name) = child
                    .child_by_field_name("pattern")
                    .and_then(|pattern| name(pattern, &self.source.text))
            {
                locals.insert(name);
            }
            self.visit(child, env, active);
        }
        env.aliases = previous.aliases;
        env.hidden = previous.hidden;
        env.bindings
            .retain(|name, _| previous.bindings.contains_key(name));
        for name in locals {
            if let Some(value) = previous.bindings.get(&name) {
                env.bindings.insert(name, value.clone());
            }
        }
    }
    fn parameters(&self, node: Node<'_>, env: &mut Environment) {
        let Some(parameters) = node.child_by_field_name("parameters") else {
            return;
        };
        let mut cursor = parameters.walk();
        for parameter in parameters.named_children(&mut cursor) {
            let pattern = parameter
                .child_by_field_name("pattern")
                .unwrap_or(parameter);
            if let Some(name) = name(pattern, &self.source.text) {
                let ty = parameter
                    .child_by_field_name("type")
                    .and_then(|ty| self.ty(ty, env));
                env.bindings.insert(name, Binding { ty, guard: None });
            }
        }
    }
    fn ty(&self, mut node: Node<'_>, env: &Environment) -> Option<String> {
        while matches!(node.kind(), "reference_type" | "generic_type") {
            node = node.child_by_field_name("type")?;
        }
        env.resolve(node, &self.source.text)
    }
    fn binding(&self, node: Node<'_>, env: &Environment) -> Binding {
        if node.kind() == "identifier" {
            return env
                .bindings
                .get(self.text(node))
                .cloned()
                .unwrap_or_default();
        }
        Binding {
            ty: self.receiver_type(node, env),
            guard: self.acquisition(node, env),
        }
    }
    fn local(&mut self, node: Node<'_>, env: &mut Environment, active: bool) {
        let value = node.child_by_field_name("value");
        if let Some(value) = value {
            self.visit(value, env, active);
        }
        let Some(name) = node
            .child_by_field_name("pattern")
            .and_then(|pattern| name(pattern, &self.source.text))
        else {
            return;
        };
        let mut binding = value
            .map(|value| self.binding(value, env))
            .unwrap_or_default();
        if let Some(ty) = node
            .child_by_field_name("type")
            .and_then(|ty| self.ty(ty, env))
        {
            binding.ty = Some(ty);
        }
        if binding.guard.is_some()
            && let Some(value) = value.filter(|value| value.kind() == "identifier")
            && let Some(previous) = env.bindings.get_mut(self.text(value))
        {
            previous.guard = None;
        }
        if let Some(previous) = env
            .bindings
            .get(&name)
            .and_then(|value| value.guard.clone())
        {
            env.hidden.insert(previous.start, previous);
        }
        env.bindings.insert(name, binding);
    }
    fn receiver_type(&self, node: Node<'_>, env: &Environment) -> Option<String> {
        if node.kind() == "identifier" {
            return env.bindings.get(self.text(node))?.ty.clone();
        }
        if matches!(
            node.kind(),
            "reference_expression" | "parenthesized_expression"
        ) {
            return self.receiver_type(node.named_child(0)?, env);
        }
        if node.kind() != "call_expression" {
            return None;
        }
        let function = node.child_by_field_name("function")?;
        if let Some(path) = env.resolve(function, &self.source.text) {
            return self
                .assertion
                .methods
                .iter()
                .find(|policy| policy.constructors.contains(&path))
                .map(|policy| policy.receiver.clone());
        }
        let (receiver, method) = method(function, &self.source.text)?;
        let ty = self.receiver_type(receiver, env)?;
        self.assertion
            .methods
            .iter()
            .find(|policy| {
                policy.receiver == ty && policy.fluent_methods.iter().any(|name| name == method)
            })
            .map(|_| ty)
    }
    fn acquisition(&self, node: Node<'_>, env: &Environment) -> Option<Range<usize>> {
        if matches!(node.kind(), "try_expression" | "parenthesized_expression") {
            return self.acquisition(node.named_child(0)?, env);
        }
        if node.kind() != "call_expression" {
            return None;
        }
        let (receiver, method) = method(node.child_by_field_name("function")?, &self.source.text)?;
        if self.assertion.adapters.contains(method) {
            return self.acquisition(receiver, env);
        }
        let ty = self.receiver_type(receiver, env)?;
        self.assertion
            .methods
            .iter()
            .any(|policy| {
                policy.receiver == ty
                    && policy.returns_guard
                    && policy.methods.iter().any(|name| name == method)
            })
            .then(|| node.byte_range())
    }
    fn call(&mut self, node: Node<'_>, env: &mut Environment, active: bool) {
        let function = node.child_by_field_name("function");
        let path = function.and_then(|function| env.resolve(function, &self.source.text));
        if self.selected(node, active) {
            if let Some(path) = path
                .as_ref()
                .filter(|path| self.assertion.functions.contains(*path))
            {
                self.report(
                    node,
                    &format!(
                        "configured blocking operation '{path}' can block the async executor thread"
                    ),
                    None,
                );
            } else if let Some((receiver, method)) =
                function.and_then(|function| method(function, &self.source.text))
                && let Some(ty) = self.receiver_type(receiver, env)
                && self.assertion.methods.iter().any(|policy| {
                    policy.receiver == ty && policy.methods.iter().any(|name| name == method)
                })
            {
                self.report(node, &format!("configured blocking method '{ty}::{method}' can block the async executor thread"), None);
            }
        }
        if let Some(function) = function {
            self.visit(function, env, active);
        }
        let Some(arguments) = node.child_by_field_name("arguments") else {
            return;
        };
        let boundary = path
            .as_ref()
            .is_some_and(|path| self.assertion.contexts.contains(path));
        let mut cursor = arguments.walk();
        for argument in arguments.named_children(&mut cursor) {
            if boundary {
                self.callback(argument, env, active);
            } else {
                self.visit(argument, env, active);
            }
        }
        if function
            .is_some_and(|function| matches!(function.kind(), "identifier" | "scoped_identifier"))
        {
            let mut cursor = arguments.walk();
            for argument in arguments
                .named_children(&mut cursor)
                .filter(|node| node.kind() == "identifier")
            {
                if let Some(binding) = env.bindings.get_mut(self.text(argument)) {
                    binding.guard = None;
                }
            }
        }
    }
    fn callback(&mut self, node: Node<'_>, env: &mut Environment, active: bool) {
        match node.kind() {
            "closure_expression" => self.visit(node, env, false),
            "parenthesized_expression" => {
                if let Some(child) = node.named_child(0) {
                    self.callback(child, env, active);
                }
            }
            "block" => {
                let mut inner = env.clone();
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();
                for (index, child) in children.iter().enumerate() {
                    if index + 1 == children.len() {
                        self.callback(*child, &mut inner, active);
                    } else {
                        self.visit(*child, &mut inner, active);
                    }
                }
            }
            _ => self.visit(node, env, active),
        }
    }
    fn branch(&mut self, node: Node<'_>, env: &mut Environment, active: bool) {
        if let Some(condition) = node.child_by_field_name("condition") {
            self.visit(condition, env, active);
        }
        let mut yes = env.clone();
        let mut no = env.clone();
        if let Some(body) = node.child_by_field_name("consequence") {
            self.visit(body, &mut yes, active);
        }
        if let Some(body) = node.child_by_field_name("alternative") {
            self.visit(body, &mut no, active);
        }
        env.hidden.extend(yes.hidden);
        env.hidden.extend(no.hidden);
        for (name, value) in &mut env.bindings {
            let left = yes.bindings.get(name);
            let right = no.bindings.get(name);
            value.guard = left
                .and_then(|value| value.guard.clone())
                .or_else(|| right.and_then(|value| value.guard.clone()));
            if left.and_then(|value| value.ty.as_ref()) != right.and_then(|value| value.ty.as_ref())
            {
                value.ty = None;
            }
        }
    }
    fn guards(&mut self, node: Node<'_>, env: &Environment) {
        for guard in env.hidden.values() {
            if self.reported.insert(guard.start) {
                self.report(
                    node,
                    "shadowed synchronous lock guard is held across await",
                    Some(guard.clone()),
                );
            }
        }
        for (name, binding) in &env.bindings {
            if let Some(guard) = &binding.guard
                && self.reported.insert(guard.start)
            {
                self.report(
                    node,
                    &format!("synchronous lock guard '{name}' is held across await"),
                    Some(guard.clone()),
                );
            }
        }
    }
    fn report(&mut self, node: Node<'_>, message: &str, guard: Option<Range<usize>>) {
        self.findings.push(Finding { rule: AsyncBlocking::ID, path: self.source.path.clone(), configuration: self.assertion.setting.clone(),
            span: Some(Span::new(&self.source.text, node.byte_range())), related: guard.map(|range| Evidence { path: self.source.path.clone(), span: Some(Span::new(&self.source.text, range)), message: "Synchronous guard acquired here and not proven dropped before suspension.".into() }).into_iter().chain(async_owner(node).map(|scope| Evidence { path: self.source.path.clone(), span: Some(Span::new(&self.source.text, scope.byte_range())), message: "Operation executes in this async lexical scope.".into() })).collect(),
            message: message.into(), instruction: "Use an asynchronous operation or isolate blocking work in a configured worker callback; release synchronous guards before awaiting.".into(),
        });
    }
}
fn method<'a>(node: Node<'a>, text: &'a str) -> Option<(Node<'a>, &'a str)> {
    if node.kind() != "field_expression" {
        return None;
    }
    Some((
        node.child_by_field_name("value")?,
        &text[node.child_by_field_name("field")?.byte_range()],
    ))
}
fn is_async(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|child| {
        child.kind() == "async"
            || (child.kind() == "function_modifiers" && {
                let mut cursor = child.walk();
                child
                    .children(&mut cursor)
                    .any(|node| node.kind() == "async")
            })
    })
}
fn references(node: Node<'_>, name: &str, text: &str) -> bool {
    if node.kind() == "identifier" && &text[node.byte_range()] == name {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| references(child, name, text))
}

fn async_owner(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        if node.kind() == "async_block"
            || (matches!(node.kind(), "function_item" | "closure_expression") && is_async(node))
        {
            return Some(node);
        }
        node = node.parent()?;
    }
}
