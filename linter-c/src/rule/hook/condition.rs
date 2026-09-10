use std::collections::BTreeSet;
use tree_sitter::Node;

pub(super) fn production(node: Node<'_>, source: &[u8], macros: &BTreeSet<String>) -> Option<bool> {
    if let Some(name) = node.child_by_field_name("name") {
        if !macros.contains(name.utf8_text(source).ok()?) {
            return None;
        }
        let directive = node.child(0)?.utf8_text(source).ok()?;
        return Some(matches!(directive, "#ifndef" | "#elifndef"));
    }
    evaluate(node.child_by_field_name("condition")?, source, macros)
}
fn evaluate(node: Node<'_>, source: &[u8], macros: &BTreeSet<String>) -> Option<bool> {
    let text = node.utf8_text(source).ok()?;
    match node.kind() {
        "parenthesized_expression" => evaluate(node.named_child(0)?, source, macros),
        "preproc_defined" => macros
            .contains(node.named_child(0)?.utf8_text(source).ok()?)
            .then_some(false),
        "identifier" => macros.contains(text).then_some(false),
        "number_literal" => match text {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        },
        "unary_expression"
            if node
                .child_by_field_name("operator")?
                .utf8_text(source)
                .ok()?
                == "!" =>
        {
            evaluate(node.child_by_field_name("argument")?, source, macros).map(|value| !value)
        }
        "binary_expression" => {
            let left = evaluate(node.child_by_field_name("left")?, source, macros);
            let right = evaluate(node.child_by_field_name("right")?, source, macros);
            match node
                .child_by_field_name("operator")?
                .utf8_text(source)
                .ok()?
            {
                "&&" => {
                    if left == Some(false) || right == Some(false) {
                        Some(false)
                    } else if left == Some(true) && right == Some(true) {
                        Some(true)
                    } else {
                        None
                    }
                }
                "||" => {
                    if left == Some(true) || right == Some(true) {
                        Some(true)
                    } else if left == Some(false) && right == Some(false) {
                        Some(false)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}
