use crate::{Finding, Policy, test_support::Fixture};

use super::{ID, check};

fn findings(source: &str, rule: &str) -> Vec<Finding> {
    assert_eq!(rule, ID);
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    check(&fixture.workspace(), &Policy::default()).expect("check fixture")
}

#[test]
fn nesting_rule_counts_structural_depth_but_not_else_if_as_two_levels() {
    let values = findings(
        r#"
fn shallow(value: bool) {
if value {} else if value {} else if value {}
}
fn deep(value: bool) {
if value { for _ in 0..1 { match value { true => {}, false => {} } } }
}
"#,
        "deep-control-flow",
    );
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].subject, "deep");
}

#[test]
fn counts_closures_async_blocks_and_test_bodies() {
    let values = findings(
        r#"
fn compose(value: bool) { let _ = || async { if value {} }; }
#[test] fn scenario() { if true { while false { loop { break; } } } }
"#,
        ID,
    );
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].subject, "compose");
    assert_eq!(values[1].subject, "scenario");
    assert!(
        values
            .iter()
            .all(|value| value.severity == crate::Severity::Warning)
    );
    assert!(values.iter().all(|value| value.related.len() == 1));
}
