use crate::{Analysis, Source};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use tree_sitter::Node;

mod config;
use config::Assertion;
pub use config::Config;

pub struct MaxIndent(Vec<Assertion>);

impl Rule for MaxIndent {
    const ID: &'static str = "rust/max-indent";
    type Config = Config;
    type Analysis = Analysis;

    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self(config.compile()?))
    }

    fn configured(&self) -> bool {
        !self.0.is_empty()
    }

    fn check(&self, _: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let mut findings = Vec::new();
        for source in &analysis.sources {
            let lines = Lines::new(&source.text);
            for assertion in self.0.iter().filter(|assertion| assertion.selects(source)) {
                inspect(
                    source.syntax.root_node(),
                    source,
                    &lines,
                    assertion,
                    &mut findings,
                )?;
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

struct Lines<'a>(Vec<(usize, &'a str)>);

impl Assertion {
    fn selects(&self, source: &Source) -> bool {
        self.target.matches(&source.path)
            && !self
                .exclude
                .as_ref()
                .is_some_and(|value| value.matches(&source.path))
    }
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let mut offset = 0;
        Self(
            text.split_inclusive('\n')
                .map(|line| {
                    let start = offset;
                    offset += line.len();
                    (start, line)
                })
                .collect(),
        )
    }

    fn columns(&self, row: usize, tab_width: usize) -> Result<usize, Error> {
        self.0[row]
            .1
            .chars()
            .take_while(|value| value.is_whitespace())
            .try_fold(0usize, |column, value| {
                let advance = if value == '\t' {
                    tab_width - column % tab_width
                } else {
                    1
                };
                column.checked_add(advance).ok_or_else(|| {
                    Error::Analysis("indentation exceeds the supported column range".into())
                })
            })
    }
}

fn inspect(
    node: Node<'_>,
    source: &Source,
    lines: &Lines<'_>,
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) -> Result<(), Error> {
    if node.kind() == "function_item"
        && let Some(body) = node.child_by_field_name("body")
        && let Some(finding) = measure(node, body, source, lines, assertion)?
    {
        findings.push(finding);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, lines, assertion, findings)?;
    }
    Ok(())
}

fn measure(
    function: Node<'_>,
    body: Node<'_>,
    source: &Source,
    lines: &Lines<'_>,
    assertion: &Assertion,
) -> Result<Option<Finding>, Error> {
    let baseline = lines.columns(function.start_position().row, assertion.tab_width)?;
    let mut maximum = assertion.max_columns;
    let mut location = None;
    for row in body.start_position().row..=body.end_position().row {
        let (start, line) = lines.0[row];
        let content = line.trim_start();
        let offset = start + line.len() - content.len();
        if content.is_empty() || offset <= body.start_byte() || offset >= body.end_byte() {
            continue;
        }
        if !code_in_body(body, offset) {
            continue;
        }
        let relative = lines
            .columns(row, assertion.tab_width)?
            .saturating_sub(baseline);
        if relative > maximum {
            maximum = relative;
            location = Some(offset);
        }
    }
    Ok(location.map(|offset| finding(function, source, offset, maximum, assertion)))
}

fn finding(
    function: Node<'_>,
    source: &Source,
    offset: usize,
    maximum: usize,
    assertion: &Assertion,
) -> Finding {
    let span = Span::new(&source.text, offset..offset);
    let name = function
        .child_by_field_name("name")
        .map(|node| &source.text[node.byte_range()])
        .unwrap_or("<anonymous>");
    Finding {
        rule: MaxIndent::ID,
        path: source.path.clone(),
        span: Some(Span::new(&source.text, function.byte_range())),
        related: vec![Evidence {
            path: source.path.clone(),
            span: Some(span.clone()),
            message: format!("{maximum} indentation columns relative to the function declaration"),
        }],
        configuration: assertion.setting.clone(),
        message: format!(
            "`{name}` reaches {maximum} relative indentation columns on line {}; maximum is {}",
            span.line, assertion.max_columns
        ),
        instruction: concat!(
            "Flatten nested control flow with guard clauses where semantics permit; ",
            "give deeply nested expressions readable intermediate steps. ",
            "Module and impl indentation do not count."
        )
        .into(),
    }
}

fn code_in_body(body: Node<'_>, offset: usize) -> bool {
    let mut current = body.descendant_for_byte_range(offset, offset + 1);
    while let Some(node) = current {
        if node == body {
            return true;
        }
        if matches!(
            node.kind(),
            "function_item"
                | "mod_item"
                | "impl_item"
                | "trait_item"
                | "line_comment"
                | "block_comment"
        ) {
            return false;
        }
        if matches!(node.kind(), "string_literal" | "raw_string_literal")
            && offset > node.start_byte()
        {
            return false;
        }
        current = node.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn check(source: &str, settings: &str) -> Result<linter::Report, Error> {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("input.rs"), source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/max-indent\"]]\ntarget='*.rs'\n{settings}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<MaxIndent>()?
            .check(root.path())
    }

    const NESTED: &str = "fn run(a: bool, b: bool) {
    if a {
        if b {
            work();
        }
    }
}
";

    #[test]
    fn enclosing_modules_and_impls_do_not_change_the_budget() {
        let indented = NESTED
            .lines()
            .map(|line| format!("        {line}\n"))
            .collect::<String>();
        for source in [
            NESTED.to_owned(),
            format!("mod outer {{\n    struct Item;\n    impl Item {{\n{indented}    }}\n}}"),
        ] {
            let report = check(&source, "max_columns=8").unwrap();
            assert_eq!(report.findings.len(), 1);
            assert!(
                report.findings[0]
                    .message
                    .contains("12 relative indentation columns")
            );
            assert!(report.findings[0].related[0].span.is_some());
        }
    }

    #[test]
    fn early_returns_remove_the_indentation_violation() {
        assert_eq!(check(NESTED, "max_columns=8").unwrap().findings.len(), 1);
        let guard = "fn run(a: bool, b: bool) {
    if !a { return; }
    if !b { return; }
    work();
}
";
        assert!(check(guard, "max_columns=8").unwrap().findings.is_empty());
    }

    #[test]
    fn nested_functions_reset_but_closures_keep_the_enclosing_budget() {
        let source = "fn outer() {\n    fn inner() {\n        work();\n    }\n}\n";
        assert!(check(source, "max_columns=4").unwrap().findings.is_empty());
        let closure = "fn outer() {\n    let run = || {\n        work();\n    };\n}\n";
        assert_eq!(check(closure, "max_columns=4").unwrap().findings.len(), 1);
    }

    #[test]
    fn ignores_signatures_comments_literal_contents_and_nonfunctions() {
        let source = r##"const TABLE: &[u8] = &[
                            1,
];
fn run(
                        argument: bool,
) {
    let text = r#"sample
                             deeply indented fixture
"#;
                            // explanation
}
"##;
        assert!(check(source, "max_columns=4").unwrap().findings.is_empty());
    }

    #[test]
    fn checks_tests_trait_defaults_and_wrapped_expressions() {
        for source in [
            format!("#[test]\n{NESTED}"),
            format!("trait Item {{\n{NESTED}}}"),
            "fn run() {\n    call(\n            argument,\n    );\n}".into(),
            "fn run() {\n    call(\n            \"argument\",\n    );\n}".into(),
        ] {
            assert_eq!(check(&source, "max_columns=8").unwrap().findings.len(), 1);
        }
    }

    #[test]
    fn tab_stops_crlf_and_exact_limits_work() {
        let source = "mod outer {\r\n\tfn run() {\r\n\t\twork();\r\n\t}\r\n}";
        assert!(
            check(source, "max_columns=4\ntab_width=4")
                .unwrap()
                .findings
                .is_empty()
        );
        assert_eq!(
            check(source, "max_columns=3\ntab_width=4")
                .unwrap()
                .findings
                .len(),
            1
        );
    }

    #[test]
    fn validates_configuration_and_honors_exclusions_and_directives() {
        for settings in ["max_columns=0", "tab_width=0", "unknown=true", "exclude=[]"] {
            assert!(matches!(
                check(NESTED, settings),
                Err(Error::Configuration(_))
            ));
        }
        assert!(
            check(NESTED, "exclude='input.rs'\nmax_columns=4")
                .unwrap()
                .findings
                .is_empty()
        );
        let source =
            format!("// linter:disable rust/max-indent -- External callback structure.\n{NESTED}");
        let report = check(&source, "max_columns=4").unwrap();
        assert!(report.findings.is_empty());
        assert_eq!(report.suppressed.len(), 1);
    }
}
