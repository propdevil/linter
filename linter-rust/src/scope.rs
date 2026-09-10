use crate::{Analysis, Source};
use std::path::Path;
use syn::{Meta, Token, punctuated::Punctuated};
use tree_sitter::Node;

impl Source {
    pub(crate) fn test_mask(&self, root: &Path, analysis: &Analysis) -> Vec<bool> {
        let integration = integration(self, root, analysis);
        let mut mask = vec![integration; self.text.len()];
        if !integration {
            mark_tests(self.syntax.root_node(), &self.text, &mut mask);
        }
        mask
    }
}

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
                let documentation = comment.starts_with("///") || comment.starts_with("/**");
                start = start.or(documentation.then_some(child.start_byte()));
            }
            _ => {
                mark_item(child, text, excluded, &attributes, start);
                start = None;
                attributes.clear();
            }
        }
    }
}

fn mark_item(
    node: Node<'_>,
    text: &str,
    excluded: &mut [bool],
    attributes: &[Node<'_>],
    start: Option<usize>,
) {
    let test = test_scope(node, text)
        || attributes
            .iter()
            .any(|attribute| test_attribute(*attribute, text, node.kind() == "function_item"));
    if test {
        excluded[start.unwrap_or(node.start_byte())..node.end_byte()].fill(true);
    } else {
        mark_tests(node, text, excluded);
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
                return aggregate(&values, false);
            }
            if list.path.is_ident("any") {
                return aggregate(&values, true);
            }
            None
        }
        _ => None,
    }
}
fn aggregate(values: &[Option<bool>], decisive: bool) -> Option<bool> {
    if values.contains(&Some(decisive)) {
        Some(decisive)
    } else if values.iter().all(|value| *value == Some(!decisive)) {
        Some(!decisive)
    } else {
        None
    }
}
