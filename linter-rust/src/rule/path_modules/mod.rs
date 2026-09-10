use crate::{
    Analysis, Source,
    scope::{integration, mark_tests},
};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};
use tree_sitter::Node;
mod attributes;
mod config;
pub use config::Config;
use config::{Assertion, Scope};

pub struct PathModules(Vec<Assertion>);
impl Rule for PathModules {
    const ID: &'static str = "rust/path-module-flattening";
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
            let base = source.path.parent().unwrap_or(Path::new(""));
            let root_module = source.path.file_name().is_some_and(|name| name == "mod.rs")
                || analysis.packages.values().any(|package| {
                    package
                        .targets
                        .iter()
                        .any(|target| target.src_path.as_std_path() == root.join(&source.path))
                });
            let inline = if root_module {
                base.to_path_buf()
            } else {
                base.join(source.path.file_stem().unwrap_or_default())
            };
            for assertion in self.0.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                Scan {
                    source,
                    tests: &tests,
                    assertion,
                    findings: &mut findings,
                }
                .namespace(source.syntax.root_node(), base, &inline);
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
struct Scan<'a> {
    source: &'a Source,
    tests: &'a [bool],
    assertion: &'a Assertion,
    findings: &'a mut Vec<Finding>,
}
impl Scan<'_> {
    fn namespace(&mut self, node: Node<'_>, base: &Path, inline: &Path) {
        let mut attributes = Vec::new();
        let mut domains: BTreeMap<String, Vec<(Node<'_>, PathBuf)>> = BTreeMap::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "attribute_item" => {
                    if let Some(meta) = attributes::meta(child, &self.source.text) {
                        attributes.push(meta);
                    }
                }
                "line_comment" | "block_comment" => {}
                "mod_item" => {
                    if !attributes::platform(&attributes) {
                        let explicit = attributes::path(&attributes);
                        if let Some(body) = child.child_by_field_name("body") {
                            let name = child
                                .child_by_field_name("name")
                                .map(|name| &self.source.text[name.byte_range()])
                                .unwrap_or("");
                            let directory = explicit.map_or_else(
                                || inline.join(name.trim_start_matches("r#")),
                                |path| base.join(path),
                            );
                            if let Some(directory) = normalize(&directory) {
                                self.namespace(body, &directory, &directory);
                            }
                        } else if self.selected(child)
                            && let Some(path) =
                                explicit.and_then(|path| normalize(&base.join(path)))
                            && let Ok(relative) = path.strip_prefix(base)
                            && relative.components().count() > 1
                            && let Some(domain) = relative.components().next()
                        {
                            domains
                                .entry(domain.as_os_str().to_string_lossy().into_owned())
                                .or_default()
                                .push((child, path));
                        }
                    }
                    attributes.clear();
                }
                _ => {
                    attributes.clear();
                }
            }
        }
        if domains.len() > self.assertion.max_child_domains {
            self.report(&domains);
        }
    }
    fn selected(&self, node: Node<'_>) -> bool {
        let test = self.tests[node.start_byte()];
        match self.assertion.scope {
            Scope::Production => !test,
            Scope::Tests => test,
            Scope::All => true,
        }
    }
    fn report(&mut self, domains: &BTreeMap<String, Vec<(Node<'_>, PathBuf)>>) {
        let declarations: Vec<_> = domains.values().flatten().collect();
        let names = domains.keys().cloned().collect::<Vec<_>>().join(", ");
        self.findings.push(Finding {
            rule: PathModules::ID,
            path: self.source.path.clone(),
            configuration: self.assertion.setting.clone(),
            span: declarations
                .iter()
                .map(|(node, _)| *node)
                .min_by_key(Node::start_byte)
                .map(|node| Span::new(&self.source.text, node.byte_range())),
            related: declarations
                .iter()
                .map(|(node, path)| Evidence {
                    path: self.source.path.clone(),
                    span: Some(Span::new(&self.source.text, node.byte_range())),
                    message: format!(
                        "\
                Explicit module resolves to '{}'.",
                        path.display()
                    ),
                })
                .collect(),
            message: format!(
                "{} explicit module paths flatten {} child domains into one\
                \u{20}namespace: {names}; maximum is {}",
                declarations.len(),
                domains.len(),
                self.assertion.max_child_domains
            ),
            instruction: "Declare cohesive child modules through their natural module bo\
                undary and deliberately re-export the public surface."
                .into(),
        });
    }
}
fn normalize(path: &Path) -> Option<PathBuf> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => result.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::PathModules>()?
            .check(root)
    }
    fn fixture(source: &str, settings: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "Cargo.toml",
            "[package]\nname='example'\nversion='0.0.0'\nedition='2024'",
        );
        write(root.path(), "src/lib.rs", source);
        write(
            root.path(),
            "linter.toml",
            &format!("[[rules.\"rust/path-module-flattening\"]]\ntarget = '**/*.rs'\n{settings}"),
        );
        root
    }
    #[test]
    fn transfers_multiple_injected_domains_and_configurable_boundary() {
        let root = fixture(
            r#"#[path = "registry/state.rs"] mod state;
#[path = "registry/snapshot.rs"] mod snapshot;
#[path = "signal/plan.rs"] mod signal_plan;
#[path = "checkpoint/mod.rs"] mod checkpoint;
"#,
            "",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(
            report.findings[0]
                .message
                .contains("checkpoint, registry, signal")
        );
        assert_eq!(report.findings[0].related.len(), 4);
        write(
            root.path(),
            "linter.toml",
            "[[rules.\"rust/path-module-flattening\"]]\ntarget = '**/*.rs'\nmax_child_domains = 3",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
    }
    #[test]
    fn one_domain_test_scopes_and_platform_wiring_are_preserved() {
        let source = r#"#[path = "registry/state.rs"] mod state;
#[path = "registry/snapshot.rs"] mod snapshot;
#[cfg(test)] #[path = "fixture/mock.rs"] mod mock;
#[cfg(target_os = "linux")] #[path = "platform/linux.rs"] mod linux;
#[cfg(windows)] #[path = "windows/implementation.rs"] mod windows;
"#;
        let root = fixture(source, "");
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(
            root.path(),
            "linter.toml",
            "[[rules.\"rust/path-module-flattening\"]]\ntarget = '**/*.rs'\nscope = 'all'",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].related.len(), 3);
    }
    #[test]
    fn normalized_paths_and_separate_namespaces_do_not_create_fake_domains() {
        let source = r#"#[path = "./registry/state.rs"] mod state;
#[path = "signal/../registry/snapshot.rs"] mod snapshot;
#[path = "../outside/a.rs"] mod outside;
mod first { #[path = "one/a.rs"] mod a; }
mod second { #[path = "two/b.rs"] mod b; }
"#;
        assert!(
            check(fixture(source, "").path())
                .unwrap()
                .findings
                .is_empty()
        );
    }
    #[test]
    fn resolves_inline_non_mod_files_and_explicit_inline_directories() {
        let root = fixture("mod owner;", "");
        write(
            root.path(),
            "src/owner.rs",
            r#"mod inline {
#[path = "one/a.rs"] mod a;
#[path = "two/b.rs"] mod b;
}
#[path = "custom"] mod redirected {
#[path = "three/c.rs"] mod c;
#[path = "four/d.rs"] mod d;
}"#,
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 2);
        let evidence = report
            .findings
            .iter()
            .flat_map(|finding| &finding.related)
            .map(|evidence| evidence.message.as_str())
            .collect::<Vec<_>>();
        assert!(
            evidence
                .iter()
                .any(|text| text.contains("src/owner/inline/one/a.rs"))
        );
        assert!(
            evidence
                .iter()
                .any(|text| text.contains("src/custom/three/c.rs"))
        );
    }
    #[test]
    fn directives_attach_to_first_path_module_and_empty_config_is_rejected() {
        let root = fixture(
            "// linter:disable rust/path-module-flattening -- Generated public facade pr\
                eserves an external module contract.\n#[path=\"one/a.rs\"] mod a;\n#[pat\
                h=\"two/b.rs\"] mod b;",
            "",
        );
        let report = check(root.path()).unwrap();
        assert!(report.findings.is_empty());
        assert_eq!(report.suppressed.len(), 1);
        for settings in [
            "target = []",
            "target = '*'\nmax_child_domains = 0",
            "target = '*'\nscope = 'unknown'",
            "target = '*'\nexclude = []",
            "target = '*'\nunknown = true",
        ] {
            write(
                root.path(),
                "linter.toml",
                &format!("[[rules.\"rust/path-module-flattening\"]]\n{settings}"),
            );
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{settings}"
            );
        }
    }
}
