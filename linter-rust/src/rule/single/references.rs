use super::*;
use std::collections::BTreeSet;

pub(super) struct References {
    uses: Vec<Vec<(bool, Evidence)>>,
    uncertain: BTreeMap<String, [bool; 2]>,
    wildcard: [bool; 2],
}
impl References {
    pub fn new(
        functions: &[Declaration<'_>],
        analysis: &Analysis,
        index: &Index<'_>,
        masks: &[Vec<bool>],
    ) -> Self {
        let mut result = Self {
            uses: vec![Vec::new(); functions.len()],
            uncertain: BTreeMap::new(),
            wildcard: [false; 2],
        };
        let mut names = BTreeSet::new();
        let mut identities: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (position, function) in functions.iter().enumerate() {
            names.insert(function.id.name.as_str());
            identities
                .entry(function.id.key())
                .or_default()
                .push(position);
        }
        for (source, mask) in analysis.sources.iter().zip(masks) {
            let mut walker = Walker {
                functions,
                index,
                source,
                mask,
                names: &names,
                identities: &identities,
                result: &mut result,
            };
            walker.visit(source.syntax.root_node());
        }
        result
    }
    pub fn get(&self, position: usize, name: &str, scope: Scope) -> (Vec<Evidence>, bool) {
        let selected = |test: bool| match scope {
            Scope::Production => !test,
            Scope::Tests => test,
            Scope::All => true,
        };
        let evidence = self.uses[position]
            .iter()
            .filter(|(test, _)| selected(*test))
            .map(|(_, evidence)| evidence.clone())
            .collect();
        let flags = self.uncertain.get(name).copied().unwrap_or([false; 2]);
        let uncertain =
            (0..2).any(|test| selected(test == 1) && (flags[test] || self.wildcard[test]));
        (evidence, uncertain)
    }
}
struct Walker<'a, 'b> {
    functions: &'a [Declaration<'b>],
    index: &'a Index<'b>,
    source: &'b Source,
    mask: &'a [bool],
    names: &'a BTreeSet<&'b str>,
    identities: &'a BTreeMap<String, Vec<usize>>,
    result: &'a mut References,
}
impl Walker<'_, '_> {
    fn visit(&mut self, node: Node<'_>) {
        let test = self.mask.get(node.start_byte()) == Some(&true);
        let text = &self.source.text[node.byte_range()];
        self.ambiguity(node, text, test);
        if reference(node)
            && let Some(name) = text.split("::").last()
            && self.names.contains(name)
        {
            if let Some(position) = self.resolve(text, node) {
                let candidate = &self.functions[position];
                let recursive = candidate.source.path == self.source.path
                    && candidate.node.start_byte() <= node.start_byte()
                    && candidate.node.end_byte() >= node.end_byte();
                if !recursive {
                    self.result.uses[position].push((
                        test,
                        Evidence {
                            path: self.source.path.clone(),
                            span: Some(Span::new(&self.source.text, node.byte_range())),
                            message: "resolved function reference".into(),
                        },
                    ));
                }
            } else if !shadowed(name, node, self.source) {
                self.result.uncertain.entry(name.into()).or_default()[usize::from(test)] = true;
            }
        }
        for child in children(node) {
            self.visit(child);
        }
    }
    fn ambiguity(&mut self, node: Node<'_>, text: &str, test: bool) {
        let opaque = matches!(
            node.kind(),
            "macro_invocation"
                | "macro_definition"
                | "attribute_item"
                | "use_declaration"
                | "closure_expression"
                | "for_expression"
                | "match_arm"
                | "let_condition"
                | "generic_function"
        );
        let unsupported = node.kind() == "identifier"
            && node.parent().is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    "binary_expression"
                        | "assignment_expression"
                        | "compound_assignment_expr"
                        | "field_initializer"
                        | "shorthand_field_initializer"
                        | "cast_expression"
                        | "type_cast_expression"
                        | "match_arm"
                )
            });
        if node.kind() == "use_declaration" && text.contains('*') {
            self.result.wildcard[usize::from(test)] = true;
        }
        if opaque || unsupported {
            for word in text
                .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                .filter(|word| self.names.contains(word))
            {
                self.result.uncertain.entry(word.into()).or_default()[usize::from(test)] = true;
            }
        }
    }
    fn resolve(&self, path: &str, node: Node<'_>) -> Option<usize> {
        let mut owner = self.index.identity(self.source, node);
        let mut parts: Vec<_> = path.split("::").collect();
        let name = parts.pop()?;
        owner.name = name.into();
        if parts.is_empty() {
            if shadowed(name, node, self.source) {
                return None;
            }
            loop {
                if let Some(matching) = self.identities.get(&owner.key()) {
                    return (matching.len() == 1).then_some(matching[0]);
                }
                if !owner
                    .module
                    .last()
                    .is_some_and(|part| part.starts_with('@'))
                {
                    return None;
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
        self.identities
            .get(&owner.key())
            .filter(|matching| matching.len() == 1)
            .map(|matching| matching[0])
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
