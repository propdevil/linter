use syn::{Expr, Lit, Meta, Token, punctuated::Punctuated};
use tree_sitter::Node;

pub(super) fn meta(node: Node<'_>, text: &str) -> Option<Meta> {
    let mut cursor = node.walk();
    let attribute = node
        .named_children(&mut cursor)
        .find(|node| node.kind() == "attribute")?;
    syn::parse_str(&text[attribute.byte_range()]).ok()
}
pub(super) fn path(attributes: &[Meta]) -> Option<String> {
    attributes.iter().find_map(|meta| {
        let Meta::NameValue(value) = meta else {
            return None;
        };
        if !value.path.is_ident("path") {
            return None;
        }
        let Expr::Lit(value) = &value.value else {
            return None;
        };
        let Lit::Str(path) = &value.lit else {
            return None;
        };
        Some(path.value())
    })
}
pub(super) fn platform(attributes: &[Meta]) -> bool {
    attributes.iter().any(|meta| {
        let Meta::List(list) = meta else { return false };
        list.path.is_ident("cfg")
            && list
                .parse_args::<Meta>()
                .is_ok_and(|meta| platform_predicate(&meta))
    })
}
fn platform_predicate(meta: &Meta) -> bool {
    match meta {
        Meta::NameValue(value) => [
            "target_arch",
            "target_os",
            "target_family",
            "target_env",
            "target_vendor",
            "target_endian",
            "target_pointer_width",
            "target_abi",
            "target_feature",
        ]
        .iter()
        .any(|key| value.path.is_ident(key)),
        Meta::Path(path) => path.is_ident("unix") || path.is_ident("windows"),
        Meta::List(list) => list
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .is_ok_and(|values| values.iter().any(platform_predicate)),
    }
}
