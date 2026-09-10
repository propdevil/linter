use crate::{Finding, Policy, test_support::Fixture};

use super::{ID, check};

fn findings(source: &str, rule: &str) -> Vec<Finding> {
    assert_eq!(rule, ID);
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    check(&fixture.workspace(), &Policy::default()).expect("check fixture")
}

#[test]
fn environment_rule_covers_std_calls_and_builtin_macros() {
    let values = findings(
        r#"
fn reads() {
let _ = std::env::var("A");
let _ = std::env::var_os("B");
let _ = std::env::vars();
let _ = std::env::vars_os();
let _ = env!("C");
let _ = option_env!("D");
}
"#,
        "environment-variable-access",
    );
    assert_eq!(values.len(), 6);
    assert!(values.iter().all(Finding::is_violation));
    assert!(values.iter().all(|finding| !finding.related.is_empty()));
}

fn findings_in(package_name: &str, source: &str, relative: &str) -> Vec<Finding> {
    let manifest = format!("[package]\nname = \"{package_name}\"\nversion = \"0.0.0\"\n");
    let fixture = Fixture::new(&[("Cargo.toml", &manifest), (relative, source)]);
    check(&fixture.workspace(), &Policy::default()).expect("check fixture")
}

fn environment_findings(source: &str, relative: &str) -> Vec<Finding> {
    findings_in("fixture", source, relative)
}

fn selected_findings(source: &str, relative: &str) -> Vec<Finding> {
    let fixture = Fixture::new(&[(relative, source)]);
    let policy: Policy = toml::from_str(&format!(
        "[boundaries]\nenvironment = [{{ path = {relative:?} }}]\n"
    ))
    .expect("selector policy");
    check(&fixture.workspace(), &policy).expect("check fixture")
}

#[test]
fn resolves_std_env_aliases_and_host_path_apis() {
    let values = environment_findings(
        r#"
use std::env::{self as process_environment, current_dir as cwd, var as read};
use std::env as host;
fn load() {
    let _ = read("A");
    let _ = process_environment::vars_os();
    let _ = host::current_exe();
    let _ = cwd();
    let _ = std::env::temp_dir();
}
"#,
        "src/lib.rs",
    );
    assert_eq!(values.len(), 5);
    assert!(values.iter().all(crate::Finding::is_violation));
}

#[test]
fn resolves_renamed_dirs_crate_without_matching_similar_names() {
    let values = environment_findings(
        r#"
use dirs as locations;
use dirs::config_dir as preferences;
fn load() {
    let _ = locations::home_dir();
    let _ = preferences();
    let _ = my_dirs::home_dir();
}
"#,
        "src/lib.rs",
    );
    assert_eq!(values.len(), 2);
}

#[test]
fn permits_test_scopes_and_explicitly_selected_sources() {
    for relative in ["src/adapter/host.rs", "build.rs"] {
        let text = "fn load() { let _ = std::env::var(\"A\"); }";
        assert_eq!(environment_findings(text, relative).len(), 1);
        assert!(selected_findings(text, relative).is_empty());
    }
    assert!(
        environment_findings(
            "#[test] fn load() { let _ = std::env::var(\"A\"); }",
            "src/lib.rs"
        )
        .is_empty()
    );
}

#[test]
fn role_words_outside_an_adapter_boundary_do_not_suppress_environment_findings() {
    for relative in ["src/domain/host.rs", "src/model/linux.rs"] {
        let values =
            environment_findings("fn load() { let _ = std::env::current_dir(); }", relative);
        assert_eq!(values.len(), 1, "{relative} must not be an adapter");
    }
    let values = findings_in(
        "ordinary-wgpu",
        "fn load() { let _ = std::env::var(\"A\"); }",
        "src/device.rs",
    );
    assert_eq!(values.len(), 1);
}

#[test]
fn role_names_need_an_explicit_source_selector() {
    for relative in [
        "src/adapter/wayland.rs",
        "src/adapters/macos.rs",
        "src/platform/linux.rs",
        "src/host.rs",
    ] {
        let text = "fn load() { let _ = std::env::current_exe(); }";
        assert_eq!(environment_findings(text, relative).len(), 1);
        assert!(selected_findings(text, relative).is_empty());
    }
}

#[test]
fn configuration_globals_require_semantic_evidence() {
    let values = environment_findings(
        r#"
use std::sync::{Mutex, OnceLock};
struct AppConfig;
struct State;
static CONFIG: OnceLock<AppConfig> = OnceLock::new();
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static LOCKS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static REGISTRY: OnceLock<Vec<String>> = OnceLock::new();
"#,
        "src/lib.rs",
    );
    assert_eq!(values.len(), 2);
    assert!(values.iter().any(|value| value.subject == "CONFIG"));
    assert!(values.iter().any(|value| value.subject == "STATE"));
}

#[test]
fn source_selector_does_not_authorize_a_sibling_with_a_common_prefix() {
    let fixture = Fixture::new(&[
        (
            "apps/api/src/config.rs",
            "fn load() { std::env::var(\"A\"); }",
        ),
        (
            "apps/api/src/configuration.rs",
            "fn load() { std::env::var(\"B\"); }",
        ),
    ]);
    let policy: Policy =
        toml::from_str("[boundaries]\nenvironment = [{ path = \"apps/api/src/config.rs\" }]")
            .expect("selector policy");
    let values = check(&fixture.workspace(), &policy).expect("check fixture");
    assert_eq!(values.len(), 1);
    assert!(values[0].location.path.ends_with("configuration.rs"));
}
