use crate::{
    Analysis, Source,
    declaration::Index,
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};
use heck::ToSnakeCase;
use linter::{Error, Finding, Project, Rule, RuleResult, Span, Status};
use tree_sitter::Node;
mod config;
use crate::exports;
use crate::namespace as context;
pub use config::Config;
pub struct ModulePrefix(Vec<Assertion>);
impl Rule for ModulePrefix {
    const ID: &'static str = "rust/redundant-module-prefix";
    type Analysis = Analysis;
    type Config = Config;
    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self(config.compile()?))
    }
    fn configured(&self) -> bool {
        !self.0.is_empty()
    }
    fn check(&self, project: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let root = std::fs::canonicalize(project.root())
            .map_err(|error| Error::Analysis(error.to_string()))?;
        let index = Index::new(analysis, &root);
        let contexts = context::Context::new(analysis, &index, &root);
        let exports = exports::Exports::new(analysis, &index, &contexts);
        let mut findings = Vec::new();
        for source in &analysis.sources {
            let Some(namespace) = contexts.namespace(&source.path) else {
                continue;
            };
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
            }) {
                inspect(
                    source.syntax.root_node(),
                    source,
                    &namespace,
                    &exports,
                    &tests,
                    assertion,
                    &mut findings,
                );
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
fn inspect(
    node: Node<'_>,
    source: &Source,
    namespace: &[String],
    exports: &exports::Exports,
    tests: &[bool],
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) {
    if node.kind() == "impl_item" {
        return;
    }
    let selected = match assertion.scope {
        Scope::Production => !tests.get(node.start_byte()).copied().unwrap_or(false),
        Scope::Tests => tests.get(node.start_byte()).copied().unwrap_or(false),
        Scope::All => true,
    };
    if selected
        && matches!(
            node.kind(),
            "struct_item"
                | "enum_item"
                | "trait_item"
                | "function_item"
                | "function_signature_item"
        )
        && let Some(finding) = finding(node, source, namespace, exports, assertion)
    {
        findings.push(finding);
    }
    let mut namespace = namespace.to_vec();
    if node.kind() == "mod_item"
        && let Some(name) = node.child_by_field_name("name")
    {
        namespace.push(
            source.text[name.byte_range()]
                .trim_start_matches("r#")
                .to_owned(),
        );
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(
            child, source, &namespace, exports, tests, assertion, findings,
        );
    }
}
fn finding(
    node: Node<'_>,
    source: &Source,
    namespace: &[String],
    exports: &exports::Exports,
    assertion: &Assertion,
) -> Option<Finding> {
    let name = &source.text[node.child_by_field_name("name")?.byte_range()];
    if assertion
        .ignored_names
        .contains(name.trim_start_matches("r#"))
    {
        return None;
    }
    let normalized = name.trim_start_matches("r#").to_snake_case();
    let words: Vec<_> = normalized.split('_').collect();
    let module = namespace.iter().rev().find(|module| {
        let normalized = module.to_snake_case();
        let prefix: Vec<_> = normalized.split('_').collect();
        !normalized.is_empty()
            && words.len() > prefix.len()
            && words.starts_with(&prefix)
            && !exports.omits(source, node, module)
    })?;
    Some(Finding {
        rule: ModulePrefix::ID,
        path: source.path.clone(),
        span: Some(Span::new(&source.text, node.byte_range())),
        related: Vec::new(),
        configuration: assertion.setting.clone(),
        message: format!("`{name}` repeats its `{module}` module prefix"),
        instruction: format!(
            "Remove the `{module}` prefix; the module path already supplies that context."
        ),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn check(files: &[(&str, &str)], config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let file = root.path().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/redundant-module-prefix\"]]\ntarget='**/*.rs'\n{config}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<ModulePrefix>()?
            .check(root.path())
            .map(|report| report.findings)
    }
    #[test]
    fn preserves_directory_context_without_duplicating_receiver_checks() {
        let source = "struct LauncherPlan;struct RuntimePlan;fn launcher_start(){}struct\
            \u{20}Runner;impl Runner{fn launcher_prepare(){}}impl external::Contract for\
            \u{20}Runner{fn launcher_external_name(){}}trait Drive{fn launcher_publish()\
            ;}#[cfg(test)]mod tests{struct LauncherFixture;}";
        let found = check(&[("src/launcher/plan.rs", source)], "").unwrap();
        assert_eq!(found.len(), 3);
        assert!(
            found
                .iter()
                .any(|finding| finding.message.contains("launcher_publish"))
        );
        assert!(
            !found
                .iter()
                .any(|finding| finding.message.contains("launcher_prepare"))
        );
    }
    #[test]
    fn checks_current_and_ancestor_modules_without_substring_matching() {
        let source = "struct LauncherPlan;struct PlanSpec;struct PlanetSpec;enum PlanKin\
            d{A}mod nested{struct NestedValue;struct PlanOther;}";
        assert_eq!(
            check(&[("src/launcher/plan.rs", source)], "")
                .unwrap()
                .len(),
            5
        );
        assert_eq!(
            check(
                &[(
                    "src/http_server/mod.rs",
                    "struct HTTPServerValue;struct HTTPService;"
                )],
                ""
            )
            .unwrap()
            .len(),
            1
        );
        assert!(
            check(&[("src/lib.rs", "struct SrcValue;struct LibValue;")], "")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn resolves_path_override_and_ignores_import_aliases() {
        let found = check(
            &[
                (
                    "src/lib.rs",
                    "#[path=\"other.rs\"]mod launcher;use launcher as alias;",
                ),
                (
                    "src/other.rs",
                    "struct LauncherPlan;struct OtherPlan;struct AliasPlan;",
                ),
            ],
            "",
        )
        .unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("LauncherPlan"));
        let ambiguous = check(
            &[
                (
                    "src/lib.rs",
                    "#[path=\"other.rs\"]mod launcher;#[path=\"other.rs\"]mod other;",
                ),
                ("src/other.rs", "struct LauncherPlan;struct OtherPlan;"),
            ],
            "",
        )
        .unwrap();
        assert!(ambiguous.is_empty());
    }
    #[test]
    fn follows_declared_child_modules_under_path_overrides() {
        let found = check(
            &[
                ("src/lib.rs", "#[path=\"alt/mod.rs\"]mod launcher;"),
                ("src/alt/mod.rs", "mod plan;"),
                (
                    "src/alt/plan.rs",
                    "struct LauncherData;struct PlanData;struct AltData;",
                ),
            ],
            "",
        )
        .unwrap();
        assert_eq!(found.len(), 2);
    }
    #[test]
    fn scope_ignored_names_and_directives_apply() {
        let text = "#[test]fn launcher_assertion(){}fn launcher_production(){}";
        assert_eq!(
            check(&[("src/launcher/plan.rs", text)], "").unwrap().len(),
            1
        );
        assert_eq!(
            check(&[("src/launcher/plan.rs", text)], "scope='all'")
                .unwrap()
                .len(),
            2
        );
        assert!(
            check(
                &[("src/launcher/plan.rs", text)],
                "ignored_names=['launcher_production']"
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            check(&[("src/launcher/plan.rs", text)], "exclude='src/**'")
                .unwrap()
                .is_empty()
        );
        assert!(
            check(&[("tests/launcher/plan.rs", text)], "")
                .unwrap()
                .is_empty()
        );
        assert!(
            check(
                &[(
                    "src/launcher/plan.rs",
                    "// linter:disable rust/redundant-module\
            -prefix -- public API name is externally fixed\nstruct LauncherPlan;"
                )],
                ""
            )
            .unwrap()
            .is_empty()
        );
    }
    #[test]
    fn rejects_bad_configuration_and_accepts_empty_sources() {
        for config in [
            "scope='unknown'",
            "ignored_names=['bad name']",
            "ignored_names=['a','a']",
            "exclude=[]",
            "unknown=true",
        ] {
            assert!(matches!(
                check(&[("lib.rs", "")], config),
                Err(Error::Configuration(_))
            ));
        }
        assert!(check(&[("lib.rs", "")], "").unwrap().is_empty());
        for source in [
            include_str!("mod.rs"),
            include_str!("config.rs"),
            include_str!("../../namespace.rs"),
        ] {
            assert!(check(&[("lib.rs", source)], "").unwrap().is_empty());
        }
    }
    #[test]
    fn root_reexports_do_not_inherit_private_implementation_prefixes() {
        let files = [
            ("src/lib.rs", "mod rule;pub use rule::RuleResult;"),
            ("src/rule/mod.rs", "pub struct RuleResult;struct RuleCache;"),
        ];
        let found = check(&files, "").unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("RuleCache"));
        let public = [
            ("src/lib.rs", "pub mod rule;"),
            ("src/rule/mod.rs", "pub struct RuleResult;"),
        ];
        assert_eq!(check(&public, "").unwrap().len(), 1);
        let partial = [
            ("src/lib.rs", "mod rule;pub(crate) use rule::RuleResult;"),
            ("src/rule/mod.rs", "pub struct RuleResult;"),
        ];
        assert_eq!(check(&partial, "").unwrap().len(), 1);
    }
    #[test]
    fn follows_grouped_alias_chained_and_glob_public_reexports() {
        for root in [
            "mod rule;pub use rule::{RuleResult};",
            "mod rule;pub use rule::RuleResult as Outcome;",
            "mod rule;pub use rule::*;",
            "mod rule;pub mod api{pub use crate::rule::RuleResult;}",
        ] {
            let files = [
                ("src/lib.rs", root),
                ("src/rule/mod.rs", "mod hidden;pub use hidden::RuleResult;"),
                ("src/rule/hidden.rs", "pub struct RuleResult;"),
            ];
            assert!(check(&files, "").unwrap().is_empty(), "{root}");
        }
    }
    #[test]
    fn reexports_from_unreachable_modules_do_not_hide_real_module_prefixes() {
        let files = [
            (
                "src/lib.rs",
                "pub mod rule;mod api{pub use crate::rule::RuleResult;}",
            ),
            ("src/rule/mod.rs", "pub struct RuleResult;"),
        ];
        assert_eq!(check(&files, "").unwrap().len(), 1);
        let traits = [
            ("src/lib.rs", "mod rule;pub use rule::Rules;"),
            ("src/rule/mod.rs", "pub trait Rules{fn rule_check(&self);}"),
        ];
        assert!(check(&traits, "").unwrap().is_empty());
    }
}
