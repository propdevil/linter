use super::*;
pub(super) struct Method {
    pub name: String,
    pub path: std::path::PathBuf,
    pub span: Span,
    pub fields: BTreeSet<String>,
    pub calls: BTreeSet<String>,
    pub workflow: bool,
}
pub(super) fn collect(
    analysis: &Analysis,
    index: &Index<'_>,
    masks: &[Vec<bool>],
    scope: Scope,
) -> BTreeMap<String, Vec<Method>> {
    let mut methods = BTreeMap::new();
    for (source, mask) in analysis.sources.iter().zip(masks) {
        implementations(
            source.syntax.root_node(),
            source,
            mask,
            scope,
            index,
            &mut methods,
        );
    }
    methods
}
fn implementations(
    node: Node<'_>,
    source: &Source,
    mask: &[bool],
    selected: Scope,
    index: &Index<'_>,
    methods: &mut BTreeMap<String, Vec<Method>>,
) {
    if let Some((key, values)) = implementation(node, source, mask, selected, index) {
        methods.entry(key).or_default().extend(values);
    }
    for child in children(node) {
        implementations(child, source, mask, selected, index, methods);
    }
}
fn implementation(
    node: Node<'_>,
    source: &Source,
    mask: &[bool],
    selected: Scope,
    index: &Index<'_>,
) -> Option<(String, Vec<Method>)> {
    if node.kind() != "impl_item"
        || node.child_by_field_name("trait").is_some()
        || attributes(node, source, "automatically_derived", None)
        || !scope(mask.get(node.start_byte()) == Some(&true), selected)
    {
        return None;
    }
    let key = index.resolve(
        source,
        node.child_by_field_name("type")?,
        &index.identity(source, node),
    )?;
    let item = index
        .structures
        .iter()
        .find(|item| format!("nominal:{}", item.id.key()) == key)?;
    let known = item.fields.keys().cloned().collect();
    let body = node.child_by_field_name("body")?;
    let values = children(body)
        .into_iter()
        .filter(|node| {
            node.kind() == "function_item"
                && scope(mask.get(node.start_byte()) == Some(&true), selected)
        })
        .filter_map(|node| method(node, source, mask, selected, &known))
        .collect();
    Some((key, values))
}

fn method(
    node: Node<'_>,
    source: &Source,
    mask: &[bool],
    scope: Scope,
    known: &BTreeSet<String>,
) -> Option<Method> {
    let parameters = node.child_by_field_name("parameters")?;
    if !children(parameters)
        .iter()
        .any(|parameter| parameter.kind() == "self_parameter")
    {
        return None;
    }
    let body = node.child_by_field_name("body")?;
    let name = source.text[node.child_by_field_name("name")?.byte_range()].into();
    let mut method = Method {
        name,
        path: source.path.clone(),
        span: Span::new(&source.text, node.byte_range()),
        fields: BTreeSet::new(),
        calls: BTreeSet::new(),
        workflow: false,
    };
    inspect(body, source, mask, scope, known, &mut method);
    Some(method)
}
fn inspect(
    node: Node<'_>,
    source: &Source,
    mask: &[bool],
    selected: Scope,
    known: &BTreeSet<String>,
    method: &mut Method,
) {
    if !scope(mask.get(node.start_byte()) == Some(&true), selected)
        || matches!(
            node.kind(),
            "function_item" | "closure_expression" | "async_block"
        )
    {
        return;
    }
    if let Some(field) = receiver(node, source).filter(|field| known.contains(field)) {
        method.fields.insert(field);
    }
    if node.kind() == "call_expression"
        && let Some(function) = node.child_by_field_name("function")
        && function.kind() == "field_expression"
        && let Some(value) = function.child_by_field_name("value")
        && let Some(field) = receiver(value, source).filter(|field| known.contains(field))
    {
        method.calls.insert(field);
    }
    if matches!(
        node.kind(),
        "assignment_expression"
            | "for_expression"
            | "if_expression"
            | "loop_expression"
            | "match_expression"
            | "while_expression"
    ) {
        method.workflow = true;
    }
    for child in children(node) {
        inspect(child, source, mask, selected, known, method);
    }
}
fn receiver(mut node: Node<'_>, source: &Source) -> Option<String> {
    while matches!(
        node.kind(),
        "reference_expression" | "parenthesized_expression"
    ) {
        node = node
            .child_by_field_name("value")
            .or_else(|| node.named_child(0))?;
    }
    if node.kind() != "field_expression"
        || &source.text[node.child_by_field_name("value")?.byte_range()] != "self"
    {
        return None;
    }
    Some(source.text[node.child_by_field_name("field")?.byte_range()].into())
}
