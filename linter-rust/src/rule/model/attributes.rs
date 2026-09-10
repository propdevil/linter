use crate::Source;
use syn::{Meta, Token, punctuated::Punctuated};
use tree_sitter::Node;

pub(super) fn attributes(node: Node<'_>, source: &Source) -> Vec<Meta> {
    let mut items = Vec::new();
    let mut previous = node.prev_named_sibling();
    while let Some(item) = previous {
        match item.kind() {
            "attribute_item" => {
                let mut cursor = item.walk();
                if let Some(attribute) = item
                    .named_children(&mut cursor)
                    .find(|child| child.kind() == "attribute")
                    && let Ok(meta) = syn::parse_str(&source.text[attribute.byte_range()])
                {
                    items.push(meta);
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        previous = item.prev_named_sibling();
    }
    items.reverse();
    items
}

pub(super) fn serialized(attributes: &[Meta]) -> bool {
    attributes.iter().any(|meta| {
        if meta.path().is_ident("serde") {
            return true;
        }
        let Meta::List(list) = meta else {
            return false;
        };
        list.path.is_ident("derive")
            && list
                .parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
                .is_ok_and(|paths| {
                    paths.iter().any(|path| {
                        path.segments.last().is_some_and(|segment| {
                            matches!(
                                segment.ident.to_string().as_str(),
                                "Serialize" | "Deserialize"
                            )
                        })
                    })
                })
    })
}

pub(super) fn renamed(default: &str, attributes: &[Meta]) -> String {
    let mut name = default.to_owned();
    for meta in attributes {
        let Meta::List(list) = meta else {
            continue;
        };
        if !list.path.is_ident("serde") {
            continue;
        }
        let _ = list.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                name = meta.value()?.parse::<syn::LitStr>()?.value();
            } else if meta.input.peek(Token![=]) {
                let _: syn::Expr = meta.value()?.parse()?;
            }
            Ok(())
        });
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
