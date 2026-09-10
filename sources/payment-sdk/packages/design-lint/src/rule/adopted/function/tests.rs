use crate::{Finding, Policy, test_support::Fixture};

use super::{ID, check};

fn findings(source: &str, rule: &str) -> Vec<Finding> {
    assert_eq!(rule, ID);
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    check(&fixture.workspace(), &Policy::default()).expect("check fixture")
}

#[test]
fn free_function_rule_preserves_scope_but_does_not_honor_donor_annotations() {
    let values = findings(
        r#"
fn zero() {}
fn one(value: usize) { let _ = value; }
fn two(left: usize, right: usize) { let _ = (left, right); }
fn three(a: usize, b: usize, c: usize) { let _ = (a, b, c); }
extern "C" fn ffi(value: usize) { let _ = value; }
#[hl_design::adapter] async fn handler(State(state): State<AppState>) { let _ = state; }
async fn unreviewed_handler(State(state): State<AppState>) { let _ = state; }
fn detached(state: AppState) { let _ = state; }
#[cfg(test)] fn test_only(value: usize) { let _ = value; }
#[hl_design::classify(pkg)] fn package(value: usize) { let _ = value; }
#[hl_design::classify(domain = "gpu")] fn domain(value: usize) { let _ = value; }
#[hl_design::classify(domain = "")] fn malformed(value: usize) { let _ = value; }
"#,
        "unclassified-free-function",
    );
    assert_eq!(
        values
            .iter()
            .map(|finding| finding.subject.as_str())
            .collect::<Vec<_>>(),
        [
            "one",
            "two",
            "handler",
            "unreviewed_handler",
            "detached",
            "package",
            "domain",
            "malformed"
        ]
    );
    assert!(values.iter().all(Finding::is_violation));
    assert!(values.iter().all(|finding| finding.review.is_some()));
}

#[test]
fn reports_framework_signature_as_evidence_without_exempting_it() {
    let values = findings(
        "async fn create(State(wallets): State<Wallets>) { wallets.generate().await; }",
        ID,
    );
    assert_eq!(values.len(), 1);
    let review = values[0].review.as_ref().expect("review");
    assert!(
        review
            .metadata
            .contains(&("Framework-shaped signature".into(), "true".into()))
    );
    assert!(review.dependencies.contains(&".generate".to_owned()));
    assert!(values[0].is_violation());
}

#[test]
fn proc_macros_and_nested_test_items_are_excluded() {
    let values = findings(
        r#"
#[proc_macro] pub fn derive(input: TokenStream) -> TokenStream { input }
#[proc_macro_attribute] pub fn decorate(attr: TokenStream, item: TokenStream) -> TokenStream { item }
#[test] fn sample() { fn nested(value: usize) {} }
fn parse(value: usize) {}
"#,
        ID,
    );
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].subject, "parse");
}

#[test]
fn ambiguous_names_keep_reference_evidence_in_the_same_file() {
    let fixture = Fixture::new(&[
        (
            "src/first.rs",
            "fn parse(value: usize) {} fn run() { parse(1); }",
        ),
        (
            "src/second.rs",
            "fn parse(value: usize) {} fn run() { parse(2); }",
        ),
    ]);
    let values = check(&fixture.workspace(), &Policy::default()).expect("check fixture");
    assert_eq!(values.len(), 2);
    for finding in values {
        assert_eq!(finding.related.len(), 1);
        assert_eq!(finding.related[0].location.path, finding.location.path);
        assert!(finding.review.expect("review").metadata.contains(&(
            "Usage resolution".into(),
            "ambiguous name; same-file references only".into()
        )));
    }
}

#[test]
fn thin_handler_can_have_a_reasoned_sdk_exception() {
    let fixture = Fixture::new(&[(
        "src/lib.rs",
        "// design-lint: allow unclassified-free-function -- Axum owns this extractor signature\nasync fn create(State(wallets): State<Wallets>) { wallets.generate().await; }\n",
    )]);
    let registry = crate::Registry::new().register(super::FreeFunction);
    let summaries = crate::Linter::new(Policy::default(), registry)
        .run(
            vec![fixture.path().to_owned()],
            &mut crate::Diagnostic::new(Vec::new()),
        )
        .expect("lint fixture");
    assert_eq!(summaries[0].findings, 0);
}

#[test]
fn nested_functions_in_test_only_impls_and_methods_are_excluded() {
    let values = findings(
        r#"
struct Sample;
#[cfg(test)] impl Sample { fn fixture() { fn nested(value: u8) {} } }
impl Sample {
    #[cfg(test)] fn helper() { fn nested_method(value: u8) {} }
    fn production() { fn retained(value: u8) {} }
}
"#,
        ID,
    );
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].subject, "retained");
}
