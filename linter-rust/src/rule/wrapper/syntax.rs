use crate::{Source, declaration::Index};
use tree_sitter::Node;
pub(super) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
        .collect()
}
pub(super) fn descendants(node: Node<'_>) -> Vec<Node<'_>> {
    let mut out = vec![node];
    for child in children(node) {
        out.extend(descendants(child));
    }
    out
}
pub(super) fn text<'a>(node: Node<'_>, source: &'a Source) -> &'a str {
    &source.text[node.byte_range()]
}
pub(super) fn compact(node: Node<'_>, source: &Source) -> String {
    text(node, source).split_whitespace().collect()
}
pub(super) fn boundary(node: Node<'_>, source: &Source) -> bool {
    let mut previous = node.prev_named_sibling();
    while let Some(item) = previous {
        if item.kind() == "attribute_item" {
            let value = compact(item, source);
            let Some(derives) = value
                .strip_prefix("#[derive(")
                .and_then(|s| s.strip_suffix(")]"))
            else {
                return true;
            };
            if derives.split(',').filter(|s| !s.is_empty()).any(|s| {
                !matches!(
                    s,
                    "Debug"
                        | "Clone"
                        | "Copy"
                        | "Eq"
                        | "PartialEq"
                        | "Ord"
                        | "PartialOrd"
                        | "Hash"
                        | "Default"
                )
            }) {
                return true;
            }
        } else if !matches!(item.kind(), "line_comment" | "block_comment") {
            break;
        }
        previous = item.prev_named_sibling();
    }
    false
}
pub(super) fn visibility(node: Node<'_>, source: &Source) -> String {
    children(node)
        .into_iter()
        .find(|n| n.kind() == "visibility_modifier")
        .map(|n| compact(n, source))
        .unwrap_or_default()
}
fn expression(node: Node<'_>) -> Option<Node<'_>> {
    let mut node = node.child_by_field_name("body")?;
    let body = children(node);
    if body.len() != 1 {
        return None;
    }
    node = body[0];
    loop {
        if matches!(
            node.kind(),
            "parenthesized_expression" | "expression_statement"
        ) {
            let parts = children(node);
            if parts.len() != 1 {
                return None;
            }
            node = parts[0];
        } else {
            return Some(node);
        }
    }
}
fn parameters(node: Node<'_>, source: &Source) -> Option<Vec<String>> {
    children(node.child_by_field_name("parameters")?)
        .into_iter()
        .filter(|n| n.kind() != "self_parameter")
        .map(|n| {
            let pattern = n.child_by_field_name("pattern")?;
            (pattern.kind() == "identifier").then(|| text(pattern, source).to_owned())
        })
        .collect()
}
pub(super) fn signature(node: Node<'_>, source: &Source, index: &Index<'_>) -> Option<String> {
    if node.child_by_field_name("type_parameters").is_some()
        || children(node).iter().any(|n| n.kind() == "where_clause")
    {
        return None;
    }
    let owner = index.identity(source, node.parent()?.parent()?);
    let params = children(node.child_by_field_name("parameters")?);
    let receiver = params.first()?;
    if receiver.kind() != "self_parameter" {
        return None;
    }
    let inputs = params
        .iter()
        .skip(1)
        .map(|param| index.resolve(source, param.child_by_field_name("type")?, &owner))
        .collect::<Option<Vec<_>>>()?;
    let output = node
        .child_by_field_name("return_type")
        .map(|ty| index.resolve(source, ty, &owner))
        .unwrap_or(Some("()".into()))?;
    let name = node.child_by_field_name("name")?;
    let modifiers = &source.text[node.start_byte()..name.start_byte()];
    let modes: Vec<_> = modifiers
        .split_whitespace()
        .filter(|word| matches!(*word, "async" | "unsafe" | "const" | "extern"))
        .collect();
    if modes
        .iter()
        .any(|word| matches!(*word, "async" | "unsafe" | "extern"))
    {
        return None;
    }
    Some(format!(
        "{}|{:?}|{output}|{modes:?}|{}",
        compact(*receiver, source),
        inputs,
        visibility(node, source)
    ))
}
pub(super) fn forwards(node: Node<'_>, source: &Source, field: &str) -> bool {
    let Some(call) = expression(node) else {
        return false;
    };
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let Some(receiver) = function.child_by_field_name("value") else {
        return false;
    };
    if compact(receiver, source) != format!("self.{field}") {
        return false;
    }
    let Some(method) = function.child_by_field_name("field") else {
        return false;
    };
    if node
        .child_by_field_name("name")
        .is_none_or(|name| text(name, source) != text(method, source))
    {
        return false;
    }
    let Some(parameters) = parameters(node, source) else {
        return false;
    };
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    let arguments = children(arguments);
    arguments.len() == parameters.len()
        && arguments
            .iter()
            .zip(parameters)
            .all(|(arg, name)| arg.kind() == "identifier" && text(*arg, source) == name)
}
pub(super) fn constructor(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    field: &str,
    inner: &str,
) -> bool {
    if node.child_by_field_name("type_parameters").is_some()
        || children(node).iter().any(|n| n.kind() == "where_clause")
        || node.child_by_field_name("name").is_some_and(|name| {
            source.text[node.start_byte()..name.start_byte()]
                .split_whitespace()
                .any(|word| matches!(word, "async" | "unsafe" | "extern"))
        })
    {
        return false;
    }
    let Some(params) = node.child_by_field_name("parameters") else {
        return false;
    };
    let params = children(params);
    if params.len() != 1 || params[0].kind() != "parameter" {
        return false;
    }
    let Some(ty) = params[0].child_by_field_name("type") else {
        return false;
    };
    let context = index.identity(
        source,
        node.parent().and_then(|n| n.parent()).unwrap_or(node),
    );
    if index.resolve(source, ty, &context).as_deref() != Some(inner) {
        return false;
    }
    let Some(names) = parameters(node, source) else {
        return false;
    };
    let Some(name) = names.first() else {
        return false;
    };
    let Some(body) = expression(node) else {
        return false;
    };
    let Some(output) = node.child_by_field_name("return_type") else {
        return false;
    };
    text(output, source) == "Self"
        && (compact(body, source).replace(",}", "}") == format!("Self{{{field}:{name}}}")
            || field == name
                && compact(body, source).replace(",}", "}") == format!("Self{{{field}}}"))
}
