use crate::{
    Analysis,
    scope::{integration, mark_tests},
};
use linter::{Error, Project, Rule, RuleResult, Status};
use std::fs;
mod config;
mod environment;
mod scan;
use config::Assertion;
pub use config::Config;

pub struct AsyncBlocking(Vec<Assertion>);
impl Rule for AsyncBlocking {
    const ID: &'static str = "rust/async-blocking-operation";
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
                    && !assertion
                        .allowed
                        .as_ref()
                        .is_some_and(|allowed| allowed.matches(&source.path))
            }) {
                scan::inspect(source, &tests, assertion, &mut findings);
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
    use std::{fs, path::Path};
    const POLICY: &str = "\n[[rules.\"rust/async-blocking-operation\"]]\ntarget = \"**/*\
        .rs\"\nblocking_functions = [\"std::thread::sleep\", \"std::fs::read\", \"std::f\
        s::read_to_string\", \"std::fs::write\", \"std::fs::File::open\", \"std::fs::Fil\
        e::create\"]\nblocking_contexts = [\"tokio::task::spawn_blocking\", \"tokio::tas\
        k::block_in_place\"]\nguard_adapters = [\"unwrap\", \"expect\"]\nblocking_method\
        s = [\n    { receiver = \"std::process::Command\", methods = [\"spawn\", \"statu\
        s\", \"output\", \"wait\", \"wait_with_output\"], constructors = [\"std::process\
        ::Command::new\"], fluent_methods = [\"arg\", \"args\"] },\n    { receiver = \"s\
        td::fs::OpenOptions\", methods = [\"open\"], constructors = [\"std::fs::OpenOpti\
        ons::new\"], fluent_methods = [\"read\", \"write\", \"create\"] },\n    { receiv\
        er = \"std::sync::Mutex\", methods = [\"lock\"], constructors = [\"std::sync::Mu\
        tex::new\"], returns_guard = true },\n    { receiver = \"std::sync::RwLock\", me\
        thods = [\"read\", \"write\"], constructors = [\"std::sync::RwLock::new\"], retu\
        rns_guard = true },\n    { receiver = \"parking_lot::Mutex\", methods = [\"lock\
        \"], constructors = [\"parking_lot::Mutex::new\"], returns_guard = true },\n    \
        { receiver = \"tokio::sync::Mutex\", methods = [\"blocking_lock\"], constructors\
        \u{20}= [\"tokio::sync::Mutex::new\"] },\n    { receiver = \"tokio::sync::mpsc::\
        Receiver\", methods = [\"blocking_recv\"] },\n]\n";
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::AsyncBlocking>()?
            .check(root)
    }
    fn report(source: &str, settings: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("{POLICY}\n{settings}"),
        )
        .unwrap();
        check(root.path()).unwrap()
    }
    fn findings(source: &str) -> Vec<linter::Finding> {
        report(source, "").findings
    }
    #[test]
    fn detects_async_scopes() {
        let values = findings(
            r#"
use std::{fs as disk, thread::sleep as pause};
use std::process::Command as HostCommand;

async fn bad() {
    pause(std::time::Duration::ZERO);
    let _ = disk::read("x");
    let mut command = HostCommand::new("git");
    let _ = command.output();
    let _ = disk::File::open("x");
    let _ = disk::OpenOptions::new().read(true).open("x");
}
fn synchronous() {
    pause(std::time::Duration::ZERO);
    let _ = disk::read("x");
}
"#,
        );
        assert_eq!(values.len(), 5);
        assert!(values.iter().any(|value| value.message.contains("sleep")));
        assert!(values.iter().any(|value| value.message.contains("std::fs")));
        assert!(
            values
                .iter()
                .any(|value| value.message.contains("std::process"))
        );
    }

    #[test]
    fn ignores_blocking_boundaries() {
        let values = findings(
            r#"
async fn safe() {
    let _ = std::process::Command::new("git");
    let _ = tokio::process::Command::new("git").output().await;
    let _ = tokio::fs::read("x").await;
    tokio::task::spawn_blocking(|| {
        std::thread::sleep(std::time::Duration::ZERO);
        let _ = std::fs::read("x");
        let _ = std::process::Command::new("git").output();
    }).await;
}
#[cfg(test)]
async fn fixture() { let _ = std::fs::read("x"); }
"#,
        );
        assert!(values.is_empty(), "{values:#?}");
    }

    #[test]
    fn diagnoses_runtime_methods() {
        let values = findings(
            r"
use std::sync::Mutex as StdMutex;
use tokio::sync::{mpsc::Receiver, Mutex as AsyncMutex};

async fn locks(
    std_lock: &StdMutex<u8>,
    async_lock: &AsyncMutex<u8>,
    channel: &mut Receiver<u8>,
) {
    let _ = std_lock.lock();
    let _ = async_lock.lock().await;
    let _ = channel.blocking_recv();
}
",
        );
        assert_eq!(values.len(), 2);
        assert!(values.iter().any(|value| value.message.contains("::lock")));
        assert!(
            values
                .iter()
                .any(|value| value.message.contains("::blocking_recv"))
        );
    }

    #[test]
    fn reports_dropped_guard() {
        let values = findings(
            r"
use std::sync::Mutex;
async fn guards(lock: &Mutex<Vec<u8>>) {
    {
        let guard = lock.lock().unwrap();
        ready().await;
        consume(&guard);
    }
    {
        let guard = lock.lock().unwrap();
        drop(guard);
        ready().await;
    }
}
",
        );
        assert_eq!(
            values
                .iter()
                .filter(|value| value.message.contains("held across await"))
                .count(),
            1
        );
    }

    #[test]
    fn async_method_names() {
        let values = findings(
            r"
struct Builder;
impl Builder {
    fn blocking_recv(&self) {}
    fn lock(&self) {}
    fn output(&self) {}
}
fn closures(builder: Builder) {
    let _future = async || {
        std::thread::sleep(std::time::Duration::ZERO);
        builder.blocking_recv();
        builder.lock();
        builder.output();
    };
}
",
        );
        assert_eq!(values.len(), 1);
        assert!(values[0].message.contains("sleep"));
    }
    #[test]
    fn sdk_storage_adapter_and_test_only_impl_regressions() {
        let source = "async fn restore() {\nlet _ = std::fs::read(\"wallets.db\");\ntoki\
            o::task::spawn_blocking(|| std::fs::read(\"wallets.db\")).await;\ntokio::tas\
            k::block_in_place(|| std::fs::read(\"wallets.db\"));\n}";
        let values = findings(source);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].related.len(), 1);
        assert!(
            findings(
                "struct Fixture; #[cfg(all(test, feature = \"fixtures\"))] impl\
            \u{20}Fixture { async fn prepare() { std::fs::read(\"fixture\"); } }"
            )
            .is_empty()
        );
    }
    #[test]
    fn aliases_and_shadowing_do_not_turn_custom_apis_into_known_blockers() {
        for source in [
            "use std::thread::sleep as pause; async fn run(pause: fn(u8)) { pause(1); }",
            "use std::thread::sleep as pause; async fn run() { let pause = |value| value\
                ; pause(1); }",
            "mod std { pub mod thread { pub fn sleep() {} } } async fn run() { std::thre\
                ad::sleep(); }",
            "use std::sync::Mutex; struct Custom; async fn run(lock: &Mutex<u8>) { { let\
                \u{20}lock = Custom; lock.lock(); } }",
            "async fn outer() { fn nested() { std::fs::read(\"x\"); } }",
        ] {
            assert!(findings(source).is_empty(), "{source}");
        }
        assert_eq!(
            findings(
                "use std::thread::sleep as pause; async fn run() { { let pau\
            se = || {}; pause(); } pause(); }"
            )
            .len(),
            1
        );
    }
    #[test]
    fn worker_context_only_exempts_callback_not_evaluated_arguments() {
        let source = "async fn run() { tokio::task::spawn_blocking({ std::fs::read(\"bef\
            ore\"); || std::fs::read(\"inside\") }).await; }";
        assert_eq!(findings(source).len(), 1);
        assert_eq!(
            findings(
                "async fn run() { tokio::task::spawn_blocking(|| std::fs::re\
            ad(\"inside\"), std::fs::read(\"before\")).await; }"
            )
            .len(),
            1
        );
    }
    #[test]
    fn guard_liveness_handles_aliases_shadowing_scope_and_conditional_drop() {
        for body in [
            "let guard = lock.lock().unwrap(); ready().await;",
            "let guard = lock.lock().unwrap(); let alias = guard; ready().await; consume(alias);",
            "let guard = lock.lock().unwrap(); let guard = 0; ready().await;",
            "let guard = lock.lock().unwrap(); if condition { drop(guard); } ready().await;",
            "let guard = lock.lock().unwrap(); while condition { drop(guard); } ready().await;",
        ] {
            let source =
                format!("async fn run(lock: &std::sync::Mutex<u8>, condition: bool) {{ {body} }}");
            assert_eq!(
                findings(&source)
                    .iter()
                    .filter(|finding| finding.message.contains("held across await"))
                    .count(),
                1,
                "{body}"
            );
        }
        for body in [
            "{ let guard = lock.lock().unwrap(); consume(&guard); } ready().await;",
            "let guard = lock.lock().unwrap(); std::mem::drop(guard); ready().await;",
            "let guard = lock.lock().unwrap(); consume(guard); ready().await;",
            "let guard = lock.lock().unwrap(); if condition { drop(guard); } else { drop\
                (guard); } ready().await;",
            "let guard = lock.lock().unwrap(); let unused = async { ready().await; }; drop(guard);",
        ] {
            let source =
                format!("async fn run(lock: &std::sync::Mutex<u8>, condition: bool) {{ {body} }}");
            assert!(
                !findings(&source)
                    .iter()
                    .any(|finding| finding.message.contains("held across await")),
                "{body}"
            );
        }
    }
    #[test]
    fn directives_scopes_and_allowed_paths_are_explicit() {
        let source = "async fn run() {\n// linter:disable rust/async-blocking-operation \
            -- Single startup task uses the bounded compatibility adapter.\nstd::fs::rea\
            d(\"x\");\n}";
        let result = report(source, "");
        assert!(result.findings.is_empty());
        assert_eq!(result.suppressed.len(), 1);
        let source = "#[cfg(test)] async fn run() { std::fs::read(\"x\"); }";
        assert!(report(source, "").findings.is_empty());
        assert_eq!(report(source, "scope = 'tests'").findings.len(), 1);
        assert!(
            report(
                "async fn run() { std::fs::read(\"x\"); }",
                "allowed_targets = 'lib.rs'"
            )
            .findings
            .is_empty()
        );
    }
    #[test]
    fn configuration_rejects_empty_policies_invalid_apis_and_unknown_settings() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "target = '*'",
            "target = '*'\nblocking_functions = ['bad-path']",
            "target = '*'\nblocking_methods = [{ receiver = 'std::sync::Mutex', methods = [] }]",
            "target = []\nblocking_functions = ['std::fs::read']",
            "target = '*'\nblocking_functions = ['std::fs::read']\nunknown = true",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/async-blocking-operation\"]]\n{fields}"),
            )
            .unwrap();
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
    }
}
