use crate::{Analysis, Source};
use linter::{Error, Finding, Project, Rule, RuleResult, Span, Status};
use tree_sitter::Node;
mod config;
use config::Assertion;
pub use config::Config;

pub struct ForbiddenCall {
    assertions: Vec<Assertion>,
}
impl Rule for ForbiddenCall {
    const ID: &'static str = "c/forbidden-call";
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
    if node.kind() == "call_expression"
        && let Some(function) = node
            .child_by_field_name("function")
            .filter(|function| function.kind() == "identifier")
        && let name = &source.text[function.byte_range()]
        && assertion.functions.contains(name)
    {
        findings.push(Finding {
            rule: ForbiddenCall::ID,
            path: source.path.clone(),
            configuration: assertion.setting.clone(),
            span: Some(Span::new(&source.text, node.byte_range())),
            related: Vec::new(),
            message: format!("call to '{name}' is forbidden: {}", assertion.description),
            instruction: assertion.instruction.clone(),
        });
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect(child, source, assertion, findings);
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    const POLICY: &str = "[[rules.\"c/forbidden-call\"]]\ntarget = '**/*.c'\nfunctions =\
        \u{20}['system', 'popen']\ndescription = 'Shell execution.'\ninstruction = 'Laun\
        ch an explicit argv vector.'";
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::ForbiddenCall>()?
            .check(root)
    }
    fn run(source: &str, policy: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("linter.toml"), policy).unwrap();
        fs::write(root.path().join("arbitrary.c"), source).unwrap();
        check(root.path()).unwrap()
    }
    #[test]
    fn reports_exact_call_location_configured_reason_and_instruction() {
        let report = run("int f(void) { return system(\"x\"); }\n", POLICY);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule, "c/forbidden-call");
        let span = report.findings[0].span.as_ref().unwrap();
        assert_eq!((span.line, span.column), (1, 22));
        assert_eq!(
            report.findings[0].instruction,
            "Launch an explicit argv vector."
        );
        assert!(report.findings[0].message.contains("Shell execution."));
    }
    #[test]
    fn matches_no_comments_strings_members_pointer_calls_macros_or_substrings() {
        let source = "// system(\"x\")\n#define SHELL() system(\"x\")\nconst char *s = \
            \"popen(x)\";\nint f(void) { object.system(); object->system(); (*system)(\"\
            x\"); (system)(\"x\"); SHELL(); return subsystem(); }\n";
        assert!(run(source, POLICY).findings.is_empty());
    }
    #[test]
    fn default_has_no_bans_and_callers_choose_their_vocabulary() {
        let source = "char *f(void) { return getenv(\"HOME\"); }\n";
        assert!(run(source, "").findings.is_empty());
        assert!(run(source, POLICY).findings.is_empty());
        let policy = POLICY.replace("['system', 'popen']", "['getenv']");
        assert_eq!(run(source, &policy).findings.len(), 1);
    }
    #[test]
    fn directives_validate_and_cannot_be_forged_in_strings() {
        let source = "const char *s = \"linter:disable c/forbidden-call -- forged\";\nin\
            t f(void) { return system(\"x\"); }";
        assert_eq!(run(source, POLICY).findings.len(), 1);
        let source = "// linter:disable c/forbidden-call -- compatibility launcher has n\
            o argv API\nint f(void) { return system(\"x\"); }\n";
        let report = run(source, POLICY);
        assert!(report.findings.is_empty());
        assert_eq!(report.suppressed.len(), 1);
        for source in [
            "// linter:disable unknown -- reason\nint f(void) { return 0; }",
            "// linter:disable c/forbidden-call\nint f(void) { return system(\"x\"); }",
            "// linter:disable c/forbidden-call -- obsolete\nint f(void) { return 0; }",
        ] {
            assert!(
                run(source, POLICY)
                    .findings
                    .iter()
                    .any(|finding| finding.rule == "directive")
            );
        }
    }
    #[test]
    fn blocks_report_independently_and_exclusions_apply() {
        let policy = format!("{POLICY}\n{POLICY}");
        let report = run("void f(void) { system(\"x\"); }", &policy);
        assert_eq!(report.findings.len(), 2);
        assert_ne!(
            report.findings[0].configuration,
            report.findings[1].configuration
        );
        assert!(
            run(
                "void f(void) { system(\"x\"); }",
                &format!("{POLICY}\nexclude = 'arbitrary.c'")
            )
            .findings
            .is_empty()
        );
    }
    #[test]
    fn invalid_or_missing_configured_policy_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        for policy in [
            POLICY.replace("['system', 'popen']", "[]"),
            POLICY.replace("['system', 'popen']", "['bad-name']"),
            POLICY.replace("'Shell execution.'", "'  '"),
            POLICY.replace("'Launch an explicit argv vector.'", "''"),
            POLICY.replace("target = '**/*.c'", "target = []"),
            format!("{POLICY}\nunknown = true"),
            "[[rules.\"c/forbidden-call\"]]\ntarget = '*'\nfunctions = ['system']".into(),
        ] {
            fs::write(root.path().join("linter.toml"), &policy).unwrap();
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{policy}"
            );
        }
    }
}
