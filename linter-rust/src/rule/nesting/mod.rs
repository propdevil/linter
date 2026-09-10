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
            range: body.byte_range(),
        };
        depth.visit(body, 0, true);
        if depth.maximum > assertion.max_depth {
            findings.push(depth.finding(node, body));
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
    range: std::ops::Range<usize>,
}

impl Depth<'_> {
    fn finding(&self, node: Node<'_>, body: Node<'_>) -> Finding {
        let name = node
            .child_by_field_name("name")
            .map(|name| &self.source.text[name.byte_range()])
            .unwrap_or("<anonymous>");
        let mut related = vec![linter::Evidence {
            path: self.source.path.clone(),
            span: Some(linter::Span::new(&self.source.text, self.range.clone())),
            message: format!("Deepest {} reaches depth {}", self.construct, self.maximum),
        }];
        let instruction = self.guidance(node, body, &mut related);
        Finding {
            span: Some(linter::Span::new(&self.source.text, node.byte_range())),
            related,
            rule: NestingRule::ID,
            path: self.source.path.clone(),
            configuration: format!("{}.max_depth", self.assertion.setting),
            message: format!(
                "`{name}` reaches syntactic nesting depth {} at {} on line {}; maximum is {}",
                self.maximum, self.construct, self.line, self.assertion.max_depth
            ),
            instruction,
        }
    }

    fn guidance(
        &self,
        node: Node<'_>,
        body: Node<'_>,
        related: &mut Vec<linter::Evidence>,
    ) -> String {
        if let Some(condition) = terminal_condition(node, body, &self.range, &self.source.text) {
            related.push(linter::Evidence {
                path: self.source.path.clone(),
                span: Some(linter::Span::new(&self.source.text, condition.byte_range())),
                message: concat!(
                    "Final conditional in a unit-returning function; ",
                    "no later statements are skipped by an early return."
                )
                .into(),
            });
            concat!(
                "Consider inverting this final condition into an early-return guard, ",
                "then moving its body after the guard. ",
                "Keep condition evaluation and effects in their original order."
            )
            .into()
        } else {
            concat!(
                "Reduce dependent control-flow levels or extract a cohesive operation. ",
                "No semantics-preserving early-return rewrite has been established ",
                "for this finding."
            )
            .into()
        }
    }

    fn enter(&mut self, node: Node<'_>, depth: usize, construct: &'static str) -> usize {
        let next = depth + 1;
        if next > self.maximum {
            self.maximum = next;
            self.line = node.start_position().row + 1;
            self.construct = construct;
            self.range = node.byte_range();
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
        .is_some_and(|body| diverging(body, text, body))
        && node
            .child_by_field_name("alternative")
            .is_none_or(|alternative| diverging(alternative, text, alternative))
}

fn diverging(node: Node<'_>, text: &str, boundary: Node<'_>) -> bool {
    match node.kind() {
        "return_expression" => true,
        "break_expression" | "continue_expression" => escaping_jump(node, boundary, text),
        "block" | "expression_statement" | "else_clause" => children(node)
            .last()
            .is_some_and(|child| diverging(*child, text, boundary)),
        "if_expression" => {
            node.child_by_field_name("alternative")
                .is_some_and(|alternative| diverging(alternative, text, boundary))
                && node
                    .child_by_field_name("consequence")
                    .is_some_and(|body| diverging(body, text, boundary))
        }
        "match_expression" => node.child_by_field_name("body").is_some_and(|body| {
            let arms = children(body);
            !arms.is_empty()
                && arms.iter().all(|arm| {
                    arm.child_by_field_name("value")
                        .is_some_and(|value| diverging(value, text, boundary))
                })
        }),
        "macro_invocation" => standard_exit(node, text),
        _ => false,
    }
}

fn escaping_jump(node: Node<'_>, boundary: Node<'_>, text: &str) -> bool {
    let label = node
        .named_child(0)
        .filter(|child| matches!(child.kind(), "label" | "loop_label" | "lifetime"))
        .map(|label| text[label.byte_range()].trim_end_matches(':').to_owned());
    let mut parent = node.parent();
    while let Some(target) = parent {
        if matches!(
            target.kind(),
            "function_item" | "closure_expression" | "async_block"
        ) {
            return false;
        }
        let looping = matches!(
            target.kind(),
            "loop_expression" | "while_expression" | "for_expression"
        );
        let target_label = target
            .named_child(0)
            .filter(|child| matches!(child.kind(), "label" | "loop_label" | "lifetime"))
            .map(|label| text[label.byte_range()].trim_end_matches(':'));
        let matches = match &label {
            Some(label) => target_label == Some(label.as_str()),
            None => looping,
        };
        if matches {
            return (looping || node.kind() == "break_expression")
                && (target.start_byte() < boundary.start_byte()
                    || target.end_byte() > boundary.end_byte());
        }
        parent = target.parent();
    }
    false
}
fn standard_exit(node: Node<'_>, text: &str) -> bool {
    let Some(name) = node.child_by_field_name("macro") else {
        return false;
    };
    let path = text[name.byte_range()].trim_start_matches("::");
    let name = path.rsplit("::").next().unwrap_or_default();
    if !matches!(name, "panic" | "unreachable" | "todo" | "unimplemented") {
        return false;
    }
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    if path.contains("::") {
        let prefix = path.split("::").next().unwrap_or_default();
        return (path == format!("std::{name}") || path == format!("core::{name}"))
            && !macro_shadow(root, prefix, text);
    }
    !macro_shadow(root, name, text)
}
fn macro_shadow(node: Node<'_>, name: &str, text: &str) -> bool {
    if matches!(node.kind(), "macro_definition" | "mod_item")
        && node
            .child_by_field_name("name")
            .is_some_and(|value| &text[value.byte_range()] == name)
    {
        return true;
    }
    if matches!(node.kind(), "use_declaration" | "extern_crate_declaration") {
        let value = &text[node.byte_range()];
        if value.contains('*')
            || value
                .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                .any(|word| word == name)
        {
            return true;
        }
    }
    children(node)
        .into_iter()
        .any(|child| macro_shadow(child, name, text))
}
fn terminal_condition<'a>(
    function: Node<'a>,
    body: Node<'a>,
    deepest: &std::ops::Range<usize>,
    text: &str,
) -> Option<Node<'a>> {
    if function
        .child_by_field_name("return_type")
        .is_some_and(|ty| text[ty.byte_range()].trim() != "()")
    {
        return None;
    }
    let statement = *children(body).last()?;
    let mut tail = statement;
    if tail.kind() == "expression_statement" {
        tail = tail.named_child(0)?;
    }
    if tail.kind() != "if_expression"
        || tail.child_by_field_name("alternative").is_some()
        || deepest.start < tail.start_byte()
        || deepest.end > tail.end_byte()
    {
        return None;
    }
    let mut previous = statement.prev_named_sibling();
    while let Some(sibling) = previous {
        if sibling.kind() == "attribute_item" {
            return None;
        }
        if !matches!(sibling.kind(), "line_comment" | "block_comment") {
            break;
        }
        previous = sibling.prev_named_sibling();
    }
    let condition = tail.child_by_field_name("condition")?;
    (!matches!(condition.kind(), "let_condition" | "let_chain")).then_some(condition)
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
        let source = "fn shallow(value: bool) { if value {} else if value {} else if val\
            ue {} } fn deep(value: bool) { if value { for _ in 0..1 { match value { true\
            \u{20}=> {}, false => {} } } } }";
        assert_eq!(configured(source, "ignore_guard_clauses=false").len(), 1);
        let source = "fn compose(value: bool) { let _ = || async { if value {} }; } #[te\
            st] fn scenario() { if true { while false { loop { break; } } } }";
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
        let source = "fn work() { loop { loop { loop {} } } } #[cfg(test)] fn test_work(\
            ) { loop { loop { loop {} } } }";
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
    #[test]
    fn function_depth_ignores_module_and_impl_wrappers() {
        let body = "if a { if b { if c { work(); } } }";
        for source in [
            format!("fn run(){{{body}}}"),
            format!("struct Value; impl Value {{fn run(&self){{{body}}}}}"),
            [
                "mod outer { mod inner {struct Value; impl Value {fn run(&self){",
                body,
                "}}}}",
            ]
            .concat(),
        ] {
            let report = findings(&source);
            assert_eq!(report.len(), 1);
            assert!(report[0].message.contains("depth 3"));
        }
    }
    #[test]
    fn proven_terminal_condition_suggests_early_return() {
        let positive = findings("fn run(){if ready {if valid {if enabled {work();}}}}");
        assert_eq!(positive.len(), 1);
        assert!(
            positive[0]
                .instruction
                .contains("inverting this final condition")
        );
        assert_eq!(positive[0].related.len(), 2);
        assert!(
            findings("fn run(){if !ready {return;} if !valid {return;} if enabled {work();}}")
                .is_empty()
        );
        for source in [
            "fn run(){if ready {if valid {if enabled {work();}}} finish();}",
            "fn run()->u8{if ready {if valid {if enabled {work();}}} 0}",
            "fn run(){if let Some(value)=input {if valid {if enabled {work();}}}}",
        ] {
            let report = findings(source);
            assert_eq!(report.len(), 1);
            assert!(
                !report[0]
                    .instruction
                    .contains("inverting this final condition")
            );
        }
    }
    #[test]
    fn inner_labeled_break_rejoins_and_cannot_hide_nesting() {
        let report = findings("fn run(){if a {if b {if c {'local: {break 'local;}}}}}");
        assert_eq!(report.len(), 1);
        assert!(report[0].message.contains("depth 3"));
        assert!(findings("fn run(){'outer: loop {if a {if b {break 'outer;}}}}").is_empty());
        assert!(findings("fn run(){loop {if a {if b {continue;}}}}").is_empty());
    }
    #[test]
    fn closures_and_local_loops_do_not_exit_the_outer_branch() {
        for tail in [
            "let callback=||{return;};",
            "let future=async{return;};",
            "loop {break;}",
        ] {
            let source = format!("fn run(){{if a {{if b {{if c {{{tail}}}}}}}}}");
            assert!(!findings(&source).is_empty(), "{source}");
        }
    }
    #[test]
    fn exit_macros_need_known_identity() {
        assert!(!findings("fn run(){if a {if b {if c {custom::panic!();}}}}").is_empty());
        assert!(
            !findings("macro_rules! panic {()=>{()}} fn run(){if a {if b {if c {panic!();}}}}")
                .is_empty()
        );
        assert!(findings("fn run(){if a {if b {if c {std::panic!();}}}}").is_empty());
    }
}
