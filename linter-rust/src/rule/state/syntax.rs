use crate::Source;
use tree_sitter::Node;

pub(super) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

pub(super) fn text<'a>(node: Node<'_>, source: &'a Source) -> &'a str {
    &source.text[node.byte_range()]
}

pub(super) fn method<'a, 's>(
    node: Node<'a>,
    source: &'s Source,
) -> Option<(&'s str, Node<'a>, Vec<Node<'a>>)> {
    if node.kind() != "call_expression" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }
    Some((
        text(function.child_by_field_name("field")?, source),
        function.child_by_field_name("value")?,
        children(node.child_by_field_name("arguments")?),
    ))
}

pub(super) fn peel<'a>(node: Node<'a>, source: &Source) -> Node<'a> {
    match node.kind() {
        "parenthesized_expression" | "reference_expression" => children(node)
            .last()
            .map_or(node, |child| peel(*child, source)),
        _ => {
            if let Some((name, receiver, args)) = method(node, source)
                && args.is_empty()
                && matches!(name, "as_str" | "as_ref" | "borrow")
            {
                return peel(receiver, source);
            }
            node
        }
    }
}

pub(super) fn literal<'a>(node: Node<'a>, source: &Source) -> Option<(String, Node<'a>)> {
    let node = peel(node, source);
    if matches!(node.kind(), "string_literal" | "raw_string_literal") {
        return syn::parse_str::<syn::LitStr>(text(node, source))
            .ok()
            .map(|value| (value.value(), node));
    }
    if let Some((name, receiver, args)) = method(node, source)
        && args.is_empty()
        && matches!(name, "into" | "to_owned" | "to_string")
    {
        return literal(receiver, source);
    }
    if node.kind() == "call_expression" {
        let function = node.child_by_field_name("function")?;
        if !matches!(
            text(function, source),
            "String::from" | "std::string::String::from" | "alloc::string::String::from"
        ) {
            return None;
        }
        let args = children(node.child_by_field_name("arguments")?);
        if args.len() == 1 {
            return literal(args[0], source);
        }
    }
    None
}

pub(super) fn string_type(ty: &str) -> bool {
    ty == "std:String"
        || ty == "primitive:str"
        || (ty.starts_with('&') && (ty.ends_with("primitive:str") || ty.ends_with("std:String")))
}

pub(super) fn literals<'a>(pattern: Node<'a>, source: &Source) -> Vec<(String, Node<'a>)> {
    if let Some(value) = literal(pattern, source) {
        return vec![value];
    }
    if matches!(
        pattern.kind(),
        "or_pattern" | "match_pattern" | "reference_pattern" | "tuple_pattern"
    ) {
        return children(pattern)
            .into_iter()
            .flat_map(|child| literals(child, source))
            .collect();
    }
    Vec::new()
}

pub(super) fn preserves(pattern: Node<'_>, body: Node<'_>, concept: &str, source: &Source) -> bool {
    if pattern.child_by_field_name("condition").is_some() {
        return false;
    }
    let pattern = if pattern.kind() == "match_pattern" {
        children(pattern).first().copied().unwrap_or(pattern)
    } else {
        pattern
    };
    let name = match pattern.kind() {
        "identifier" => text(pattern, source),
        "_" => concept,
        "match_pattern" if text(pattern, source).trim() == "_" => concept,
        _ => return false,
    };
    preserved(body, name, source)
}

fn direct(node: Node<'_>, name: &str, source: &Source) -> bool {
    let node = peel(node, source);
    if node.kind() == "identifier" {
        return text(node, source) == name;
    }
    if node.kind() == "field_expression" {
        return node
            .child_by_field_name("field")
            .is_some_and(|field| text(field, source) == name);
    }
    if let Some((method, receiver, args)) = method(node, source)
        && args.is_empty()
        && matches!(method, "clone" | "into" | "to_owned" | "to_string")
    {
        return direct(receiver, name, source);
    }
    false
}

fn preserved(node: Node<'_>, name: &str, source: &Source) -> bool {
    if direct(node, name, source) {
        return true;
    }
    if matches!(
        node.kind(),
        "block" | "return_expression" | "expression_statement"
    ) {
        return children(node)
            .into_iter()
            .rfind(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
            .is_some_and(|child| preserved(child, name, source));
    }
    let (constructor, values) = match node.kind() {
        "call_expression" => (
            node.child_by_field_name("function"),
            node.child_by_field_name("arguments")
                .map(children)
                .unwrap_or_default(),
        ),
        "struct_expression" => (
            node.child_by_field_name("name"),
            node.child_by_field_name("body")
                .map(children)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|field| field.child_by_field_name("value"))
                .collect(),
        ),
        _ => return false,
    };
    constructor.is_some_and(|constructor| unknown(constructor, source))
        && values.into_iter().any(|value| direct(value, name, source))
}

fn unknown(node: Node<'_>, source: &Source) -> bool {
    (matches!(node.kind(), "identifier" | "type_identifier")
        && matches!(
            text(node, source),
            "Unknown" | "Unrecognized" | "Other" | "Raw" | "Custom"
        ))
        || children(node)
            .into_iter()
            .any(|child| unknown(child, source))
}

// Only direct return values establish a conversion boundary.
pub(super) fn returned_by<'a>(mut node: Node<'a>, source: &Source) -> Option<Node<'a>> {
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "function_item" => return Some(parent),
            "block"
                if children(parent)
                    .into_iter()
                    .rfind(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
                    != Some(node) =>
            {
                return None;
            }
            "expression_statement" if text(parent, source).trim_end().ends_with(';') => {
                return None;
            }
            "return_expression" => return enclosing_function(parent),
            "block" | "expression_statement" | "parenthesized_expression" => {}
            _ => return None,
        }
        node = parent;
    }
    None
}

fn enclosing_function(mut node: Node<'_>) -> Option<Node<'_>> {
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "function_item" => return Some(parent),
            "closure_expression" | "async_block" => return None,
            _ => node = parent,
        }
    }
    None
}
