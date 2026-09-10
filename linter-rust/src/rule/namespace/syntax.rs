use crate::Source;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

pub(super) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}
pub(super) fn text<'a>(node: Node<'_>, source: &'a Source) -> &'a str {
    &source.text[node.byte_range()]
}
pub(super) fn attrs(node: Node<'_>) -> bool {
    let mut previous = node.prev_named_sibling();
    while let Some(item) = previous {
        if item.kind() == "attribute_item" {
            return true;
        }
        if !matches!(item.kind(), "line_comment" | "block_comment") {
            break;
        }
        previous = item.prev_named_sibling();
    }
    false
}
pub(super) fn visibility(node: Node<'_>, source: &Source) -> String {
    children(node)
        .into_iter()
        .find(|child| child.kind() == "visibility_modifier")
        .map(|node| text(node, source).split_whitespace().collect())
        .unwrap_or_default()
}
pub(super) fn transparent(node: Node<'_>, source: &Source, child: &str) -> bool {
    if node.kind() != "use_declaration" || attrs(node) || visibility(node, source) != "pub(crate)" {
        return false;
    }
    let Some(argument) = node.child_by_field_name("argument") else {
        return false;
    };
    let path = text(argument, source).trim_start_matches("self::");
    path.strip_prefix(child)
        .is_some_and(|rest| rest.starts_with("::"))
}
pub(super) fn boundary(source: &Source) -> bool {
    children(source.syntax.root_node())
        .into_iter()
        .any(|item| match item.kind() {
            "inner_attribute_item"
            | "foreign_mod_item"
            | "macro_invocation"
            | "macro_definition" => true,
            "expression_statement" => children(item)
                .iter()
                .any(|child| child.kind() == "macro_invocation"),
            "attribute_item" => {
                let attribute = children(item)
                    .into_iter()
                    .find(|child| child.kind() == "attribute");
                attribute
                    .and_then(|attribute| syn::parse_str::<syn::Meta>(text(attribute, source)).ok())
                    .is_some_and(|meta| {
                        meta.path().segments.last().is_some_and(|segment| {
                            matches!(
                                segment.ident.to_string().as_str(),
                                "cfg" | "cfg_attr" | "path" | "link" | "repr" | "doc"
                            )
                        })
                    })
            }
            "line_comment" | "block_comment" => {
                text(item, source).starts_with("///") || text(item, source).starts_with("/**")
            }
            _ => false,
        })
}
pub(super) fn references(node: Node<'_>, source: &Source, module: &str) -> bool {
    if matches!(
        node.kind(),
        "scoped_identifier" | "scoped_type_identifier" | "use_declaration"
    ) {
        let words: Vec<_> = text(node, source)
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .filter(|word| !word.is_empty())
            .collect();
        if node.kind() == "use_declaration" && words.contains(&module) {
            return true;
        }
        if words
            .iter()
            .position(|word| *word == module)
            .is_some_and(|index| index + 1 < words.len())
        {
            return true;
        }
    }
    children(node)
        .into_iter()
        .any(|child| references(child, source, module))
}
pub(super) fn declarations<'a>(node: Node<'a>, source: &'a Source, target: &Path) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    if node.kind() == "mod_item"
        && node.child_by_field_name("body").is_none()
        && !attrs(node)
        && let Some(name) = node.child_by_field_name("name")
    {
        let mut directory = source.path.parent().unwrap_or(Path::new("")).to_owned();
        let stem = source
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        if !matches!(stem, "lib" | "main" | "mod") {
            directory.push(stem);
        }
        let mut ancestry = Vec::new();
        let mut parent = node.parent();
        while let Some(item) = parent {
            if item.kind() == "mod_item"
                && let Some(name) = item.child_by_field_name("name")
            {
                ancestry.push(text(name, source));
            }
            parent = item.parent();
        }
        for ancestor in ancestry.into_iter().rev() {
            directory.push(ancestor);
        }
        let name = text(name, source).trim_start_matches("r#");
        let child: PathBuf = directory.join(name).join("mod.rs");
        if child == target {
            result.push(node);
        }
    }
    for child in children(node) {
        result.extend(declarations(child, source, target));
    }
    result
}
