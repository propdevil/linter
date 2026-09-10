use super::{
    Assertion, Concept, Kind, Use,
    syntax::{self, children, text},
};
use crate::{Source, declaration::Index};
use std::collections::BTreeMap;
use tree_sitter::Node;

#[derive(Clone)]
struct Binding {
    key: String,
    ty: Option<String>,
}

pub(super) struct Scanner<'a, 'b> {
    source: &'a Source,
    index: &'b Index<'a>,
    assertion: &'b Assertion,
    tests: &'b [bool],
    concepts: &'b mut BTreeMap<String, Concept>,
    scopes: Vec<BTreeMap<String, Binding>>,
    owner: Option<String>,
}
impl<'a, 'b> Scanner<'a, 'b> {
    pub fn new(
        source: &'a Source,
        index: &'b Index<'a>,
        assertion: &'b Assertion,
        tests: &'b [bool],
        concepts: &'b mut BTreeMap<String, Concept>,
    ) -> Self {
        Self {
            source,
            index,
            assertion,
            tests,
            concepts,
            scopes: vec![BTreeMap::new()],
            owner: None,
        }
    }
    pub fn run(&mut self) {
        self.visit(self.source.syntax.root_node());
    }
    fn binding(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }
    fn field(&self, owner: &str, name: &str) -> Option<(String, String, Kind)> {
        if !self.assertion.candidate(name) {
            return None;
        }
        let structure = self
            .index
            .structures
            .iter()
            .find(|item| format!("nominal:{}", item.id.key()) == owner)?;
        let ty = structure.fields.get(name)?.ty.as_ref()?;
        syntax::string_type(ty).then(|| {
            (
                format!("{owner}:{name}"),
                format!("{}::{name}", structure.id.name),
                Kind::Field,
            )
        })
    }
    fn concept(&self, node: Node<'_>) -> Option<(String, String, Kind)> {
        let node = syntax::peel(node, self.source);
        if node.kind() == "identifier" {
            let name = text(node, self.source);
            if !self.assertion.candidate(name) {
                return None;
            }
            let binding = self.binding(name)?;
            if !binding.ty.as_deref().is_some_and(syntax::string_type) {
                return None;
            }
            return Some((binding.key.clone(), name.into(), Kind::Binding));
        }
        if node.kind() == "field_expression" {
            let base = syntax::peel(node.child_by_field_name("value")?, self.source);
            if text(base, self.source) != "self" {
                return None;
            }
            return self.field(
                self.owner.as_deref()?,
                text(node.child_by_field_name("field")?, self.source),
            );
        }
        None
    }
    fn record(&mut self, concept: Option<(String, String, Kind)>, value: Node<'a>, usage: Use) {
        let Some((key, name, kind)) = concept else {
            return;
        };
        if let Some((value, node)) = syntax::literal(value, self.source) {
            self.concepts
                .entry(key)
                .or_insert_with(|| Concept::new(name, kind))
                .record(value, node, self.source, usage);
        }
    }
    fn visit(&mut self, node: Node<'a>) {
        let selected = self
            .tests
            .get(node.start_byte())
            .is_some_and(|test| self.assertion.selected(*test));
        let function = matches!(node.kind(), "function_item" | "closure_expression");
        let saved = function.then(|| std::mem::replace(&mut self.scopes, vec![BTreeMap::new()]));
        let scoped = node.kind() == "block";
        if scoped {
            self.scopes.push(BTreeMap::new());
        }
        let previous_owner = self.owner.clone();
        if node.kind() == "impl_item" {
            let context = self.index.identity(self.source, node);
            self.owner = node
                .child_by_field_name("type")
                .and_then(|ty| self.index.resolve(self.source, ty, &context));
        }
        if node.kind() == "parameter" {
            self.declare(node);
        }
        if selected {
            self.inspect(node);
        }
        for child in children(node) {
            self.visit(child);
        }
        if node.kind() == "let_declaration" {
            self.declare(node);
        }
        if scoped {
            self.scopes.pop();
        }
        if let Some(saved) = saved {
            self.scopes = saved;
        }
        self.owner = previous_owner;
    }
    fn declare(&mut self, node: Node<'a>) {
        let Some(pattern) = node.child_by_field_name("pattern") else {
            return;
        };
        if pattern.kind() != "identifier" {
            return;
        }
        let name = text(pattern, self.source);
        let context = self.index.identity(self.source, node);
        let value = node.child_by_field_name("value");
        let ty = node
            .child_by_field_name("type")
            .and_then(|ty| self.index.resolve(self.source, ty, &context))
            .or_else(|| {
                value.and_then(|value| {
                    if syntax::literal(value, self.source).is_some() {
                        Some("std:String".into())
                    } else {
                        self.binding(text(syntax::peel(value, self.source), self.source))
                            .and_then(|binding| binding.ty.clone())
                    }
                })
            });
        let key = format!(
            "{}:{}:{name}",
            self.source.path.display(),
            node.start_byte()
        );
        if self.assertion.candidate(name)
            && ty.as_deref().is_some_and(syntax::string_type)
            && self
                .tests
                .get(node.start_byte())
                .is_some_and(|test| self.assertion.selected(*test))
            && let Some(value) = value
        {
            self.record(
                Some((key.clone(), name.into(), Kind::Binding)),
                value,
                Use::Assignment,
            );
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.into(), Binding { key, ty });
        }
    }
    fn inspect(&mut self, node: Node<'a>) {
        match node.kind() {
            "assignment_expression" => {
                if let (Some(left), Some(right)) = (
                    node.child_by_field_name("left"),
                    node.child_by_field_name("right"),
                ) {
                    self.record(self.concept(left), right, Use::Assignment);
                }
            }
            "binary_expression" => {
                let Some(operator) = node.child_by_field_name("operator") else {
                    return;
                };
                if !matches!(text(operator, self.source), "==" | "!=") {
                    return;
                }
                if let (Some(left), Some(right)) = (
                    node.child_by_field_name("left"),
                    node.child_by_field_name("right"),
                ) {
                    self.record(self.concept(left), right, Use::Decision);
                    self.record(self.concept(right), left, Use::Decision);
                }
            }
            "struct_expression" => self.construct(node),
            "match_expression" => self.matches(node),
            "call_expression" => self.setter(node),
            _ => {}
        }
    }
    fn construct(&mut self, node: Node<'a>) {
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        let context = self.index.identity(self.source, node);
        let owner = if text(name, self.source) == "Self" {
            self.owner.clone()
        } else {
            self.index.resolve(self.source, name, &context)
        };
        let (Some(owner), Some(body)) = (owner, node.child_by_field_name("body")) else {
            return;
        };
        for field in children(body) {
            if let (Some(name), Some(value)) = (
                field.child_by_field_name("field"),
                field.child_by_field_name("value"),
            ) {
                self.record(
                    self.field(&owner, text(name, self.source)),
                    value,
                    Use::Assignment,
                );
            }
        }
    }
    fn matches(&mut self, node: Node<'a>) {
        let Some(value) = node.child_by_field_name("value") else {
            return;
        };
        let Some((key, name, kind)) = self.concept(value) else {
            return;
        };
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        for arm in children(body) {
            let (Some(pattern), Some(value)) = (
                arm.child_by_field_name("pattern"),
                arm.child_by_field_name("value"),
            ) else {
                continue;
            };
            let concept = self
                .concepts
                .entry(key.clone())
                .or_insert_with(|| Concept::new(name.clone(), kind));
            concept.open |= syntax::preserves(
                pattern,
                value,
                name.rsplit("::").next().unwrap_or(&name),
                self.source,
            );
            for (literal, node) in syntax::literals(pattern, self.source) {
                concept.record(literal, node, self.source, Use::Decision);
            }
        }
    }
    fn setter(&mut self, node: Node<'a>) {
        let Some((method, receiver, args)) = syntax::method(node, self.source) else {
            return;
        };
        let Some(name) = method
            .strip_prefix("set_")
            .filter(|name| self.assertion.candidate(name))
        else {
            return;
        };
        if args.len() != 1 {
            return;
        }
        let receiver = syntax::peel(receiver, self.source);
        let Some(binding) = self.binding(text(receiver, self.source)) else {
            return;
        };
        if !binding
            .ty
            .as_deref()
            .is_some_and(|ty| ty.contains("nominal:"))
        {
            return;
        }
        self.record(
            Some((
                format!("{}:{method}", binding.key),
                name.into(),
                Kind::Target,
            )),
            args[0],
            Use::Assignment,
        );
    }
}
