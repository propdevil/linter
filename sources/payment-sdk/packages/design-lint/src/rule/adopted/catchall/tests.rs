use crate::{Finding, Policy, test_support::Fixture};

use super::{ID, check};

fn findings(source: &str, rule: &str) -> Vec<Finding> {
    assert_eq!(rule, ID);
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    check(&fixture.workspace(), &Policy::default()).expect("check fixture")
}

#[test]
fn catch_all_module_rule_checks_rust_module_identity_only() {
    let values = findings(
        r#"
mod util {}
mod r#common {}
mod utility {}
fn helper() {}
struct SharedState;
use external_crate::core;
const PROSE: &str = "mod misc {}";
"#,
        "catch-all-module-name",
    );
    assert_eq!(
        values
            .iter()
            .map(|finding| finding.subject.as_str())
            .collect::<Vec<_>>(),
        ["util", "common"]
    );
    assert!(values.iter().all(Finding::is_violation));
    assert!(values.iter().all(|finding| {
        finding.review.as_ref().is_some_and(|review| {
            review
                .metadata
                .iter()
                .any(|(key, value)| key == "declaration" && value == "inline module declaration")
        })
    }));
}

#[test]
fn maps_files_directories_and_external_modules() {
    let fixture = Fixture::new(&[
        ("src/lib.rs", "mod common;\nmod helper;\n"),
        ("src/common/mod.rs", "pub struct Fixture;\n"),
        ("src/helper.rs", "pub struct Value;\n"),
        (
            "src/path_root.rs",
            "#[path = \"fixture/shared.rs\"] mod shared;\n",
        ),
        ("src/fixture/shared.rs", "pub struct Input;\n"),
    ]);
    let values = check(&fixture.workspace(), &Policy::default()).expect("check fixture");
    let subjects: Vec<_> = values
        .iter()
        .map(|finding| finding.subject.as_str())
        .collect();
    assert_eq!(subjects, ["common", "shared", "helper"]);
    assert!(values.iter().all(|finding| finding.location.line == 1));
    assert!(values.iter().all(|finding| {
        finding
            .review
            .as_ref()
            .expect("review")
            .metadata
            .contains(&("scope".to_owned(), "production".to_owned()))
    }));
}

#[test]
fn does_not_exempt_test_support() {
    let fixture = Fixture::new(&[
        ("tests/common/mod.rs", "pub struct Harness;\n"),
        ("tests/workflow.rs", "mod common;\n"),
    ]);
    let values = check(&fixture.workspace(), &Policy::default()).expect("check fixture");
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].subject, "common");
    assert!(
        values[0]
            .review
            .as_ref()
            .expect("review")
            .metadata
            .contains(&("scope".to_owned(), "test".to_owned()))
    );
}

#[test]
fn path_override_uses_declared_module_identity() {
    let fixture = Fixture::new(&[
        (
            "src/lib.rs",
            "#[path = \"entities.rs\"] mod common;\n#[path = \"helpers.rs\"] mod transactions;",
        ),
        ("src/entities.rs", "pub struct Entity;"),
        ("src/helpers.rs", "pub struct Transaction;"),
    ]);
    let values = check(&fixture.workspace(), &Policy::default()).expect("check fixture");
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].subject, "common");
    assert!(values[0].location.path.ends_with("src/lib.rs"));
}

#[test]
fn configured_vocabulary_is_exact() {
    let fixture = Fixture::new(&[(
        "src/lib.rs",
        "mod util {} mod helper {} mod helper_domain {}",
    )]);
    let mut policy = Policy::default();
    policy.rust.forbidden_modules = vec!["helper".into()];
    let values = check(&fixture.workspace(), &policy).expect("check fixture");
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].subject, "helper");
}
