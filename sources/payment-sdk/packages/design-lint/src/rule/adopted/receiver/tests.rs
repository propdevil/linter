use crate::{Finding, Policy, test_support::Fixture};

use super::{ID, check};

fn findings(source: &str, rule: &str) -> Vec<Finding> {
    assert_eq!(rule, ID);
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    check(&fixture.workspace(), &Policy::default()).expect("check fixture")
}

#[test]
fn receiver_repetition_handles_traits_acronyms_versions_and_exclusions() {
    let values = findings(
        r#"
struct Directory;
impl Directory {
fn create_directory(&self) {}
fn directory_remove(&self) {}
fn create_file(&self) {}
fn from_directory(_: Directory) -> Self { Self }
fn into_directory(self) -> Directory { self }
fn try_into_directory(self) -> Result<Directory, ()> { Ok(self) }
fn try_again_directory(&self) {}
}

struct HTTPServerV2;
impl HTTPServerV2 {
fn restart_http_server_v2(&self) {}
fn restart_http_server(&self) {}
}

struct Id;
impl Id {
fn parse_id(&self) {}
}

trait Workspace {
fn remove_workspace(&self);
fn workspace_settings(&self);
}

trait Foreign {
fn remove_directory(&self);
}
impl Foreign for Directory {
fn remove_directory(&self) {}
}
"#,
        "receiver-name-repetition",
    );
    let subjects = values
        .iter()
        .map(|finding| finding.subject.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        subjects,
        [
            "Directory::create_directory",
            "Directory::directory_remove",
            "Directory::try_again_directory",
            "HTTPServerV2::restart_http_server_v2",
            "Workspace::remove_workspace",
            "Workspace::workspace_settings",
        ]
    );
    assert!(values.iter().all(|finding| finding.review.is_some()));
    assert_eq!(
        values[3].review.as_ref().unwrap().metadata[1].1,
        "http, server, v, 2"
    );
}

#[test]
fn test_only_receivers_are_excluded_without_hiding_later_production() {
    let values = findings(
        r#"
#[cfg(test)] mod checks { struct Wallet; impl Wallet { fn wallet_address(&self) {} } }
struct Wallet;
#[cfg(test)] impl Wallet { fn wallet_balance(&self) {} }
impl Wallet {
    #[cfg(test)] fn wallet_history(&self) {}
    fn wallet_address(&self) {}
}
"#,
        ID,
    );
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].subject, "Wallet::wallet_address");
    assert_eq!(values[0].severity, crate::Severity::Error);
    assert_eq!(values[0].location.line, 7);
}
