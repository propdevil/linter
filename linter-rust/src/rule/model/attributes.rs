use crate::Source;
use syn::{Meta, Token, punctuated::Punctuated};
use tree_sitter::Node;

pub(super) fn attributes(node: Node<'_>, source: &Source) -> Vec<Meta> {
    let mut items: Vec<_> =
        std::iter::successors(node.prev_named_sibling(), |node| node.prev_named_sibling())
            .take_while(|node| {
                matches!(
                    node.kind(),
                    "attribute_item" | "line_comment" | "block_comment"
                )
            })
            .filter(|node| node.kind() == "attribute_item")
            .filter_map(|node| {
                let mut cursor = node.walk();
                let attribute = node
                    .named_children(&mut cursor)
                    .find(|child| child.kind() == "attribute")?;
                syn::parse_str(&source.text[attribute.byte_range()]).ok()
            })
            .collect();
    items.reverse();
    items
}
pub(super) fn serialized(attributes: &[Meta]) -> bool {
    if attributes.iter().any(|meta| meta.path().is_ident("serde")) {
        return true;
    }
    attributes
        .iter()
        .filter_map(|meta| match meta {
            Meta::List(list) if list.path.is_ident("derive") => list
                .parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
                .ok(),
            _ => None,
        })
        .flatten()
        .filter_map(|path| {
            path.segments
                .last()
                .map(|segment| segment.ident.to_string())
        })
        .any(|name| matches!(name.as_str(), "Serialize" | "Deserialize"))
}

pub(super) fn renamed(default: &str, attributes: &[Meta]) -> String {
    let mut name = default.to_owned();
    let lists = attributes.iter().filter_map(|meta| match meta {
        Meta::List(list) if list.path.is_ident("serde") => Some(list),
        _ => None,
    });
    let mut rename = |meta: syn::meta::ParseNestedMeta<'_>| {
        if meta.path.is_ident("rename") {
            name = meta.value()?.parse::<syn::LitStr>()?.value();
        } else if meta.input.peek(Token![=]) {
            let _: syn::Expr = meta.value()?.parse()?;
        }
        Ok(())
    };
    for list in lists {
        let _ = list.parse_nested_meta(&mut rename);
    }
    name
}

pub(super) fn words(name: &str) -> Vec<String> {
    use heck::ToSnakeCase;
    name.to_snake_case()
        .split('_')
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn concept(name: &str) -> Vec<String> {
    words(name)
        .into_iter()
        .filter(|word| !matches!(word.as_str(), "wire" | "api" | "dto" | "model" | "data"))
        .collect()
}

pub(super) fn projection(name: &str) -> bool {
    words(name).last().is_some_and(|word| {
        matches!(
            word.as_str(),
            "request" | "response" | "view" | "summary" | "snapshot" | "event" | "command"
        )
    })
}
