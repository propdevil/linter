use super::*;
#[derive(Default)]
pub(super) struct Observation {
    pub literals: Vec<Construction>,
    pub evidence: Vec<(BTreeSet<String>, Evidence)>,
}
pub(super) struct Construction {
    pub values: BTreeMap<String, bool>,
    pub evidence: Evidence,
}

pub(super) fn inspect(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    minimum: usize,
    observations: &mut BTreeMap<String, Observation>,
    tests: &[bool],
    scope: Scope,
) {
    let mask = Mask { tests, scope };
    match node.kind() {
        "struct_expression" => {
            if let Some((key, literal)) = construction(node, source, index) {
                observations.entry(key).or_default().literals.push(literal);
            }
        }
        "function_item" => {
            let Some((key, fields)) = receiver(node, source, index) else {
                return;
            };
            let Some(body) = node.child_by_field_name("body") else {
                return;
            };
            let name = node
                .child_by_field_name("name")
                .map(|name| &source.text[name.byte_range()])
                .unwrap_or_default();
            let observation = observations.entry(key).or_default();
            transitions(
                body,
                source,
                &fields,
                minimum,
                name,
                &mut observation.evidence,
                &mask,
            );
            let mut excluded = BTreeSet::new();
            exclusions(body, source, &fields, &mut excluded, &mask);
            let implicated: BTreeSet<_> = excluded
                .iter()
                .flat_map(|(a, b)| [a.clone(), b.clone()])
                .collect();
            if implicated.len() >= minimum && excluded.len() >= minimum - 1 {
                observation.evidence.push((
                    implicated,
                    evidence(
                        source,
                        node,
                        format!("method `{name}` rejects mutually active boolean fields"),
                    ),
                ));
            }
        }
        _ => {}
    }
}
fn construction(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
) -> Option<(String, Construction)> {
    let name = node.child_by_field_name("name")?;
    let context = index.identity(source, node);
    let key = if &source.text[name.byte_range()] == "Self" {
        enclosing_impl(node).and_then(|implementation| {
            index.resolve(
                source,
                implementation.child_by_field_name("type")?,
                &index.identity(source, implementation),
            )
        })?
    } else {
        index.resolve(source, name, &context)?
    };
    let body = node.child_by_field_name("body")?;
    let values = children(body)
        .into_iter()
        .filter_map(|field| {
            let name = field.child_by_field_name("field")?;
            let value = literal(field.child_by_field_name("value")?, source)?;
            Some((source.text[name.byte_range()].into(), value))
        })
        .collect();
    Some((
        key,
        Construction {
            values,
            evidence: evidence(
                source,
                node,
                "one-hot construction of the same boolean field set".into(),
            ),
        },
    ))
}
fn receiver(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
) -> Option<(String, BTreeSet<String>)> {
    let parent = node.parent()?;
    if parent.kind() != "declaration_list" {
        return None;
    }
    let implementation = parent.parent()?;
    if implementation.kind() != "impl_item" || implementation.child_by_field_name("trait").is_some()
    {
        return None;
    }
    let key = index.resolve(
        source,
        implementation.child_by_field_name("type")?,
        &index.identity(source, implementation),
    )?;
    let item = index
        .structures
        .iter()
        .find(|item| format!("nominal:{}", item.id.key()) == key)?;
    Some((key, bool_fields(item)))
}
fn enclosing_impl(mut node: Node<'_>) -> Option<Node<'_>> {
    while let Some(parent) = node.parent() {
        if parent.kind() == "impl_item" {
            return Some(parent);
        }
        node = parent;
    }
    None
}
fn transitions(
    node: Node<'_>,
    source: &Source,
    fields: &BTreeSet<String>,
    minimum: usize,
    name: &str,
    evidence_set: &mut Vec<(BTreeSet<String>, Evidence)>,
    mask: &Mask<'_>,
) {
    if node.kind() != "block" {
        return;
    }
    let mut values = BTreeMap::new();
    for statement in children(node)
        .into_iter()
        .filter(|node| mask.includes(*node))
    {
        let expr = if statement.kind() == "expression_statement" {
            statement.named_child(0).unwrap_or(statement)
        } else {
            statement
        };
        if let Some((field, value)) =
            assignment(expr, source).filter(|(field, _)| fields.contains(field))
        {
            values.insert(field, value);
        } else {
            record(&values, source, node, minimum, name, evidence_set);
            values.clear();
            nested_blocks(expr, source, fields, minimum, name, evidence_set, mask);
        }
    }
    record(&values, source, node, minimum, name, evidence_set);
}
fn nested_blocks(
    node: Node<'_>,
    source: &Source,
    fields: &BTreeSet<String>,
    minimum: usize,
    name: &str,
    evidence_set: &mut Vec<(BTreeSet<String>, Evidence)>,
    mask: &Mask<'_>,
) {
    if !mask.includes(node) {
        return;
    }
    if matches!(
        node.kind(),
        "function_item" | "closure_expression" | "async_block"
    ) {
        return;
    }
    if node.kind() == "block" {
        transitions(node, source, fields, minimum, name, evidence_set, mask);
        return;
    }
    for child in children(node) {
        nested_blocks(child, source, fields, minimum, name, evidence_set, mask);
    }
}
fn record(
    values: &BTreeMap<String, bool>,
    source: &Source,
    node: Node<'_>,
    minimum: usize,
    name: &str,
    evidence_set: &mut Vec<(BTreeSet<String>, Evidence)>,
) {
    let fields: BTreeSet<_> = values.keys().cloned().collect();
    if fields.len() >= minimum && one_hot(values, &fields).is_some() {
        evidence_set.push((
            fields,
            evidence(
                source,
                node,
                format!(
                    "method `{name}` coordinates {} boolean state fields",
                    values.len()
                ),
            ),
        ));
    }
}
fn assignment(node: Node<'_>, source: &Source) -> Option<(String, bool)> {
    if node.kind() != "assignment_expression" {
        return None;
    }
    Some((
        self_field(node.child_by_field_name("left")?, source)?,
        literal(node.child_by_field_name("right")?, source)?,
    ))
}
fn literal(node: Node<'_>, source: &Source) -> Option<bool> {
    match &source.text[node.byte_range()] {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
fn self_field(node: Node<'_>, source: &Source) -> Option<String> {
    if node.kind() != "field_expression"
        || &source.text[node.child_by_field_name("value")?.byte_range()] != "self"
    {
        return None;
    }
    Some(source.text[node.child_by_field_name("field")?.byte_range()].into())
}
fn exclusions(
    node: Node<'_>,
    source: &Source,
    fields: &BTreeSet<String>,
    pairs: &mut BTreeSet<(String, String)>,
    mask: &Mask<'_>,
) {
    if !mask.includes(node) {
        return;
    }
    if matches!(
        node.kind(),
        "function_item" | "closure_expression" | "async_block"
    ) {
        return;
    }
    if node.kind() == "unary_expression"
        && source.text[node.byte_range()].starts_with('!')
        && let Some(child) = node.named_child(0)
    {
        rejected(child, source, fields, pairs);
        return;
    }
    for child in children(node) {
        exclusions(child, source, fields, pairs, mask);
    }
}
fn rejected(
    mut node: Node<'_>,
    source: &Source,
    fields: &BTreeSet<String>,
    pairs: &mut BTreeSet<(String, String)>,
) {
    while node.kind() == "parenthesized_expression" {
        let Some(child) = node.named_child(0) else {
            return;
        };
        node = child;
    }
    if node.kind() != "binary_expression"
        || node
            .child_by_field_name("operator")
            .is_none_or(|operator| &source.text[operator.byte_range()] != "&&")
    {
        return;
    }
    let Some((left, right)) = node
        .child_by_field_name("left")
        .zip(node.child_by_field_name("right"))
    else {
        return;
    };
    if let Some((left, right)) = self_field(left, source).zip(self_field(right, source))
        && left != right
        && fields.contains(&left)
        && fields.contains(&right)
    {
        pairs.insert(if left < right {
            (left, right)
        } else {
            (right, left)
        });
    }
}

struct Mask<'a> {
    tests: &'a [bool],
    scope: Scope,
}
impl Mask<'_> {
    fn includes(&self, node: Node<'_>) -> bool {
        super::scope(self.tests.get(node.start_byte()) == Some(&true), self.scope)
    }
}
