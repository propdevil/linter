use crate::{Finding, Policy, test_support::Fixture};

use super::{ID, check};

fn findings(source: &str, rule: &str) -> Vec<Finding> {
    assert_eq!(rule, ID);
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    check(&fixture.workspace(), &Policy::default()).expect("check fixture")
}

#[test]
fn platform_command_rule_resolves_qualified_grouped_and_renamed_imports() {
    let values = findings(
        r#"
use std::process::Command;
use std::process as host_process;
use tokio::{process::Command as AsyncCommand};
fn commands() {
let _ = Command::new("git");
let _ = host_process::Command::new("git");
let _ = AsyncCommand::new("git");
let _ = std::process::Command::new("git");
let _ = tokio::process::Command::new("git");
}
"#,
        "platform-command-boundary",
    );
    assert_eq!(values.len(), 5);
    assert!(values.iter().all(Finding::is_violation));
    assert!(
        values
            .iter()
            .all(|finding| finding.message.contains("outside an application"))
    );
}

#[test]
fn platform_command_rule_distinguishes_guest_process_models() {
    let values = findings(
        r#"
struct Process;
impl Process { fn new(_: &str) -> Self { Self } }
fn guest() {
let _ = Process::new("/bin/sh");
let _ = guest_engine::Process::new("/bin/bash");
}
"#,
        "platform-command-boundary",
    );
    assert!(values.is_empty());
}

#[test]
fn platform_command_rule_module_names_do_not_authorize_process_access() {
    let values = findings(
        r#"
mod adapters {
use std::process::Command as HostCommand;
fn run() { let _ = HostCommand::new("git"); }
}
mod model {
fn run() { let _ = std::process::Command::new("git"); }
}
"#,
        "platform-command-boundary",
    );
    assert_eq!(values.len(), 2);
    assert!(values[1].location.source.contains("std::process::Command"));
}

#[test]
fn platform_command_rule_rejects_interpolated_shell_source() {
    let values = findings(
        r#"
fn unsafe_shell(value: &str) {
let _ = std::process::Command::new("/bin/sh")
    .arg("-c")
    .arg(format!("echo {value}"));
let _ = tokio::process::Command::new("bash")
    .args(["-c", value]);
}
"#,
        "platform-command-boundary",
    );
    assert_eq!(values.len(), 4);
    assert_eq!(
        values
            .iter()
            .filter(|finding| finding.subject.contains("interpolated script"))
            .count(),
        2
    );
}

#[test]
fn platform_command_rule_allows_cfg_test_commands() {
    let values = findings(
        r#"
#[cfg(test)]
mod tests {
fn fixture() { let _ = std::process::Command::new("git"); }
}
#[test]
fn test_fixture() { let _ = tokio::process::Command::new("git"); }
"#,
        "platform-command-boundary",
    );
    assert!(values.is_empty());
}

#[test]
fn selected_build_script_allows_commands_but_not_dynamic_shells() {
    let fixture = Fixture::new(&[(
        "build.rs",
        r#"
fn main() {
    let _ = std::process::Command::new("cc").arg("--version").status();
    let _ = std::process::Command::new("sh").arg("-c").arg(concat!("echo ", "static")).status();
    let value = "untrusted";
    let _ = std::process::Command::new("sh").arg("-c").arg(value).status();
}
"#,
    )]);
    let policy: Policy = toml::from_str("[boundaries]\nprocess = [{ path = \"build.rs\" }]")
        .expect("selector policy");
    let values = check(&fixture.workspace(), &policy).expect("check fixture");
    assert_eq!(values.len(), 1);
    assert!(values[0].subject.contains("interpolated script"));
}

#[test]
fn tracks_staged_shell_builders_with_lexical_scope() {
    let fixture = Fixture::new(&[(
        "build.rs",
        r#"
use std::process::Command as HostCommand;
fn unsafe_build(value: &str) {
    let mut command = HostCommand::new("/bin/sh");
    command.arg("-c");
    command.arg(format!("echo {value}"));
    let mut alias = command;
    alias.args(["-c", value]);
}
fn safe_build() {
    let mut command = HostCommand::new("sh");
    command.arg("-c");
    command.arg(concat!("echo ", "static"));
}
fn unrelated(value: &str) {
    struct Builder;
    impl Builder { fn arg(&mut self, _: &str) {} }
    let mut command = Builder;
    command.arg("-c");
    command.arg(value);
}
"#,
    )]);
    let policy: Policy = toml::from_str("[boundaries]\nprocess = [{ path = \"build.rs\" }]")
        .expect("selector policy");
    let values = check(&fixture.workspace(), &policy).expect("check fixture");
    assert_eq!(values.len(), 2);
    assert!(
        values
            .iter()
            .all(|finding| finding.message.contains("staged shell"))
    );
}

#[test]
fn dynamic_shells_remain_errors_inside_test_scopes() {
    let values = findings(
        r#"
#[cfg(test)] mod tests {
    #[test] fn scenario() { std::process::Command::new("sh").arg("-c").arg(format!("echo {}", 1)); }
}
"#,
        ID,
    );
    assert_eq!(values.len(), 1);
    assert!(values[0].subject.contains("interpolated script"));
}

#[test]
fn a_selected_package_does_not_authorize_other_packages() {
    let fixture = Fixture::new(&[
        (
            "tools/Cargo.toml",
            "[package]\nname = \"fixture-tools\"\nversion = \"0.0.0\"",
        ),
        (
            "tools/src/lib.rs",
            "fn run() { std::process::Command::new(\"git\"); }",
        ),
        (
            "sdk/Cargo.toml",
            "[package]\nname = \"fixture-sdk\"\nversion = \"0.0.0\"",
        ),
        (
            "sdk/src/lib.rs",
            "fn run() { std::process::Command::new(\"git\"); }",
        ),
    ]);
    let policy: Policy =
        toml::from_str("[boundaries]\nprocess = [{ package = \"fixture-tools\" }]")
            .expect("selector policy");
    let values = check(&fixture.workspace(), &policy).expect("check fixture");
    assert_eq!(values.len(), 1);
    assert!(values[0].location.path.ends_with("sdk/src/lib.rs"));
}
