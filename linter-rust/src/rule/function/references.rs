use super::*;
pub(super) fn collect(
    candidate: &Declaration<'_>,
    functions: &[Declaration<'_>],
    analysis: &Analysis,
    index: &Index<'_>,
    masks: &[Vec<bool>],
    scope: Scope,
) -> Vec<Evidence> {
    let mut evidence = Vec::new();
    for (source, mask) in analysis.sources.iter().zip(masks) {
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
    evidence
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
    let mut parts: Vec<_> = path.split("::").collect();
    let name = parts.pop().unwrap_or_default();
    if owner.package != candidate.id.package || name != candidate.id.name {
        return false;
    }
    if parts.is_empty() {
        return !shadowed(name, node, source) && candidate.lexical(owner, name, functions);
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
    functions
        .iter()
        .filter(|function| function.id == owner)
        .count()
        == 1
        && owner == candidate.id
}
impl Declaration<'_> {
    fn lexical(&self, mut owner: Identity, name: &str, functions: &[Declaration<'_>]) -> bool {
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
                return matching.len() == 1 && matching[0].id == self.id;
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
}
fn shadowed(name: &str, mut node: Node<'_>, source: &Source) -> bool {
    while let Some(parent) = node.parent() {
        let parameters = parent
            .child_by_field_name("parameters")
            .map(children)
            .unwrap_or_default();
        let parameter_binding = parameters
            .iter()
            .filter_map(|parameter| parameter.child_by_field_name("pattern"))
            .any(|pattern| binds(pattern, name, source));
        if matches!(parent.kind(), "function_item" | "closure_expression") && parameter_binding {
            return true;
        }
        let local_binding = children(parent)
            .into_iter()
            .filter(|statement| {
                statement.start_byte() < node.start_byte() && statement.kind() == "let_declaration"
            })
            .filter_map(|statement| statement.child_by_field_name("pattern"))
            .any(|pattern| binds(pattern, name, source));
        if parent.kind() == "block" && local_binding {
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
