use crate::{Finding, Policy, test_support::Fixture};

use super::{ID, check};

fn findings(source: &str, rule: &str) -> Vec<Finding> {
    assert_eq!(rule, ID);
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    check(&fixture.workspace(), &Policy::default()).expect("check fixture")
}

#[test]
fn naming_rule_checks_structs_and_ignores_donor_exception_attributes() {
    let values = findings(
        r#"
struct Workspace;
struct Selected;
#[hl_design::naming(reason = "external protocol vocabulary")]
struct Updated;
struct VkImageCopy2;
enum Changed { Value }
type Chosen = usize;
struct Workspaces;
impl Workspaces {
fn workspace(&self, id: usize) { let _ = id; }
fn remove(&self, id: usize) { let _ = id; }
fn from_items(_: Vec<Workspace>) -> Self { Self }
}
"#,
        "struct-noun-naming",
    );
    assert_eq!(values.len(), 2);
    assert!(values.iter().any(|finding| finding.subject == "Selected"));
    assert!(!values.iter().any(|finding| finding.subject == "workspace"));
    assert!(values.iter().any(|finding| finding.subject == "Updated"));
    assert!(!values.iter().any(|finding| finding.subject == "remove"));
    assert!(!values.iter().any(|finding| finding.subject == "from_items"));
}

use super::{English, Language, has_noun, identifier_words};

#[test]
fn english_classifies_domain_nouns() {
    let english = English::new();
    assert!(english.noun("workspace"));
    assert!(english.noun("settings"));
    assert!(english.noun("archive"));
    assert!(english.noun("call"));
    assert!(english.noun("capture"));
    assert!(english.noun("mount"));
    assert!(english.noun("register"));
    assert!(!english.noun("selected"));
}

#[test]
fn versioned_compound_identifier_accepts_any_noun_token() {
    let english = English::new();
    assert_eq!(identifier_words("VkImageCopy2"), ["vk", "image", "copy"]);
    assert!(has_noun(&english, "VkImageCopy2"));
    assert!(!has_noun(&english, "Selected"));
}

#[test]
fn test_only_structs_do_not_hide_later_production() {
    let values = findings(
        r#"
#[cfg(test)] mod checks { struct Selected; }
#[cfg(test)] struct Updated;
#[test] fn example() { struct Chosen; }
struct Selected;
struct Wallet;
"#,
        ID,
    );
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].subject, "Selected");
    assert_eq!(values[0].location.line, 5);
    assert_eq!(values[0].severity, crate::Severity::Error);
}

#[test]
fn reasoned_sdk_exception_retains_protocol_vocabulary() {
    let fixture = Fixture::new(&[(
        "src/lib.rs",
        "// design-lint: allow struct-noun-naming -- external protocol vocabulary\nstruct Selected;\n",
    )]);
    let registry = crate::Registry::new().register(super::StructNaming);
    let summaries = crate::Linter::new(Policy::default(), registry)
        .run(
            vec![fixture.path().to_owned()],
            &mut crate::Diagnostic::new(Vec::new()),
        )
        .expect("lint fixture");
    assert_eq!(summaries[0].findings, 0);
}
