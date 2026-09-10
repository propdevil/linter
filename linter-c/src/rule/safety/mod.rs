use crate::{Analysis, Source};
use linter::{Error, Finding, Project, Rule, RuleResult, Span, Status};
use tree_sitter::Node;
mod config;
use config::Assertion;
pub use config::Config;

pub struct Safety {
    assertions: Vec<Assertion>,
}
impl Rule for Safety {
    const ID: &'static str = "c/safety-rationale";
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
            let mut comments = Vec::new();
            collect_comments(source.syntax.root_node(), &mut comments);
            for assertion in self.assertions.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                collect(
                    source.syntax.root_node(),
                    source,
                    assertion,
                    &comments,
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

fn collect_comments<'tree>(node: Node<'tree>, comments: &mut Vec<Node<'tree>>) {
    if node.kind() == "comment" {
        comments.push(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_comments(child, comments);
    }
}
fn collect(
    node: Node<'_>,
    source: &Source,
    assertion: &Assertion,
    comments: &[Node<'_>],
    findings: &mut Vec<Finding>,
) {
    if node.kind() == "call_expression"
        && let Some(function) = node
            .child_by_field_name("function")
            .filter(|function| function.kind() == "identifier")
        && let name = &source.text[function.byte_range()]
        && assertion.operations.contains(name)
        && !attached(source, comments, node.start_byte())
        && !attached(source, comments, anchor(node).start_byte())
    {
        findings.push(Finding {
            rule: Safety::ID, path: source.path.clone(), configuration: assertion.setting.clone(),
            span: Some(Span::new(&source.text, node.byte_range())), related: Vec::new(),
            message: format!("configured safety-sensitive C operation '{name}' has no attached nonempty SAFETY: rationale"),
            instruction: "State the pointer, lifetime, bounds, ownership or concurrency invariant in an immediately attached SAFETY: comment.".into(),
        });
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect(child, source, assertion, comments, findings);
    }
}
fn anchor(mut node: Node<'_>) -> Node<'_> {
    while let Some(parent) = node.parent() {
        if node.kind().ends_with("_statement") || node.kind() == "declaration" {
            break;
        }
        if matches!(parent.kind(), "compound_statement" | "translation_unit") {
            break;
        }
        node = parent;
    }
    node
}
fn attached(source: &Source, comments: &[Node<'_>], mut before: usize) -> bool {
    let mut block = Vec::new();
    let anchor = before;
    for comment in comments
        .iter()
        .rev()
        .filter(|comment| comment.end_byte() <= anchor)
    {
        let gap = &source.text[comment.end_byte()..before];
        if !gap.chars().all(char::is_whitespace)
            || gap.bytes().filter(|byte| *byte == b'\n').count() > 1
        {
            break;
        }
        block.push(&source.text[comment.byte_range()]);
        before = comment.start_byte();
    }
    block.reverse();
    let content = block
        .iter()
        .flat_map(|comment| {
            comment
                .trim_start_matches("//")
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .lines()
        })
        .map(|line| line.trim().trim_start_matches('*').trim())
        .collect::<Vec<_>>()
        .join("\n");
    content
        .split_once("SAFETY:")
        .is_some_and(|(_, reason)| reason.chars().any(char::is_alphanumeric))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::Safety>()?
            .check(root)
    }
    fn run(source: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"c/safety-rationale\"]]\ntarget = '**/*.c'\noperations = ['copy_bytes']",
        )
        .unwrap();
        fs::write(root.path().join("memory.c"), source).unwrap();
        check(root.path()).unwrap()
    }
    #[test]
    fn accepts_contiguous_line_block_inline_and_statement_rationales() {
        for body in [
            "// SAFETY: buffers are valid.\ncopy_bytes();",
            "/* SAFETY: buffers are valid. */ copy_bytes();",
            "/*\n * SAFETY:\n * Buffers are valid.\n */\ncopy_bytes();",
            "// SAFETY:\n// Buffers are valid.\ncopy_bytes();",
            "// SAFETY: buffers are valid.\n// Bounds checked above.\ncopy_bytes();",
            "// SAFETY: buffers are valid.\nint result = copy_bytes();",
            "int result = /* SAFETY: buffers are valid. */ copy_bytes();",
        ] {
            assert!(
                run(&format!("void run(void) {{\n{body}\n}}"))
                    .findings
                    .is_empty(),
                "{body}"
            );
        }
    }
    #[test]
    fn rejects_missing_detached_empty_or_string_rationales() {
        for body in [
            "copy_bytes();",
            "// SAFETY: buffers are valid.\n\ncopy_bytes();",
            "// SAFETY:\ncopy_bytes();",
            "/* SAFETY: */\ncopy_bytes();",
            "/* SAFETY:\n *\n */\ncopy_bytes();",
            "const char *reason = \"SAFETY: buffers are valid\";\ncopy_bytes();",
            "// SAFETY: buffers are valid.\nother();\ncopy_bytes();",
            "// SAFETY: buffers are valid.\nother(); copy_bytes();",
        ] {
            assert_eq!(
                run(&format!("void run(void) {{\n{body}\n}}"))
                    .findings
                    .len(),
                1,
                "{body}"
            );
        }
    }
    #[test]
    fn only_configured_syntax_calls_are_checked_and_nested_calls_each_report() {
        assert!(
            run(
                "void run(void) { /* copy_bytes(); */ const char *s = \"copy_bytes();\"; other(); }"
            )
            .findings
            .is_empty()
        );
        assert_eq!(
            run("void run(void) { copy_bytes(copy_bytes()); }")
                .findings
                .len(),
            2
        );
    }
    #[test]
    fn directives_are_exact_and_stale_directives_fail() {
        let report = run(
            "void run(void) {\n// linter:disable c/safety-rationale -- generated bounds proof\ncopy_bytes();\ncopy_bytes();\n}",
        );
        assert_eq!(report.suppressed.len(), 1);
        assert_eq!(report.findings.len(), 1);
        let report = run(
            "void run(void) {\n// linter:disable c/safety-rationale -- generated bounds proof\nreturn;\n}",
        );
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule, "directive");
    }
    #[test]
    fn validates_configuration_and_honors_selectors() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "target = '*'",
            "target = '*'\noperations = []",
            "target = '*'\noperations = ['bad-name']",
            "target = []\noperations = ['copy_bytes']",
            "target = '*'\noperations = ['copy_bytes']\nexclude = []",
            "target = '*'\noperations = ['copy_bytes']\nunknown = true",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"c/safety-rationale\"]]\n{fields}"),
            )
            .unwrap();
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
        fs::write(root.path().join("linter.toml"), "[[rules.\"c/safety-rationale\"]]\ntarget = ['*.c']\nexclude = 'skip.c'\noperations = ['copy_bytes']").unwrap();
        for name in ["skip.c", "skip.h"] {
            fs::write(root.path().join(name), "void run(void) { copy_bytes(); }").unwrap();
        }
        assert!(check(root.path()).unwrap().findings.is_empty());
    }
}
