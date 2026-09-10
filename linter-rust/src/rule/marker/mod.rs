use crate::{
    Analysis, Source,
    declaration::Index,
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;
mod config;
pub use config::Config;
pub struct RedundantMarker(Vec<Assertion>);
impl Rule for RedundantMarker {
    const ID: &'static str = "rust/redundant-marker";
    type Analysis = Analysis;
    type Config = Config;
    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self(config.compile()?))
    }
    fn configured(&self) -> bool {
        !self.0.is_empty()
    }
    fn check(&self, project: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let root =
            std::fs::canonicalize(project.root()).map_err(|e| Error::Analysis(e.to_string()))?;
        let index = Index::new(analysis, &root);
        let mut candidates = BTreeMap::<String, Vec<(&Source, Node<'_>, bool)>>::new();
        let mut used = BTreeSet::new();
        let mut opaque = BTreeSet::new();
        let mut implementations = BTreeMap::<String, Vec<Evidence>>::new();
        for source in &analysis.sources {
            let mut tests = vec![false; source.text.len()];
            if integration(source, &root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            for node in descendants(source.syntax.root_node()) {
                let owner = index.identity(source, node);
                if node.kind() == "macro_invocation" {
                    for word in text(node, source).split(|c: char| !c.is_alphanumeric() && c != '_')
                    {
                        opaque.insert((owner.package.clone(), word.to_owned()));
                    }
                }
                if node.kind() == "trait_item"
                    && candidate(node, source)
                    && let Some(name) = node.child_by_field_name("name")
                    && let Some(id) = index.resolve(source, name, &owner)
                {
                    candidates.entry(id).or_default().push((
                        source,
                        node,
                        tests.get(node.start_byte()).copied().unwrap_or(false),
                    ));
                }
                if node.kind() == "impl_item"
                    && let Some(trait_node) = node.child_by_field_name("trait")
                    && let Some(id) = index.resolve(source, trait_node, &owner)
                {
                    if blanket(node, source) {
                        implementations.entry(id).or_default().push(Evidence {
                            path: source.path.clone(),
                            span: Some(Span::new(&source.text, node.byte_range())),
                            message: "Unconstrained blanket implementation".into(),
                        });
                    } else {
                        used.insert(id);
                    }
                }
                if matches!(node.kind(), "type_identifier" | "scoped_type_identifier")
                    && !definition_name(node)
                    && let Some(id) = index.resolve(source, node, &owner)
                {
                    used.insert(id);
                }
            }
        }
        let mut findings = Vec::new();
        for (id, definitions) in candidates {
            if definitions.len() != 1 || used.contains(&id) {
                continue;
            }
            let (source, node, test) = definitions[0];
            let owner = index.identity(source, node);
            if node.child_by_field_name("name").is_some_and(|name| {
                opaque.contains(&(owner.package, text(name, source).to_owned()))
            }) {
                continue;
            }
            for assertion in &self.0 {
                if !assertion.target.matches(&source.path)
                    || assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|e| e.matches(&source.path))
                    || !match assertion.scope {
                        Scope::Production => !test,
                        Scope::Tests => test,
                        Scope::All => true,
                    }
                {
                    continue;
                }
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, source))
                    .unwrap_or_default();
                let related = implementations.get(&id).cloned().unwrap_or_default();
                findings.push(Finding {
                    rule: Self::ID,
                    path: source.path.clone(),
                    span: Some(Span::new(&source.text, node.byte_range())),
                    related,
                    configuration: assertion.setting.clone(),
                    message: format!(
                        "\
                private empty marker trait `{name}` has no consumers or selective implem\
                entations"
                    ),
                    instruction: "\
                Remove the unused trait or establish a selective tagging, sealing, or sa\
                fety contract used by consumers."
                        .into(),
                });
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut c = node.walk();
    node.named_children(&mut c).collect()
}
fn descendants(node: Node<'_>) -> Vec<Node<'_>> {
    let mut result = vec![node];
    for child in children(node) {
        result.extend(descendants(child));
    }
    result
}
fn text<'a>(node: Node<'_>, source: &'a Source) -> &'a str {
    &source.text[node.byte_range()]
}
fn attrs(node: Node<'_>) -> bool {
    let mut previous = node.prev_named_sibling();
    while let Some(item) = previous {
        if item.kind() == "attribute_item" {
            return true;
        }
        if !matches!(item.kind(), "line_comment" | "block_comment") {
            break;
        }
        previous = item.prev_named_sibling();
    }
    false
}
fn candidate(node: Node<'_>, source: &Source) -> bool {
    !attrs(node)
        && !children(node).iter().any(|n| {
            matches!(
                n.kind(),
                "visibility_modifier" | "trait_bounds" | "type_parameters" | "where_clause"
            )
        })
        && !text(node, source)
            .split_whitespace()
            .take_while(|word| *word != "trait")
            .any(|w| matches!(w, "unsafe" | "auto"))
        && node.child_by_field_name("body").is_some_and(|body| {
            children(body)
                .iter()
                .all(|n| matches!(n.kind(), "line_comment" | "block_comment"))
        })
}
fn blanket(node: Node<'_>, source: &Source) -> bool {
    if attrs(node)
        || children(node)
            .iter()
            .any(|n| matches!(n.kind(), "where_clause" | "trait_bounds"))
        || text(node, source).contains('!')
    {
        return false;
    }
    let Some(owner) = node.child_by_field_name("type") else {
        return false;
    };
    if owner.kind() != "type_identifier" {
        return false;
    }
    node.child_by_field_name("type_parameters")
        .is_some_and(|params| {
            children(params).iter().any(|param| {
                param.kind() == "type_parameter"
                    && !children(*param).iter().any(|n| n.kind() == "trait_bounds")
                    && param
                        .child_by_field_name("name")
                        .is_some_and(|name| text(name, source) == text(owner, source))
            })
        })
}
fn definition_name(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() == "trait_item" && parent.child_by_field_name("name") == Some(node) {
        return true;
    }
    let mut child = node;
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if parent.kind() == "impl_item" {
            return parent.child_by_field_name("trait") == Some(child);
        }
        if matches!(
            parent.kind(),
            "trait_item" | "function_item" | "declaration_list"
        ) {
            break;
        }
        child = parent;
        ancestor = parent.parent();
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    fn check(source: &str, config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("lib.rs"), source).unwrap();
        std::fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/redundant-marker\"]]\ntarget='**/*.rs'\n{config}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<RedundantMarker>()?
            .check(root.path())
            .map(|r| r.findings)
    }
    #[test]
    fn reports_original_unused_and_blanket_cases() {
        let found = check(
            "trait Forgotten {} trait Anything {} impl<T> Anything for T {}",
            "",
        )
        .unwrap();
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .any(|f| f.message.contains("Forgotten") && f.related.is_empty())
        );
        assert!(
            found
                .iter()
                .any(|f| f.message.contains("Anything") && f.related.len() == 1)
        );
        assert!(found.iter().all(|f| f.span.is_some()));
    }
    #[test]
    fn preserves_original_meaningful_contracts_and_zero_sized_types() {
        let source = "pub trait ExternalTag {} trait Selective {} struct Linux; impl Sel\
            ective for Linux {} trait Required {} fn require<T: Required>() {} trait Agg\
            regate: Send + Sync {} unsafe trait Safety {} struct State; struct Other(())\
            ;";
        assert!(check(source, "").unwrap().is_empty());
        for source in [
            "trait Tag {} impl<T: Send> Tag for T {}",
            "trait Tag {} impl<T> Tag for T where T: Send {}",
            "#[doc=\"Safety contract\"] trait Tag {}",
            "trait Tag {fn perform();}",
            "trait Tag {} fn consume(_: &dyn Tag){}",
            "trait Tag {} fn consume<T>() where T: Tag {}",
            "trait Tag {} trait Sealed: Tag {}",
            "pub(crate) trait Tag {}",
            "trait Tag<T> {}",
            "trait Tag {} register!(Tag);",
        ] {
            assert!(check(source, "").unwrap().is_empty(), "{source}");
        }
    }
    #[test]
    fn resolves_bound_aliases_and_distinguishes_same_named_traits() {
        assert!(
            check("trait Tag {} use Tag as Alias; fn need<T:Alias>(){}", "")
                .unwrap()
                .is_empty()
        );
        let found = check(
            "mod a {trait Tag {} fn need<T:Tag>(){}} mod b {trait Tag {}}",
            "",
        )
        .unwrap();
        assert_eq!(found.len(), 1);
        let found = check(
            "trait Tag {} mod inner {trait Tag {} struct State; impl Tag for State {}}",
            "",
        )
        .unwrap();
        assert_eq!(found.len(), 1);
    }
    #[test]
    fn checks_own_implementation() {
        for source in [include_str!("mod.rs"), include_str!("config.rs")] {
            assert!(check(source, "").unwrap().is_empty());
        }
    }
    #[test]
    fn scope_exclusions_directives_and_settings() {
        assert!(
            check("trait Tag {}", "exclude='lib.rs'")
                .unwrap()
                .is_empty()
        );
        let input = "#[cfg(test)] mod tests {trait Tag {}}";
        assert!(check(input, "").unwrap().is_empty());
        assert_eq!(check(input, "scope='tests'").unwrap().len(), 1);
        assert!(
            check(
                "// linter:disable rust/redundant-marker -- reserved compatibility\
            \u{20}contract\ntrait Tag {}",
                ""
            )
            .unwrap()
            .is_empty()
        );
        for config in ["scope='bad'", "exclude=[]", "extra=true"] {
            assert!(matches!(check("", config), Err(Error::Configuration(_))));
        }
    }
}
