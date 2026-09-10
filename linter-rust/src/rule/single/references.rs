use super::*;
pub(super) fn collect(
    candidate: &Declaration<'_>,
    functions: &[Declaration<'_>],
    analysis: &Analysis,
    index: &Index<'_>,
    masks: &[Vec<bool>],
    scope: Scope,
) -> (Vec<Evidence>, bool) {
    let mut evidence = Vec::new();
    let mut uncertain = false;
    for (source, mask) in analysis.sources.iter().zip(masks) {
        uncertain |= unresolved(
            source.syntax.root_node(),
            source,
            (mask, scope),
            (candidate, functions, index),
        );
        uncertain |= ambiguity(source.syntax.root_node(), source, mask, scope, candidate);
        visit(
            source.syntax.root_node(),
            source,
            (mask, scope),
            candidate,
            functions,
            index,
            &mut evidence,
        );
    }
    (evidence, uncertain)
}
fn visit(
    node: Node<'_>,
    source: &Source,
    selection: (&[bool], Scope),
    candidate: &Declaration<'_>,
    functions: &[Declaration<'_>],
    index: &Index<'_>,
    evidence: &mut Vec<Evidence>,
) {
    if source.path == candidate.source.path && node == candidate.node {
        return;
    }
    let (mask, scope) = selection;
    if matches!(scope, Scope::Production) && !included(node, mask, scope) {
        return;
    }
    if included(node, mask, scope)
        && reference(node)
        && resolves(
            &source.text[node.byte_range()],
            node,
            source,
            candidate,
            functions,
            index,
        )
    {
        evidence.push(location(source, node, "resolved function reference"));
    }
    if node.kind() == "attribute_item" && included(node, mask, scope) {
        hooks(node, source, candidate, functions, index, evidence);
        return;
    }
    for child in children(node) {
        visit(
            child, source, selection, candidate, functions, index, evidence,
        );
    }
}
fn reference(node: Node<'_>) -> bool {
    if !matches!(node.kind(), "identifier" | "scoped_identifier") {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    if matches!(
        parent.kind(),
        "scoped_identifier"
            | "use_declaration"
            | "use_as_clause"
            | "use_list"
            | "scoped_use_list"
            | "token_tree"
    ) {
        return false;
    }
    if parent.child_by_field_name("name") == Some(node)
        || parent.child_by_field_name("pattern") == Some(node)
        || parent.child_by_field_name("field") == Some(node)
    {
        return false;
    }
    matches!(
        parent.kind(),
        "call_expression"
            | "arguments"
            | "return_expression"
            | "let_declaration"
            | "block"
            | "expression_statement"
            | "tuple_expression"
            | "array_expression"
            | "reference_expression"
    )
}
fn resolves(
    path: &str,
    node: Node<'_>,
    source: &Source,
    candidate: &Declaration<'_>,
    functions: &[Declaration<'_>],
    index: &Index<'_>,
) -> bool {
    let mut owner = index.identity(source, node);
    if owner.package != candidate.id.package {
        return false;
    }
    let mut parts: Vec<_> = path.split("::").collect();
    let Some(name) = parts.pop() else {
        return false;
    };
    if name != candidate.id.name {
        return false;
    }
    if parts.is_empty() {
        if shadowed(name, node, source) {
            return false;
        }
        loop {
            let matching: Vec<_> = functions
                .iter()
                .filter(|function| {
                    function.id.package == owner.package
                        && function.id.module == owner.module
                        && function.id.name == name
                })
                .collect();
            if !matching.is_empty() {
                return matching.len() == 1 && matching[0].id == candidate.id;
            }
            if !owner
                .module
                .last()
                .is_some_and(|part| part.starts_with('@'))
            {
                return false;
            }
            owner.module.pop();
        }
    }
    while owner
        .module
        .last()
        .is_some_and(|part| part.starts_with('@'))
    {
        owner.module.pop();
    }
    match parts.first().copied() {
        Some("crate") => {
            owner.module.clear();
            parts.remove(0);
        }
        Some("self") => {
            parts.remove(0);
        }
        Some("super") => {
            while parts.first() == Some(&"super") {
                owner.module.pop();
                parts.remove(0);
            }
        }
        _ => {}
    }
    owner.module.extend(parts.into_iter().map(str::to_owned));
    owner.name = name.into();
    let matching: Vec<_> = functions
        .iter()
        .filter(|function| function.id == owner)
        .collect();
    matching.len() == 1 && owner == candidate.id
}
fn shadowed(name: &str, mut node: Node<'_>, source: &Source) -> bool {
    while let Some(parent) = node.parent() {
        if matches!(parent.kind(), "function_item" | "closure_expression")
            && parent
                .child_by_field_name("parameters")
                .is_some_and(|parameters| {
                    children(parameters).iter().any(|parameter| {
                        parameter
                            .child_by_field_name("pattern")
                            .is_some_and(|pattern| binds(pattern, name, source))
                    })
                })
        {
            return true;
        }
        if parent.kind() == "block"
            && children(parent).iter().any(|statement| {
                statement.start_byte() < node.start_byte()
                    && statement.kind() == "let_declaration"
                    && statement
                        .child_by_field_name("pattern")
                        .is_some_and(|pattern| binds(pattern, name, source))
            })
        {
            return true;
        }
        node = parent;
    }
    false
}
fn binds(node: Node<'_>, name: &str, source: &Source) -> bool {
    (node.kind() == "identifier" && &source.text[node.byte_range()] == name)
        || children(node)
            .into_iter()
            .any(|child| binds(child, name, source))
}
fn hooks(
    node: Node<'_>,
    source: &Source,
    candidate: &Declaration<'_>,
    functions: &[Declaration<'_>],
    index: &Index<'_>,
    evidence: &mut Vec<Evidence>,
) {
    let Some(meta) = node.named_child(0) else {
        return;
    };
    let Some(name) = meta.named_child(0) else {
        return;
    };
    if &source.text[name.byte_range()] != "serde" {
        return;
    }
    let Some(arguments) = meta.child_by_field_name("arguments") else {
        return;
    };
    for assignment in source.text[arguments.byte_range()]
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(',')
    {
        let Some((key, value)) = assignment.split_once('=') else {
            continue;
        };
        if !matches!(key.trim(), "deserialize_with" | "serialize_with") {
            continue;
        }
        let value = value.trim();
        if !value.starts_with('"') || !value.ends_with('"') {
            continue;
        }
        if resolves(
            &value[1..value.len() - 1],
            node,
            source,
            candidate,
            functions,
            index,
        ) {
            evidence.push(location(source, node, "resolved serde function hook"));
        }
    }
}
fn location(source: &Source, node: Node<'_>, message: &str) -> Evidence {
    Evidence {
        path: source.path.clone(),
        span: Some(Span::new(&source.text, node.byte_range())),
        message: message.into(),
    }
}

fn ambiguity(
    node: Node<'_>,
    source: &Source,
    mask: &[bool],
    scope: Scope,
    candidate: &Declaration<'_>,
) -> bool {
    if matches!(scope, Scope::Production) && !included(node, mask, scope) {
        return false;
    }
    let text = &source.text[node.byte_range()];
    let mentions = text
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .any(|word| word == candidate.id.name);
    if included(node, mask, scope)
        && ((matches!(
            node.kind(),
            "macro_invocation" | "macro_definition" | "attribute_item"
        ) && mentions)
            || (node.kind() == "use_declaration" && (mentions || text.contains('*')))
            || (matches!(
                node.kind(),
                "closure_expression"
                    | "for_expression"
                    | "match_arm"
                    | "let_condition"
                    | "generic_function"
            ) && mentions))
    {
        return true;
    }
    children(node)
        .into_iter()
        .any(|child| ambiguity(child, source, mask, scope, candidate))
}

fn unresolved(
    node: Node<'_>,
    source: &Source,
    selection: (&[bool], Scope),
    context: (&Declaration<'_>, &[Declaration<'_>], &Index<'_>),
) -> bool {
    let (mask, scope) = selection;
    if matches!(scope, Scope::Production) && !included(node, mask, scope) {
        return false;
    }
    let (candidate, functions, index) = context;
    let text = &source.text[node.byte_range()];
    if included(node, mask, scope)
        && reference(node)
        && text.split("::").last() == Some(candidate.id.name.as_str())
        && !shadowed(&candidate.id.name, node, source)
        && !functions
            .iter()
            .any(|function| resolves(text, node, source, function, functions, index))
    {
        return true;
    }
    if node.kind() == "identifier" && text == candidate.id.name {
        let parent = node.parent().unwrap_or(node);
        if matches!(
            parent.kind(),
            "binary_expression"
                | "assignment_expression"
                | "compound_assignment_expr"
                | "field_initializer"
                | "shorthand_field_initializer"
                | "cast_expression"
                | "type_cast_expression"
                | "match_arm"
        ) {
            return true;
        }
    }
    children(node)
        .into_iter()
        .any(|child| unresolved(child, source, selection, context))
}
