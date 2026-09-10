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
    let parameters = node
        .child_by_field_name("parameters")
        .into_iter()
        .flat_map(children);
    for ty in parameters.filter_map(|parameter| parameter.child_by_field_name("type")) {
        types.extend(words(&source.text[ty.byte_range()]));
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
        let distinct = profiles.iter().enumerate().skip(left + 1).filter(|(_, b)| {
            a.0 != b.0 || (!a.1.is_empty() && !b.1.is_empty() && a.1.is_disjoint(&b.1))
        });
        for (right, _) in distinct {
            separated.insert(left);
            separated.insert(right);
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

impl Method {
    pub fn evidence(&self, source: &Source) -> Evidence {
        Evidence {
            path: source.path.clone(),
            span: Some(Span::new(&source.text, self.range.clone())),
            message: format!(
                "method `{}`; signature types: {}",
                self.name,
                self.types.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

pub(super) struct Clusters<'a>(BTreeMap<String, Vec<&'a Method>>);
impl<'a> Clusters<'a> {
    pub fn new(methods: &'a [Method], minimum: usize) -> Self {
        let mut groups = BTreeMap::<String, Vec<&Method>>::new();
        for (name, method) in methods
            .iter()
            .filter_map(|method| method.cluster.as_ref().map(|name| (name, method)))
        {
            groups.entry(name.clone()).or_default().push(method);
        }
        groups.retain(|_, methods| methods.len() >= minimum);
        Self(groups)
    }
    pub fn count(&self) -> usize {
        self.0.len()
    }
    pub fn method_count(&self) -> usize {
        self.0.values().map(Vec::len).sum()
    }
    pub fn separated(&self) -> usize {
        separated(&self.0)
    }
    pub fn summary(&self) -> String {
        self.0
            .iter()
            .map(|(cluster, methods)| {
                let names = methods
                    .iter()
                    .map(|method| method.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{cluster}: {names}")
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}
