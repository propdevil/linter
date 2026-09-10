use crate::{
    Analysis,
    declaration::Index,
    scope::{integration, mark_tests},
};
use linter::{Error, Project, Rule, RuleResult, Status};
use std::fs;
mod config;
mod scan;
use config::Assertion;
pub use config::Config;

pub struct EnvironmentAccess(Vec<Assertion>);
impl Rule for EnvironmentAccess {
    const ID: &'static str = "rust/environment-variable-access";
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
        let index = Index::new(analysis, &root);
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
                    && !assertion
                        .allowed_targets
                        .as_ref()
                        .is_some_and(|allowed| allowed.matches(&source.path))
            }) {
                scan::inspect(source, &tests, assertion, &index, &mut findings);
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    const POLICY: &str = r#"
[[rules."rust/environment-variable-access"]]
target = "**/*.rs"
functions = ["std::env::var", "std::env::var_os", "std::env::vars", "std::env::vars_os", "std::env::set_var", "std::env::remove_var", "std::env::current_dir", "std::env::current_exe", "std::env::temp_dir", "dirs::home_dir", "dirs::config_dir"]
global_types = ["std::sync::OnceLock", "std::sync::LazyLock", "once_cell::sync::OnceCell", "once_cell::sync::Lazy"]
global_words = ["config", "configuration", "settings", "state"]
"#;
    fn report(source: &str, settings: &str, path: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("{POLICY}\n{settings}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<super::EnvironmentAccess>()
            .unwrap()
            .check(root.path())
            .unwrap()
    }
    fn findings(source: &str) -> Vec<linter::Finding> {
        report(source, "", "src/lib.rs").findings
    }
    #[test]
    fn resolves_runtime_apis_and_import_aliases() {
        let values = findings(
            r#"
use std::env::{self as process_environment, current_dir as cwd, var as read};
use std::env as host;
fn load() { read("A"); process_environment::vars_os(); host::current_exe(); cwd(); std::env::temp_dir(); }
"#,
        );
        assert_eq!(values.len(), 5);
        assert!(
            values
                .iter()
                .all(|finding| finding.span.is_some() && !finding.related.is_empty())
        );
        assert_eq!(findings("use dirs as locations; use dirs::config_dir as preferences; fn load() { locations::home_dir(); preferences(); my_dirs::home_dir(); }").len(), 2);
    }
    #[test]
    fn boundaries_are_explicit_and_do_not_match_sibling_prefixes() {
        let source = "fn load() { std::env::var(\"A\"); }";
        for path in [
            "build.rs",
            "src/main.rs",
            "src/adapter/host.rs",
            "src/adapters/linux.rs",
            "src/platform/linux.rs",
            "src/host.rs",
            "src/unix/spawn.rs",
            "apps/fixture/src/paths.rs",
        ] {
            assert_eq!(report(source, "", path).findings.len(), 1, "{path}");
            assert!(
                report(source, &format!("allowed_targets = '{path}'"), path)
                    .findings
                    .is_empty()
            );
        }
        for path in [
            "src/domain/host.rs",
            "src/model/linux.rs",
            "src/spawn/unix.rs",
            "src/platform_extra/device.rs",
        ] {
            assert_eq!(
                report(source, "allowed_modules = ['platform']", path)
                    .findings
                    .len(),
                1,
                "{path}"
            );
        }
        assert!(
            report(
                source,
                "allowed_modules = ['platform']",
                "src/platform/device.rs"
            )
            .findings
            .is_empty()
        );
        assert_eq!(report("mod platform { fn load() { std::env::var(\"A\"); } } mod platform_extra { fn load() { std::env::var(\"B\"); } }", "allowed_modules = ['platform']", "src/lib.rs").findings.len(), 1);
    }
    #[test]
    fn globals_need_proven_lazy_type_and_complete_semantic_words() {
        let source = "use std::sync::{Mutex, OnceLock}; struct AppConfig; struct State; static CONFIG: OnceLock<AppConfig> = OnceLock::new(); static STATE: OnceLock<Mutex<State>> = OnceLock::new(); static LOCKS: OnceLock<Mutex<Vec<String>>> = OnceLock::new(); static REGISTRY: OnceLock<Vec<String>> = OnceLock::new(); static RECONFIGURE: OnceLock<String> = OnceLock::new();";
        let values = findings(source);
        assert_eq!(values.len(), 2);
        assert!(
            values
                .iter()
                .any(|finding| finding.message.contains("CONFIG"))
        );
        assert!(
            findings(
                "struct OnceLock<T>(T); struct Config; static CONFIG: OnceLock<Config> = build();"
            )
            .is_empty()
        );
        assert_eq!(findings("use std::sync::OnceLock as Cell; struct Settings; static STORAGE: Cell<Settings> = Cell::new();").len(), 1);
    }
    #[test]
    fn compile_time_metadata_is_an_explicit_policy_choice() {
        let source = r#"fn reads() { std::env::var("A"); std::env::var_os("B"); std::env::vars(); std::env::vars_os(); env!("C"); option_env!("D"); }"#;
        assert_eq!(findings(source).len(), 4);
        assert_eq!(
            report(source, "macros = ['env', 'option_env']", "src/lib.rs")
                .findings
                .len(),
            6
        );
        assert!(findings(r#"fn identity() { option_env!("BUILD_ID"); std::path::Path::new(env!("CARGO_MANIFEST_DIR")); env!("CARGO_PKG_VERSION"); }"#).is_empty());
        assert!(
            report(
                "macro_rules! env { () => { 1 }; } fn value() { env!(); }",
                "macros = ['env']",
                "src/lib.rs"
            )
            .findings
            .is_empty()
        );
    }
    #[test]
    fn unknown_shadowed_names_and_strings_are_not_calls() {
        for source in [
            "use std::env::var; fn run(var: fn()) { var(); }",
            "use std::env::var; fn run() { let var = || {}; var(); }",
            "fn var() {} fn run() { var(); }",
            "mod std { pub mod env { pub fn var() {} } } fn run() { std::env::var(); }",
            "use std::env::var; fn run() { { let var = || {}; var(); } }",
            r###"fn run() { let _ = r#"std::env::var(\"X\")"#; /* std::env::var("X"); */ }"###,
        ] {
            assert!(findings(source).is_empty(), "{source}");
        }
        assert_eq!(
            findings("use std::env::var; fn run() { { let var = || {}; var(); } var(\"X\"); }")
                .len(),
            1
        );
    }
    #[test]
    fn scopes_exclusions_and_directives_work_through_registry() {
        let source = "#[test] fn fixture() { std::env::var(\"A\"); }";
        assert!(findings(source).is_empty());
        assert_eq!(
            report(source, "scope = 'tests'", "src/lib.rs")
                .findings
                .len(),
            1
        );
        assert!(
            report(
                "fn run() { std::env::var(\"A\"); }",
                "exclude = 'src/*.rs'",
                "src/lib.rs"
            )
            .findings
            .is_empty()
        );
        let result = report(
            "fn run() {\n// linter:disable rust/environment-variable-access -- Reads one platform fallback before dependency injection.\nstd::env::var(\"A\");\n}",
            "",
            "src/lib.rs",
        );
        assert!(result.findings.is_empty());
        assert_eq!(result.suppressed.len(), 1);
    }
    #[test]
    fn rejects_invalid_or_ignored_configuration() {
        let root = tempfile::tempdir().unwrap();
        for settings in [
            "target = '*'",
            "target = '*'\nfunctions = []",
            "target = '*'\nfunctions = ['bad-path']",
            "target = '*'\nfunctions = ['std::env::var']\nglobal_words = ['config']",
            "target = '*'\nfunctions = ['std::env::var']\nunknown = true",
            "target = '*'\nfunctions = ['std::env::var']\nallowed_modules = ['platform::*']",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/environment-variable-access\"]]\n{settings}"),
            )
            .unwrap();
            assert!(
                matches!(
                    linter::Registry::default()
                        .register::<super::EnvironmentAccess>()
                        .unwrap()
                        .check(root.path()),
                    Err(linter::Error::Configuration(_))
                ),
                "{settings}"
            );
        }
    }
}
