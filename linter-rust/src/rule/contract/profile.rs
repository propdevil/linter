use super::*;
pub(super) struct Method {
    pub name: String,
    pub range: std::ops::Range<usize>,
    pub types: BTreeSet<String>,
    pub nouns: BTreeSet<String>,
    pub cluster: Option<String>,
}
pub(super) fn methods(
    node: Node<'_>,
    source: &Source,
    mask: &[bool],
    assertion: &Assertion,
) -> Vec<Method> {
    let Some(body) = node.child_by_field_name("body") else {
        return Vec::new();
    };
    children(body)
        .into_iter()
        .filter(|node| {
            matches!(node.kind(), "function_item" | "function_signature_item")
                && included(*node, mask, assertion.scope)
        })
        .filter_map(|node| method(node, source, assertion))
        .collect()
}
fn method(node: Node<'_>, source: &Source, assertion: &Assertion) -> Option<Method> {
    let name = source.text[node.child_by_field_name("name")?.byte_range()].to_owned();
    let mut types = BTreeSet::new();
    if let Some(parameters) = node.child_by_field_name("parameters") {
        for parameter in children(parameters) {
            if let Some(ty) = parameter.child_by_field_name("type") {
                types.extend(words(&source.text[ty.byte_range()]));
            }
        }
    }
    if let Some(ty) = node.child_by_field_name("return_type") {
        types.extend(words(&source.text[ty.byte_range()]));
    }
    types.retain(|word| !assertion.ignored_type_words.contains(word));
    let parts = words(&name);
    let nouns = parts.iter().skip(1).cloned().collect();
    let cluster = parts
        .iter()
        .skip(1)
        .find_map(|noun| {
            assertion
                .capabilities
                .iter()
                .find(|capability| capability.nouns.contains(noun))
        })
        .or_else(|| {
            parts.first().and_then(|verb| {
                assertion
                    .capabilities
                    .iter()
                    .find(|capability| capability.verbs.contains(verb))
            })
        })
        .map(|capability| capability.name.clone());
    Some(Method {
        name,
        range: node.byte_range(),
        types,
        nouns,
        cluster,
    })
}
pub(super) fn separated(clusters: &BTreeMap<String, Vec<&Method>>) -> usize {
    let profiles: Vec<_> = clusters
        .values()
        .map(|methods| {
            (
                methods
                    .iter()
                    .flat_map(|method| method.types.iter().cloned())
                    .collect::<BTreeSet<_>>(),
                methods
                    .iter()
                    .flat_map(|method| method.nouns.iter().cloned())
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect();
    let mut separated = BTreeSet::new();
    for (left, a) in profiles.iter().enumerate() {
        for (right, b) in profiles.iter().enumerate().skip(left + 1) {
            if a.0 != b.0 || (!a.1.is_empty() && !b.1.is_empty() && a.1.is_disjoint(&b.1)) {
                separated.insert(left);
                separated.insert(right);
            }
        }
    }
    separated.len()
}
pub(super) fn cohesive(name: &str, methods: &[Method], assertion: &Assertion) -> bool {
    if !assertion
        .cohesive_suffixes
        .iter()
        .any(|suffix| name.ends_with(suffix))
    {
        return false;
    }
    let mut occurrences = BTreeMap::new();
    for method in methods {
        for ty in &method.types {
            *occurrences.entry(ty).or_insert(0usize) += 1;
        }
    }
    occurrences
        .values()
        .any(|count| *count >= methods.len() - methods.len() / 4)
}
fn words(text: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut current = String::new();
    let mut previous = None;
    for ch in text.chars() {
        let boundary = !ch.is_ascii_alphanumeric()
            || (ch.is_ascii_uppercase()
                && previous.is_some_and(|ch: char| ch.is_ascii_lowercase()));
        if boundary && !current.is_empty() {
            output.push(std::mem::take(&mut current));
        }
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        }
        previous = Some(ch);
    }
    if !current.is_empty() {
        output.push(current);
    }
    output
}
