use super::Owner;
use crate::{
    Source,
    declaration::{Identity, Index},
};
use std::collections::BTreeMap;
use tree_sitter::Node;

pub(super) fn returned(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    context: &Identity,
) -> Option<String> {
    if node.kind() == "generic_type" {
        let wrapper = index.resolve(source, node.child_by_field_name("type")?, context)?;
        if !matches!(wrapper.as_str(), "std:Option" | "std:Result") {
            return None;
        }
        return returned(
            node.child_by_field_name("type_arguments")?.named_child(0)?,
            source,
            index,
            context,
        );
    }
    if !matches!(node.kind(), "type_identifier" | "scoped_type_identifier") {
        return None;
    }
    index
        .resolve(source, node, context)
        .filter(|name| name.starts_with("nominal:"))
}

pub(super) fn entrypoint(node: Node<'_>, source: &Source) -> bool {
    if node
        .child_by_field_name("name")
        .is_some_and(|name| &source.text[name.byte_range()] == "main")
    {
        return true;
    }
    let mut cursor = node.walk();
    if node.named_children(&mut cursor).any(|child| {
        child.kind() == "function_modifiers" && source.text[child.byte_range()].contains("extern")
    }) {
        return true;
    }
    let mut sibling = node.prev_named_sibling();
    while let Some(attribute) = sibling {
        if matches!(attribute.kind(), "line_comment" | "block_comment") {
            sibling = attribute.prev_named_sibling();
            continue;
        }
        if attribute.kind() != "attribute_item" {
            break;
        }
        let text = &source.text[attribute.byte_range()];
        let name = text
            .trim_start_matches("#[")
            .split(['(', '=', ']'])
            .next()
            .unwrap_or("")
            .trim();
        if !matches!(
            name,
            "cfg"
                | "allow"
                | "warn"
                | "deny"
                | "forbid"
                | "doc"
                | "inline"
                | "cold"
                | "must_use"
                | "deprecated"
                | "track_caller"
        ) {
            return true;
        }
        sibling = attribute.prev_named_sibling();
    }
    false
}
pub(super) fn entity_input(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    context: &Identity,
) -> bool {
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = parameters.walk();
    parameters
        .named_children(&mut cursor)
        .filter_map(|parameter| parameter.child_by_field_name("type"))
        .any(|mut ty| {
            while ty.kind() == "reference_type" {
                let Some(inner) = ty.child_by_field_name("type") else {
                    break;
                };
                ty = inner;
            }
            index
                .resolve(source, ty, context)
                .is_some_and(|name| name.starts_with("nominal:"))
        })
}

pub(super) struct Evidence<'a, 'b> {
    pub source: &'a Source,
    pub function: Node<'b>,
    pub index: &'b Index<'a>,
    pub context: &'b Identity,
    pub owners: &'b BTreeMap<String, Owner<'a>>,
}
impl Evidence<'_, '_> {
    pub fn terminal<'tree>(&self, node: Node<'tree>) -> Option<Node<'tree>> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .filter(|child| {
                !matches!(
                    child.kind(),
                    "line_comment" | "block_comment" | "attribute_item"
                )
            })
            .last()
    }
    fn resolve(&self, node: Node<'_>) -> Option<String> {
        if !matches!(
            node.kind(),
            "identifier" | "type_identifier" | "scoped_identifier" | "scoped_type_identifier"
        ) {
            return None;
        }
        let name = &self.source.text[node.byte_range()];
        if self.shadowed(name.split("::").next().unwrap_or(name)) {
            return None;
        }
        self.index
            .resolve_name(name, self.context)
            .filter(|name| self.owners.contains_key(name))
    }
    pub fn constructed(&self, node: Node<'_>) -> Option<String> {
        match node.kind() {
            "struct_expression" => self.resolve(node.child_by_field_name("name")?),
            "identifier" | "scoped_identifier" => self.resolve(node).or_else(|| {
                let owner = self.resolve(node.child_by_field_name("path")?)?;
                self.owners
                    .get(&owner)
                    .filter(|owner| owner.node.kind() == "enum_item")
                    .map(|_| owner)
            }),
            "block" => self.constructed(self.terminal(node)?),
            "parenthesized_expression"
            | "try_expression"
            | "return_expression"
            | "expression_statement" => self.constructed(node.named_child(0)?),
            "call_expression" => {
                let function = node.child_by_field_name("function")?;
                let name = &self.source.text[function.byte_range()];
                if matches!(name, "Ok" | "Some") {
                    if self.shadowed(name)
                        || named_function(self.source.syntax.root_node(), name, &self.source.text)
                    {
                        return None;
                    }
                    let arguments = node.child_by_field_name("arguments")?;
                    return (arguments.named_child_count() == 1)
                        .then(|| self.constructed(arguments.named_child(0)?))
                        .flatten();
                }
                if let Some(owner) = self.resolve(function) {
                    return Some(owner);
                }
                if function.kind() == "scoped_identifier" {
                    let method = function.child_by_field_name("name")?;
                    if matches!(
                        &self.source.text[method.byte_range()],
                        "from" | "try_from" | "into" | "try_into" | "as_ref" | "as_mut"
                    ) {
                        return None;
                    }
                    return self.resolve(function.child_by_field_name("path")?);
                }
                None
            }
            _ => None,
        }
    }
    fn shadowed(&self, name: &str) -> bool {
        bound(self.function, name, &self.source.text)
    }
    pub fn orchestration(&self, body: Node<'_>, terminal: Node<'_>, owner: &str) -> bool {
        let mut cursor = body.walk();
        body.named_children(&mut cursor)
            .filter(|node| node.start_byte() < terminal.start_byte())
            .any(|node| self.other(node, owner))
    }
    fn other(&self, node: Node<'_>, owner: &str) -> bool {
        if self
            .constructed(node)
            .is_some_and(|constructed| constructed != owner)
        {
            return true;
        }
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .any(|child| self.other(child, owner))
    }
}

fn bound(node: Node<'_>, name: &str, text: &str) -> bool {
    if matches!(node.kind(), "let_declaration" | "parameter")
        && let Some(pattern) = node.child_by_field_name("pattern")
        && pattern_name(pattern, name, text)
    {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| bound(child, name, text))
}
fn pattern_name(node: Node<'_>, name: &str, text: &str) -> bool {
    if node.kind() == "identifier" && &text[node.byte_range()] == name {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| pattern_name(child, name, text))
}
fn named_function(node: Node<'_>, name: &str, text: &str) -> bool {
    if node.kind() == "function_item"
        && node
            .child_by_field_name("name")
            .is_some_and(|value| &text[value.byte_range()] == name)
    {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| named_function(child, name, text))
}
