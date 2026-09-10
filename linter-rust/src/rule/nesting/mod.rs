use crate::{
    Analysis, Source,
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};
use linter::{Error, Finding, Project, Rule, RuleResult, Status};
use std::fs;
use tree_sitter::Node;
mod config;
pub use config::Config;

pub struct NestingRule(Vec<Assertion>);
impl Rule for NestingRule {
    const ID: &'static str = "rust/nesting";
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
                functions(
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

fn functions(
    node: Node<'_>,
    source: &Source,
    tests: &[bool],
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) {
    let selected = match assertion.scope {
        Scope::Production => tests.get(node.start_byte()) != Some(&true),
        Scope::Tests => tests.get(node.start_byte()) == Some(&true),
        Scope::All => true,
    };
    if node.kind() == "function_item"
        && selected
        && let Some(body) = node.child_by_field_name("body")
    {
        let mut depth = Depth {
            source,
            tests,
            assertion,
            maximum: 0,
            line: 0,
            construct: "",
        };
        depth.visit(body, 0, true);
        if depth.maximum > assertion.max_depth {
            let name = node
                .child_by_field_name("name")
                .map(|name| &source.text[name.byte_range()])
                .unwrap_or("<anonymous>");
            findings.push(Finding {
                span: Some(linter::Span::new(&source.text, node.byte_range())),
                related: Vec::new(),
                rule: NestingRule::ID,
                path: source.path.clone(),
                configuration: format!("{}.max_depth", assertion.setting),
                message: format!(
                    "`{name}` reaches syntactic nesting depth {} at {} on line {}; maximum is {}",
                    depth.maximum, depth.construct, depth.line, assertion.max_depth
                ),
                instruction:
                    "Use early returns, extract receiver behavior, or model the nested state."
                        .into(),
            });
        }
    }
    for child in children(node) {
        functions(child, source, tests, assertion, findings);
    }
}

struct Depth<'a> {
    source: &'a Source,
    tests: &'a [bool],
    assertion: &'a Assertion,
    maximum: usize,
    line: usize,
    construct: &'static str,
}

impl Depth<'_> {
    fn enter(&mut self, node: Node<'_>, depth: usize, construct: &'static str) -> usize {
        let next = depth + 1;
        if next > self.maximum {
            self.maximum = next;
            self.line = node.start_position().row + 1;
            self.construct = construct;
        }
        next
    }

    fn visit(&mut self, node: Node<'_>, depth: usize, statement: bool) {
        if matches!(self.assertion.scope, Scope::Production)
            && self.tests.get(node.start_byte()) == Some(&true)
        {
            return;
        }
        match node.kind() {
            "function_item" => {} // Nested declarations receive their own budget.
            "block" => self.block(node, depth, statement),
            "if_expression" => self.branch(node, depth, statement, false),
            "match_expression" => self.matching(node, depth, statement),
            "for_expression" | "while_expression" | "loop_expression" => self.looping(node, depth),
            "closure_expression" => self.closure(node, depth),
            "async_block" => {
                let next = self.enter(node, depth, "async block");
                for child in children(node) {
                    self.visit(child, next, true);
                }
            }
            _ => {
                for child in children(node) {
                    self.visit(child, depth, false);
                }
            }
        }
    }

    fn block(&mut self, node: Node<'_>, depth: usize, position: bool) {
        let items = children(node);
        for (index, child) in items.iter().enumerate() {
            let statement = position
                || index + 1 != items.len()
                || (child.kind() == "expression_statement"
                    && self.source.text[child.byte_range()]
                        .trim_end()
                        .ends_with(';'));
            let expression = if child.kind() == "expression_statement" {
                children(*child).first().copied().unwrap_or(*child)
            } else {
                *child
            };
            self.visit(expression, depth, statement);
        }
    }

    fn branch(&mut self, node: Node<'_>, depth: usize, statement: bool, chain: bool) {
        let strict = !self.assertion.ignore_guard_clauses;
        let counted = !chain && (strict || (statement && !guard(node, &self.source.text)));
        let next = if counted {
            self.enter(node, depth, "if")
        } else {
            depth
        };
        if let Some(condition) = node.child_by_field_name("condition") {
            self.visit(condition, if strict { next } else { depth }, false);
        }
        if let Some(body) = node.child_by_field_name("consequence") {
            self.visit(body, next, statement);
        }
        let Some(alternative) = node.child_by_field_name("alternative") else {
            return;
        };
        for child in children(alternative) {
            if child.kind() == "if_expression" {
                self.branch(child, next, strict, true);
            } else {
                self.visit(child, next, statement);
            }
        }
    }

    fn matching(&mut self, node: Node<'_>, depth: usize, statement: bool) {
        let strict = !self.assertion.ignore_guard_clauses;
        let next = if strict || statement {
            self.enter(node, depth, "match")
        } else {
            depth
        };
        if let Some(value) = node.child_by_field_name("value") {
            self.visit(value, if strict { next } else { depth }, false);
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        for arm in children(body) {
            for child in children(arm) {
                let value = arm.child_by_field_name("value") == Some(child);
                self.visit(child, next, value && statement);
            }
        }
    }

    fn looping(&mut self, node: Node<'_>, depth: usize) {
        let label = match node.kind() {
            "for_expression" => "for",
            "while_expression" => "while",
            _ => "loop",
        };
        let next = self.enter(node, depth, label);
        for child in children(node) {
            let body = node.child_by_field_name("body") == Some(child);
            self.visit(
                child,
                if body || !self.assertion.ignore_guard_clauses {
                    next
                } else {
                    depth
                },
                body,
            );
        }
    }

    fn closure(&mut self, node: Node<'_>, depth: usize) {
        if let Some(body) = node.child_by_field_name("body") {
            let strict = !self.assertion.ignore_guard_clauses;
            let braced = body.kind() == "block";
            let next = if strict {
                self.enter(node, depth, "closure")
            } else {
                depth + usize::from(braced)
            };
            self.visit(body, next, braced);
        }
    }
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| {
            !matches!(
                child.kind(),
                "line_comment" | "block_comment" | "attribute_item" | "inner_attribute_item"
            )
        })
        .collect()
}

fn guard(node: Node<'_>, text: &str) -> bool {
    node.child_by_field_name("consequence")
        .is_some_and(|body| diverging(body, text))
        && node
            .child_by_field_name("alternative")
            .is_none_or(|alternative| diverging(alternative, text))
}

fn diverging(node: Node<'_>, text: &str) -> bool {
    match node.kind() {
        "return_expression" | "break_expression" | "continue_expression" => true,
        "block" | "expression_statement" | "else_clause" => children(node)
            .last()
            .is_some_and(|child| diverging(*child, text)),
        "if_expression" => node.child_by_field_name("alternative").is_some() && guard(node, text),
        "match_expression" => node.child_by_field_name("body").is_some_and(|body| {
            let arms = children(body);
            !arms.is_empty()
                && arms.iter().all(|arm| {
                    arm.child_by_field_name("value")
                        .is_some_and(|value| diverging(value, text))
                })
        }),
        "macro_invocation" => node.child_by_field_name("macro").is_some_and(|name| {
            matches!(
                text[name.byte_range()].rsplit("::").next(),
                Some("panic" | "unreachable" | "todo" | "unimplemented")
            )
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn findings(source: &str) -> Vec<Finding> {
        configured(source, "")
    }
    fn configured(source: &str, options: &str) -> Vec<Finding> {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/nesting\"]]\ntarget='**/*.rs'\n{options}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<NestingRule>()
            .unwrap()
            .check(root.path())
            .unwrap()
            .findings
    }
    #[test]
    fn reports_nested_benchmark_orchestration_at_the_deepest_construct() {
        let findings = findings(
            r#"fn report(cases: &[u8]) {
    for case in cases {
        for sample in 0..5 {
            match sample {
                0 => println!("{case}"),
                _ => println!("{sample}"),
            }
        }
    }
}"#,
        );

        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("`report`"));
        assert!(findings[0].message.contains("line 4"));
        assert!(findings[0].message.contains("maximum is 2"));
        assert!(findings[0].message.contains("depth 3"));
    }

    #[test]
    fn accepts_flat_match_arms_and_declarative_result_data() {
        let findings = findings(
            r#"struct ResultRow { name: &'static str, samples: &'static [u64] }
fn summarize(row: &ResultRow) {
    match row.samples.first() {
        Some(value) => println!("{} {value}", row.name),
        None => println!("{} has no samples", row.name),
    }
}"#,
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn accepts_predicate_closures_inside_conditions() {
        let findings = findings(
            r#"fn validate(options: &mut Vec<String>) -> bool {
    for name in ["ro", "readonly"] {
        if options.iter().any(|option| option == name) {
            return false;
        }
    }
    true
}"#,
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn reports_control_flow_inside_a_braced_closure_body() {
        let findings = findings(
            r#"fn schedule(cases: &[u8]) {
    for case in cases {
        cases.iter().for_each(|other| {
            if other > case {
                println!("{other}");
            }
        });
    }
}"#,
        );

        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("depth 3"));
    }

    #[test]
    fn accepts_a_scrutinee_call_beside_a_nested_match() {
        let findings = findings(
            r"fn classify(values: &[u8]) -> u8 {
    match values.iter().max_by_key(|value| **value) {
        Some(value) => match value {
            0 => 1,
            other => *other,
        },
        None => 0,
    }
}",
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn accepts_a_branch_that_produces_a_bound_value() {
        let findings = findings(
            r"fn round(control: u16, truncate: bool, value: f64) -> f64 {
    for _ in 0..2 {
        let rounded = if truncate {
            value.trunc()
        } else {
            match control >> 10 & 3 {
                0 => value.round(),
                _ => value.floor(),
            }
        };
        if rounded > value {
            return rounded;
        }
    }
    value
}",
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn reports_a_branch_that_stands_as_a_statement() {
        let findings = findings(
            r"fn drain(rows: &[u8], other: &[u8]) {
    for row in rows {
        for column in other {
            if row > column {
                println!();
            }
        }
    }
}",
        );

        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("depth 3"));
    }

    #[test]
    fn accepts_a_branch_inside_an_arm_of_a_value_match() {
        let findings = findings(
            r"fn width(kind: u8, wide: bool) -> u8 {
    for _ in 0..2 {
        let size = match kind {
            0 => if wide { 8 } else { 4 },
            _ => 1,
        };
        if size > 4 {
            return size;
        }
    }
    0
}",
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn keeps_an_else_if_chain_on_one_level() {
        let findings = findings(
            r"fn select(first: u8, second: u8, third: u8) {
    for _ in 0..2 {
        if first > 0 {
            println!();
        } else if second > 0 {
            println!();
        } else if third > 0 {
            println!();
        }
    }
}",
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn accepts_a_guard_clause_that_leaves_the_enclosing_block() {
        let findings = findings(
            r#"fn resolve(components: &[u8]) -> Result<u8, &'static str> {
    let mut resolved = vec![];
    for component in components {
        match component {
            0 => {}
            1 => {
                if resolved.pop().is_none() {
                    return Err("escapes the root");
                }
            }
            other => resolved.push(*other),
        }
    }
    Ok(resolved.len() as u8)
}"#,
        );

        assert!(findings.is_empty(), "{:?}", findings[0].message);
    }

    #[test]
    fn accepts_guards_that_continue_break_or_panic() {
        for tail in ["continue", "break", "panic!()"] {
            let findings = findings(&format!(
                "fn walk(rows: &[u8], other: &[u8]) {{
    for row in rows {{
        for column in other {{
            if row > column {{
                {tail};
            }}
            println!();
        }}
    }}
}}"
            ));

            assert!(findings.is_empty(), "{tail} was charged a level");
        }
    }

    #[test]
    fn accepts_a_guard_whose_else_also_leaves_the_block() {
        let findings = findings(
            r"fn pick(rows: &[u8], other: &[u8]) -> u8 {
    for row in rows {
        for column in other {
            if row > column {
                return *row;
            } else {
                return *column;
            }
        }
    }
    0
}",
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn reports_a_branch_that_rejoins_the_enclosing_block() {
        let findings = findings(
            r"fn tally(rows: &[u8], other: &[u8]) -> u8 {
    let mut total = 0;
    for row in rows {
        for column in other {
            if row > column {
                total += 1;
            }
        }
    }
    total
}",
        );

        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("depth 3"));
    }

    #[test]
    fn reports_nesting_inside_a_guard_body() {
        let findings = findings(
            r"fn scan(rows: &[u8], other: &[u8]) -> u8 {
    for row in rows {
        if row > &0 {
            for column in other {
                for cell in other {
                    if cell > column {
                        println!();
                    }
                }
            }
            return *row;
        }
    }
    0
}",
        );

        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].message.contains("depth 4"),
            "{}",
            findings[0].message
        );
    }

    #[test]
    fn accepts_a_straight_line_braced_closure_under_two_branches() {
        let findings = findings(
            r#"fn record(rows: &[u8]) {
    for row in rows {
        if row > &0 {
            rows.iter().for_each(|other| {
                let label = format!("{other}");
                println!("{label}");
            });
        }
    }
}"#,
        );

        assert!(
            findings.is_empty(),
            "{:?}",
            findings.first().map(|f| &f.message)
        );
    }

    #[test]
    fn still_charges_the_closure_that_carries_a_third_branch() {
        let findings = findings(
            r#"fn record(rows: &[u8]) {
    for row in rows {
        if row > &0 {
            rows.iter().for_each(|other| {
                if other > row {
                    println!("{other}");
                }
            });
        }
    }
}"#,
        );

        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].message.contains("depth 4"),
            "{}",
            findings[0].message
        );
    }

    #[test]
    fn accepts_a_braced_arm_whose_tail_produces_the_value() {
        let findings = findings(
            r"fn shift(kind: u8, wide: bool) -> u64 {
    let mut total = 0;
    for half in 0..2 {
        for offset in 0..4 {
            let size = match kind {
                0 => {
                    let doubled = wide;
                    if doubled { 8 } else { 4 }
                }
                _ => 1,
            };
            total += size + half + offset;
        }
    }
    total
}",
        );

        assert!(
            findings.is_empty(),
            "{:?}",
            findings.first().map(|f| &f.message)
        );
    }

    #[test]
    fn accepts_a_braced_else_that_produces_the_value() {
        let findings = findings(
            r"fn shift(kind: u8, wide: bool) -> u64 {
    let mut total = 0;
    for half in 0..2 {
        for offset in 0..4 {
            let size = if kind == 0 {
                1
            } else {
                let doubled = wide;
                if doubled { 8 } else { 4 }
            };
            total += size + half + offset;
        }
    }
    total
}",
        );

        assert!(
            findings.is_empty(),
            "{:?}",
            findings.first().map(|f| &f.message)
        );
    }

    #[test]
    fn still_reports_a_statement_before_the_tail_of_a_value_block() {
        let findings = findings(
            r"fn shift(kind: u8, wide: bool) -> u64 {
    let mut total = 0;
    for half in 0..2 {
        for offset in 0..4 {
            let size = match kind {
                0 => {
                    if wide {
                        total += 1;
                    }
                    4
                }
                _ => 1,
            };
            total += size + half + offset;
        }
    }
    total
}",
        );

        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].message.contains("depth 3"),
            "{}",
            findings[0].message
        );
    }

    #[test]
    fn ignores_inline_test_harness_nesting() {
        let findings = findings(
            r"#[cfg(test)] mod tests {
    #[test] fn exhaustive_fixture() {
        for a in 0..2 { for b in 0..2 { match (a, b) { _ => {} } } }
    }
}",
        );
        assert!(findings.is_empty());
    }
    #[test]
    fn strict_mode_preserves_structural_branches_closures_and_tests() {
        let source = "fn shallow(value: bool) { if value {} else if value {} else if value {} } fn deep(value: bool) { if value { for _ in 0..1 { match value { true => {}, false => {} } } } }";
        assert_eq!(configured(source, "ignore_guard_clauses=false").len(), 1);
        let source = "fn compose(value: bool) { let _ = || async { if value {} }; } #[test] fn scenario() { if true { while false { loop { break; } } } }";
        assert_eq!(
            configured(source, "ignore_guard_clauses=false\nscope='all'").len(),
            2
        );
        assert_eq!(
            configured(source, "ignore_guard_clauses=false\nscope='tests'").len(),
            1
        );
    }

    #[test]
    fn all_exit_paths_are_guards_but_rejoining_else_is_not() {
        for exit in [
            "return",
            "break",
            "continue",
            "panic!()",
            "unreachable!()",
            "todo!()",
            "unimplemented!()",
            "if true { return; } else { return; }",
            "match true { true => return, false => return }",
        ] {
            let source = format!("fn walk() {{ loop {{ loop {{ if true {{ {exit} }} }} }} }}");
            let found = findings(&source);
            if exit.starts_with("match ") {
                assert_eq!(found.len(), 1);
                assert!(found[0].message.contains("depth 3"));
            } else {
                assert!(found.is_empty(), "{exit}");
            }
            assert_eq!(configured(&source, "ignore_guard_clauses=false").len(), 1);
        }
        assert_eq!(
            findings("fn walk() { loop { loop { if true { return; } else { work(); } } } }").len(),
            1
        );
    }

    #[test]
    fn configuration_targets_thresholds_and_test_exclusions() {
        let source = "fn work() { loop { loop { loop {} } } } #[cfg(test)] fn test_work() { loop { loop { loop {} } } }";
        assert!(configured(source, "max_depth=3").is_empty());
        assert_eq!(findings(source).len(), 1);
        assert_eq!(configured(source, "scope='tests'").len(), 1);
        assert!(configured(source, "exclude='lib.rs'").is_empty());
        assert!(findings("").is_empty());
        for options in [
            "max_depth=0",
            "max_depth=-1",
            "scope='unknown'",
            "ignore_guard_clauses='yes'",
            "maximum=2",
            "exclude=[]",
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/nesting\"]]\ntarget='**/*.rs'\n{options}"),
            )
            .unwrap();
            let registry = linter::Registry::default()
                .register::<NestingRule>()
                .unwrap();
            assert!(
                matches!(registry.check(root.path()), Err(Error::Configuration(_))),
                "{options}"
            );
        }
    }

    #[test]
    fn implementation_obeys_its_own_budget() {
        let found = findings(include_str!("mod.rs"));
        assert!(
            found.is_empty(),
            "{:?}",
            found
                .iter()
                .map(|finding| &finding.message)
                .collect::<Vec<_>>()
        );
    }
}
