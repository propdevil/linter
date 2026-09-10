use std::{collections::BTreeMap, ops::Range};
use tree_sitter::Node;

#[derive(Clone, Default)]
pub(super) struct Binding {
    pub ty: Option<String>,
    pub guard: Option<Range<usize>>,
}
#[derive(Clone, Default)]
pub(super) struct Environment {
    pub aliases: BTreeMap<String, Option<String>>,
    pub bindings: BTreeMap<String, Binding>,
    pub hidden: BTreeMap<usize, Range<usize>>,
}
impl Environment {
    pub fn resolve(&self, node: Node<'_>, text: &str) -> Option<String> {
        if node.kind() == "generic_function" {
            return self.resolve(node.child_by_field_name("function")?, text);
        }
        if !matches!(
            node.kind(),
            "identifier" | "scoped_identifier" | "type_identifier" | "scoped_type_identifier"
        ) {
            return None;
        }
        let name: String = text[node.byte_range()].split_whitespace().collect();
        let name = name.trim_start_matches("::");
        let (first, rest) = name.split_once("::").unwrap_or((name, ""));
        if self.bindings.contains_key(first) {
            return None;
        }
        match self.aliases.get(first) {
            Some(Some(alias)) => Some(join(alias, rest)),
            Some(None) => None,
            None => Some(name.into()),
        }
    }
    pub fn items(&mut self, node: Node<'_>, text: &str) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "use_declaration" {
                if let Some(argument) = child.child_by_field_name("argument") {
                    self.import(argument, "", text);
                }
            } else if matches!(
                child.kind(),
                "function_item"
                    | "struct_item"
                    | "enum_item"
                    | "type_item"
                    | "mod_item"
                    | "const_item"
                    | "static_item"
            ) && let Some(name) = child.child_by_field_name("name")
            {
                self.aliases.insert(text[name.byte_range()].into(), None);
            }
        }
    }
    fn import(&mut self, node: Node<'_>, prefix: &str, text: &str) {
        match node.kind() {
            "use_list" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.import(child, prefix, text);
                }
            }
            "scoped_use_list" => {
                let path = node
                    .child_by_field_name("path")
                    .map(|path| &text[path.byte_range()])
                    .unwrap_or("");
                if let Some(list) = node.child_by_field_name("list") {
                    self.import(list, &join(prefix, path), text);
                }
            }
            "use_as_clause" => {
                if let (Some(path), Some(alias)) = (
                    node.child_by_field_name("path"),
                    node.child_by_field_name("alias"),
                ) {
                    self.alias(
                        &text[alias.byte_range()],
                        join(prefix, &text[path.byte_range()]),
                    );
                }
            }
            "identifier" | "scoped_identifier" | "self" => {
                let path = &text[node.byte_range()];
                let joined = if path == "self" {
                    prefix.into()
                } else {
                    join(prefix, path)
                };
                let name = joined.rsplit("::").next().unwrap_or("").to_owned();
                self.alias(&name, joined);
            }
            _ => {}
        }
    }
    fn alias(&mut self, name: &str, path: String) {
        self.aliases
            .entry(name.into())
            .and_modify(|value| {
                if value.as_deref() != Some(&path) {
                    *value = None;
                }
            })
            .or_insert(Some(path));
    }
}
fn join(prefix: &str, suffix: &str) -> String {
    match (prefix.is_empty(), suffix.is_empty()) {
        (true, _) => suffix.into(),
        (_, true) => prefix.into(),
        _ => format!("{prefix}::{suffix}"),
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
