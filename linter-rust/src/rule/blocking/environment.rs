use std::{collections::BTreeMap, ops::Range};
use tree_sitter::Node;

#[derive(Clone, Default)]
pub(super) struct Binding {
    pub ty: Option<String>,
    pub guard: Option<Range<usize>>,
}
#[derive(Clone, Default)]
pub(super) struct Environment {
    pub imports: crate::imports::Imports,
    pub bindings: BTreeMap<String, Binding>,
    pub hidden: BTreeMap<usize, Range<usize>>,
}
impl Environment {
    pub fn resolve(&self, node: Node<'_>, text: &str) -> Option<String> {
        let raw = text[node.byte_range()].trim_start_matches("::");
        let first = raw.split("::").next()?;
        if self.bindings.contains_key(first) {
            return None;
        }
        self.imports.resolve(node, text)
    }
    pub fn items(&mut self, node: Node<'_>, text: &str) {
        self.imports.items(node, text);
    }
}
pub(super) fn name(node: Node<'_>, text: &str) -> Option<String> {
    if node.kind() == "identifier" {
        return Some(text[node.byte_range()].into());
    }
    if node.kind() == "mut_pattern" {
        return name(node.named_child(0)?, text);
    }
    None
}
