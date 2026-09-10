use crate::{Analysis, Source};
use linter::{Error, Finding, Project, Rule, RuleResult, Span, Status};
use tree_sitter::Node;
mod config;
use config::Assertion;
pub use config::Config;

pub struct ProvisionalComment {
    assertions: Vec<Assertion>,
}
impl Rule for ProvisionalComment {
    const ID: &'static str = "rust/provisional-diagnostic";
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
            for assertion in &self.assertions {
                if assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|target| target.matches(&source.path))
                {
                    assertion.visit(source.syntax.root_node(), source, &mut findings);
                }
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
impl Assertion {
    fn visit(&self, node: Node<'_>, source: &Source, findings: &mut Vec<Finding>) {
        if matches!(node.kind(), "comment" | "line_comment" | "block_comment") {
            self.comment(node, source, findings);
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, source, findings);
        }
    }
    fn comment(&self, node: Node<'_>, source: &Source, findings: &mut Vec<Finding>) {
        let comment = source.text[node.byte_range()].to_ascii_lowercase();
        if !self.terms.iter().all(|term| comment.contains(term)) {
            return;
        }
        let start = node.start_byte() + comment.find(&self.terms[0]).unwrap_or_default();
        findings.push(Finding {
            rule: ProvisionalComment::ID,
            path: source.path.clone(),
            span: Some(Span::new(&source.text, start..start + self.terms[0].len())),
            related: Vec::new(),
            configuration: self.setting.clone(),
            message: "Source comment declares configured provisional instrumentation".into(),
            instruction:
                "Remove the instrumentation or give observability a permanent bounded contract."
                    .into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(text: &str, fields: &str) -> Result<linter::Report, Error> {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("input.rs"), text).unwrap();
        std::fs::write(
            root.path().join("linter.toml"),
            format!(
                r#"
[[rules."rust/provisional-diagnostic"]]
target = "*.rs"
terms = ["temporary", "diagnostic"]
{fields}
"#
            ),
        )
        .unwrap();
        linter::Registry::default()
            .register::<ProvisionalComment>()?
            .check(root.path())
    }
    #[test]
    fn actual_comments_preserve_donor_positive_and_negative_cases() {
        let report = check("    // Temporary child diagnostics.\nfn run() {}", "").unwrap();
        assert_eq!(report.findings.len(), 1);
        let span = report.findings[0].span.as_ref().unwrap();
        assert_eq!((span.line, span.column), (1, 8));
        assert_eq!(
            check("/* diagnostics are\ntemporary */\nfn run() {}", "")
                .unwrap()
                .findings
                .len(),
            1
        );
        assert!(check("// Temporary directory owned by this call.\n// Bounded lifecycle diagnostics.\nconst TEXT: &str = \"temporary diagnostics\";", "")
            .unwrap().findings.is_empty());
    }
    #[test]
    fn target_configuration_and_directives_remain_observable() {
        assert!(
            check("// temporary diagnostics\nfn run() {}", "exclude = '*.rs'")
                .unwrap()
                .findings
                .is_empty()
        );
        assert!(matches!(
            check("fn run() {}", "unknown = true"),
            Err(Error::Configuration(_))
        ));
        let report = check("// linter:disable rust/provisional-diagnostic -- Documented bounded investigation.\n// temporary diagnostics\nfn run() {}", "").unwrap();
        assert!(report.findings.is_empty());
        assert_eq!(report.suppressed.len(), 1);
    }
}
