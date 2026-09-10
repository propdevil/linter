use crate::{Analysis, Source};
use std::path::Path;
use syn::{Meta, Token, punctuated::Punctuated};
use tree_sitter::Node;

pub(crate) fn integration(source: &Source, root: &Path, analysis: &Analysis) -> bool {
    let path = root.join(&source.path);
    source.path.starts_with("tests")
        || analysis.packages.iter().any(|(manifest, package)| {
            manifest
                .parent()
                .is_some_and(|parent| path.starts_with(parent.join("tests")))
                || package
                    .targets
                    .iter()
                    .any(|target| target.is_test() && target.src_path.as_std_path() == path)
        })
}

pub(crate) fn mark_tests(node: Node<'_>, text: &str, excluded: &mut [bool]) {
    let mut cursor = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor).collect();
    if test_scope(node, text) {
        excluded[node.byte_range()].fill(true);
        return;
    }
    let mut attributes = Vec::new();
    let mut start = None;
    for child in children {
        match child.kind() {
            "attribute_item" => {
                start.get_or_insert(child.start_byte());
                attributes.push(child);
            }
            "line_comment" | "block_comment" => {
                let comment = &text[child.byte_range()];
                if comment.starts_with("///") || comment.starts_with("/**") {
                    start.get_or_insert(child.start_byte());
                }
            }
            _ => {
                if test_scope(child, text)
                    || attributes.iter().any(|attribute| {
                        test_attribute(*attribute, text, child.kind() == "function_item")
                    })
                {
                    excluded[start.unwrap_or(child.start_byte())..child.end_byte()].fill(true);
                } else {
                    mark_tests(child, text, excluded);
                }
                start = None;
                attributes.clear();
            }
        }
    }
}

fn test_scope(node: Node<'_>, text: &str) -> bool {
    let scope = node.child_by_field_name("body").unwrap_or(node);
    let mut cursor = scope.walk();
    scope
        .named_children(&mut cursor)
        .any(|child| child.kind() == "inner_attribute_item" && test_attribute(child, text, false))
}

fn test_attribute(node: Node<'_>, text: &str, function: bool) -> bool {
    let mut cursor = node.walk();
    let Some(attribute) = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "attribute")
    else {
        return false;
    };
    let Ok(meta) = syn::parse_str::<Meta>(&text[attribute.byte_range()]) else {
        return false;
    };
    if function && meta.path().is_ident("test") {
        return true;
    }
    let Meta::List(list) = meta else {
        return false;
    };
    if !list.path.is_ident("cfg") {
        return false;
    }
    let Ok(predicate) = list.parse_args::<Meta>() else {
        return false;
    };
    evaluate(&predicate, false) == Some(false) && evaluate(&predicate, true) != Some(false)
}

// Unknown features and target predicates must not make production code disappear.
fn evaluate(meta: &Meta, test: bool) -> Option<bool> {
    match meta {
        Meta::Path(path) if path.is_ident("test") => Some(test),
        Meta::List(list) => {
            let predicates = list
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .ok()?;
            let values: Vec<_> = predicates
                .iter()
                .map(|predicate| evaluate(predicate, test))
                .collect();
            if list.path.is_ident("not") && values.len() == 1 {
                return values[0].map(|value| !value);
            }
            if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.iter().all(|value| *value == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.iter().all(|value| *value == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}
