use std::collections::BTreeMap;
use tree_sitter::Node;

#[derive(Clone, Default)]
pub(crate) struct Imports {
    pub aliases: BTreeMap<String, Option<String>>,
}
impl Imports {
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
                self.declaration(child, text);
                continue;
            }
            if matches!(
                child.kind(),
                "function_item"
                    | "macro_definition"
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
            "use_as_clause" => self.renamed(node, prefix, text),
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
    fn declaration(&mut self, node: Node<'_>, text: &str) {
        if let Some(argument) = node.child_by_field_name("argument") {
            self.import(argument, "", text);
        }
    }
    fn renamed(&mut self, node: Node<'_>, prefix: &str, text: &str) {
        let (Some(path), Some(alias)) = (
            node.child_by_field_name("path"),
            node.child_by_field_name("alias"),
        ) else {
            return;
        };
        let path = &text[path.byte_range()];
        let target = if path == "self" {
            prefix.into()
        } else {
            join(prefix, path)
        };
        self.alias(&text[alias.byte_range()], target);
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
