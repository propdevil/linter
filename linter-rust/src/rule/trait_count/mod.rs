use crate::{
    Analysis, Source,
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::fs;
use tree_sitter::Node;
mod config;
pub use config::Config;

pub struct TraitMethodCount(Vec<Assertion>);
impl Rule for TraitMethodCount {
    const ID: &'static str = "rust/trait-method-count";
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
            fs::canonicalize(project.root()).map_err(|error| Error::Analysis(error.to_string()))?;
        let mut findings = Vec::new();
        for source in &analysis.sources {
            let mut tests = vec![false; source.text.len()];
            if integration(source, &root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            for assertion in self.0.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                inspect(
                    source.syntax.root_node(),
                    source,
                    &tests,
                    assertion,
                    &mut findings,
                );
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
fn included(node: Node<'_>, tests: &[bool], scope: Scope) -> bool {
    let test = tests.get(node.start_byte()) == Some(&true);
    match scope {
        Scope::Production => !test,
        Scope::Tests => test,
        Scope::All => true,
    }
}
fn inspect(
    node: Node<'_>,
    source: &Source,
    tests: &[bool],
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) {
    if node.kind() == "trait_item"
        && included(node, tests, assertion.scope)
        && let Some(finding) = finding(node, source, tests, assertion)
    {
        findings.push(finding);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, tests, assertion, findings);
    }
}
fn finding(
    node: Node<'_>,
    source: &Source,
    tests: &[bool],
    assertion: &Assertion,
) -> Option<Finding> {
    let body = node.child_by_field_name("body")?;
    let mut cursor = body.walk();
    let methods: Vec<_> = body
        .named_children(&mut cursor)
        .filter(|method| {
            matches!(method.kind(), "function_signature_item" | "function_item")
                && included(*method, tests, assertion.scope)
        })
        .collect();
    if methods.len() <= assertion.max_methods {
        return None;
    }
    let name = &source.text[node.child_by_field_name("name")?.byte_range()];
    let related = methods
        .iter()
        .map(|method| Evidence {
            path: source.path.clone(),
            span: Some(Span::new(&source.text, method.byte_range())),
            message: format!(
                "Counted trait method `{}`",
                method
                    .child_by_field_name("name")
                    .map(|name| &source.text[name.byte_range()])
                    .unwrap_or("<anonymous>")
            ),
        })
        .collect();
    Some(Finding {
        rule: TraitMethodCount::ID,
        path: source.path.clone(),
        span: Some(Span::new(&source.text, node.byte_range())),
        related,
        configuration: format!("{}.max_methods", assertion.setting),
        message: format!(
            "trait `{name}` declares {} functions; maximum is {}",
            methods.len(),
            assertion.max_methods
        ),
        instruction: "Keep each trait focused on a small cohesive capability; split inde\
            pendent operations into separate traits."
            .into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(text: &str, settings: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), text).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/trait-method-count\"]]\ntarget='**/*.rs'\n{settings}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<TraitMethodCount>()
            .unwrap()
            .check(root.path())
            .unwrap()
    }
    #[test]
    fn threshold_counts_declarations_and_default_bodies_only() {
        let text = "trait Capability { type Value; const NAME: &str; fn one(&self); fn t\
            wo()->Self; fn three(&self) {} }";
        assert!(check(text, "").findings.is_empty());
        let report = check(text, "max_methods=2");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(
            report.findings[0].message,
            "trait `Capability` declares 3 functions; maximum is 2"
        );
        assert_eq!(report.findings[0].related.len(), 3);
    }
    #[test]
    fn payment_decorated_traits_keep_exact_counts_and_method_evidence() {
        let text = "trait Wallet { fn address(&self); fn history(&self); fn transfer(&se\
            lf); fn balance(&self); }\n/// Distinct capabilities.\n#[allow(dead_code)]\n\
            #[must_use]\ntrait Runtime { fn load_wallet(&self);fn save_wallet(&self);fn \
            start_sync(&self);fn stop_sync(&self);fn inspect_checkpoint(&self);fn query_\
            height(&self);fn configure_rpc(&self);fn update_rpc(&self); }";
        let report = check(text, "");
        assert_eq!(report.findings.len(), 2);
        let runtime = report
            .findings
            .iter()
            .find(|finding| finding.message.contains("`Runtime`"))
            .unwrap();
        assert_eq!(runtime.span.as_ref().unwrap().line, 5);
        assert_eq!(runtime.related.len(), 8);
        assert!(runtime.message.contains("8 functions; maximum is 3"));
    }
    #[test]
    fn production_excludes_test_only_methods_and_traits() {
        let text = "trait Capability { fn production(); #[cfg(test)] fn fixture(); #[cfg\
            (test)] fn scenario() {} } #[cfg(test)] trait Tests { fn one();fn two();fn t\
            hree();fn four(); }";
        assert!(check(text, "max_methods=1").findings.is_empty());
        assert_eq!(check(text, "scope='all'\nmax_methods=1").findings.len(), 2);
        assert_eq!(check(text, "scope='tests'").findings.len(), 1);
    }
    #[test]
    fn does_not_count_inherited_or_impl_or_nested_function_methods() {
        let text = "trait Base { fn one(); fn two(); fn three(); } trait Child: Base { f\
            n four() { fn nested() {} } } struct Value; impl Value { fn one() {} fn two(\
            ) {} fn three() {} fn four() {} }";
        assert!(check(text, "").findings.is_empty());
    }
    #[test]
    fn selectors_directives_and_invalid_configuration_are_observed() {
        let text = "trait Capability { fn one();fn two();fn three();fn four(); }";
        assert!(check(text, "exclude='lib.rs'").findings.is_empty());
        let report = check(
            &format!(
                "// linter:disable rust/trait-method-count -- External protocol requires\
                \u{20}this capability set.\n{text}"
            ),
            "",
        );
        assert!(report.findings.is_empty());
        assert_eq!(report.suppressed.len(), 1);
        for fields in [
            "target=[]",
            "target='../*'",
            "target='*'\nmax_methods=0",
            "target='*'\nscope='maybe'",
            "glob='*'",
            "target='*'\nmaximum=5",
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/trait-method-count\"]]\n{fields}"),
            )
            .unwrap();
            assert!(matches!(
                linter::Registry::default()
                    .register::<TraitMethodCount>()
                    .unwrap()
                    .check(root.path()),
                Err(Error::Configuration(_))
            ));
        }
    }
    #[test]
    fn own_rule_passes_the_configured_trait_budget() {
        assert!(check(include_str!("mod.rs"), "").findings.is_empty());
        assert!(check(include_str!("config.rs"), "").findings.is_empty());
    }
}
