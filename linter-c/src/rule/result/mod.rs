use crate::{Analysis, Source};
use linter::{Error, Finding, Project, Rule, RuleResult, Span, Status};
use tree_sitter::Node;
mod config;
use config::Assertion;
pub use config::Config;

pub struct ResultUse {
    assertions: Vec<Assertion>,
}
impl Rule for ResultUse {
    const ID: &'static str = "c/ignored-result";
    type Analysis = Analysis;
    type Config = Config;
    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            assertions: config.compile()?,
        })
    }
    fn configured(&self) -> bool {
        !self.assertions.is_empty()
    }
    fn check(&self, _: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let mut findings = Vec::new();
        for source in &analysis.sources {
            for assertion in self.assertions.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                collect(source.syntax.root_node(), source, assertion, &mut findings);
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn collect(node: Node<'_>, source: &Source, assertion: &Assertion, findings: &mut Vec<Finding>) {
    if node.kind() == "expression_statement"
        && let Some(call) = node.named_child(0).and_then(discarded)
        && let Some(function) = call.child_by_field_name("function").and_then(designator)
        && function.kind() == "identifier"
        && let name = &source.text[function.byte_range()]
        && assertion.functions.contains(name)
    {
        findings.push(Finding {
            rule: ResultUse::ID, path: source.path.clone(), configuration: assertion.setting.clone(),
            span: Some(Span::new(&source.text, node.byte_range())), related: Vec::new(),
            message: format!("result of configured must-use C function '{name}' is discarded"),
            instruction: "Handle the result, return it to the caller, or attach a reasoned directive for this exact intentional discard.".into(),
        });
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect(child, source, assertion, findings);
    }
}

fn unparenthesized(mut node: Node<'_>) -> Option<Node<'_>> {
    while node.kind() == "parenthesized_expression" {
        node = node.named_child(0)?;
    }
    Some(node)
}
fn discarded(node: Node<'_>) -> Option<Node<'_>> {
    let node = unparenthesized(node)?;
    if node.kind() == "cast_expression" {
        return discarded(node.child_by_field_name("value")?);
    }
    (node.kind() == "call_expression").then_some(node)
}
fn designator(node: Node<'_>) -> Option<Node<'_>> {
    let node = unparenthesized(node)?;
    if node.kind() == "pointer_expression" {
        return designator(node.child_by_field_name("argument")?);
    }
    Some(node)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::ResultUse>()?
            .check(root)
    }
    fn run(source: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"c/ignored-result\"]]\ntarget = '**/*.c'\nfunctions = ['open_resource']",
        )
        .unwrap();
        fs::write(root.path().join("resource.c"), source).unwrap();
        check(root.path()).unwrap()
    }
    #[test]
    fn reports_direct_parenthesized_cast_and_void_discarded_results() {
        for statement in [
            "open_resource();",
            "(((open_resource())));",
            "(((open_resource)))();",
            "(long)(((open_resource)))();",
            "(*(((open_resource))))();",
            "(void)open_resource();",
            "(void)(long)open_resource();",
        ] {
            let report = run(&format!("void run(void) {{ {statement} }}"));
            assert_eq!(report.findings.len(), 1, "{statement}");
            assert!(report.findings[0].span.is_some());
        }
    }
    #[test]
    fn accepts_consumed_returned_conditional_nested_and_other_calls() {
        for source in [
            "int run(void) { int value = open_resource(); value = open_resource(); return open_resource(); }",
            "void run(void) { if (open_resource()) consume(); while (open_resource()) break; }",
            "void run(void) { consume(open_resource()); other_resource(); }",
            "void run(void) { int (*alias)(void) = open_resource; alias(); }",
            "void run(void) { /* open_resource(); */ const char *s = \"open_resource();\"; }",
            "#define DISCARD() open_resource()\nvoid run(void) { DISCARD(); }",
        ] {
            assert!(run(source).findings.is_empty(), "{source}");
        }
        assert_eq!(
            run("void run(void) { open_resource(open_resource()); }")
                .findings
                .len(),
            1
        );
    }
    #[test]
    fn reasoned_directive_is_exact_and_stale_directive_fails() {
        let report = run(
            "void run(void) {\n// linter:disable c/ignored-result -- best-effort cleanup\n(void)open_resource();\nopen_resource();\n}",
        );
        assert_eq!(report.suppressed.len(), 1);
        assert_eq!(report.findings.len(), 1);
        let report = run(
            "void run(void) {\n// linter:disable c/ignored-result -- best-effort cleanup\nint value = open_resource();\n}",
        );
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule, "directive");
    }
    #[test]
    fn configuration_targets_and_exclusions_are_enforced() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "target = '*'",
            "target = '*'\nfunctions = []",
            "target = '*'\nfunctions = ['bad-name']",
            "target = []\nfunctions = ['open_resource']",
            "target = '*'\nfunctions = ['open_resource']\nexclude = []",
            "target = '*'\nfunctions = ['open_resource']\nunknown = true",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"c/ignored-result\"]]\n{fields}"),
            )
            .unwrap();
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
        fs::write(root.path().join("linter.toml"), "[[rules.\"c/ignored-result\"]]\ntarget = ['*.c']\nexclude = 'skip.c'\nfunctions = ['open_resource']").unwrap();
        for name in ["skip.c", "skip.h"] {
            fs::write(
                root.path().join(name),
                "void run(void) { open_resource(); }",
            )
            .unwrap();
        }
        assert!(check(root.path()).unwrap().findings.is_empty());
    }
}
