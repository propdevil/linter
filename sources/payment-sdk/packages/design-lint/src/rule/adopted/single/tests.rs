use crate::{Finding, Policy, test_support::Fixture};

use super::{ID, check};

fn findings(source: &str, rule: &str) -> Vec<Finding> {
    assert_eq!(rule, ID);
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    check(&fixture.workspace(), &Policy::default()).expect("check fixture")
}

#[test]
fn single_use_rule_requires_sdk_exception_for_visual_boundary() {
    let values = findings(
        r#"
fn main() { once(); visual(); public(); ffi(); twice(); twice(); }
fn once() {}
pub fn public() {}
extern "C" fn ffi() {}
// hl-lint: visual-section
fn visual() {}
fn twice() {}
#[test] fn test_only() {}
#[cfg(test)] mod tests { fn fixture() {} }
"#,
        "single-use-free-function",
    );
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].subject, "once");
    assert_eq!(values[0].related.len(), 1);
    assert_eq!(values[1].subject, "visual");
}

#[test]
fn ambiguous_names_and_recursive_calls_do_not_create_false_single_use() {
    let fixture = Fixture::new(&[
        ("src/first.rs", "fn parse() {} fn run() { parse(); }"),
        ("src/second.rs", "fn parse() {} fn run() { parse(); }"),
        ("src/third.rs", "fn recursive() { recursive(); }"),
    ]);
    let values = check(&fixture.workspace(), &Policy::default()).expect("check fixture");
    assert!(values.is_empty());
}

#[test]
fn callback_reference_is_reported_with_context() {
    let values = findings("fn decode() {} fn run() { register(decode); }", ID);
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].subject, "decode");
    assert_eq!(values[0].severity, crate::Severity::Warning);
    assert!(values[0].related[0].label.contains("run"));
}

#[test]
fn deliberate_section_boundary_uses_reasoned_sdk_exception() {
    let fixture = Fixture::new(&[(
        "src/lib.rs",
        "fn main() { section(); }\n// design-lint: allow single-use-free-function -- deliberate semantic section in the startup sequence\nfn section() {}\n",
    )]);
    let registry = crate::Registry::new().register(super::SingleUse);
    let summaries = crate::Linter::new(Policy::default(), registry)
        .run(
            vec![fixture.path().to_owned()],
            &mut crate::Diagnostic::new(Vec::new()),
        )
        .expect("lint fixture");
    assert_eq!(summaries[0].findings, 0);
}
