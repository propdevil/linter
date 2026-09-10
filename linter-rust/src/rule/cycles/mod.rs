use crate::CargoGraph;
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Status};
mod config;
mod graph;
pub use config::Config;

pub struct DependencyCycles(Vec<config::Assertion>);
impl Rule for DependencyCycles {
    const ID: &'static str = "rust/dependency-cycles";
    type Analysis = CargoGraph;
    type Config = Config;
    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self(config.compile()?))
    }
    fn configured(&self) -> bool {
        !self.0.is_empty()
    }
    fn check(&self, _: &Project, analysis: &CargoGraph) -> Result<RuleResult, Error> {
        let packages: Vec<_> = analysis.packages.values().collect();
        let mut findings = Vec::new();
        for assertion in &self.0 {
            let eligible: Vec<_> = packages
                .iter()
                .map(|package| {
                    !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|selector| selector.matches(&package.directory))
                })
                .collect();
            let mut adjacency = vec![Vec::new(); packages.len()];
            for (from, package) in packages.iter().enumerate().filter(|(i, _)| eligible[*i]) {
                for edge in &package.dependencies {
                    if assertion.kinds.contains(&edge.kind)
                        && let Some(to) = packages
                            .iter()
                            .position(|package| Some(&package.manifest) == edge.manifest.as_ref())
                        && eligible[to]
                    {
                        adjacency[from].push(to);
                    }
                }
                adjacency[from].sort_unstable();
                adjacency[from].dedup();
            }
            for component in graph::components(&adjacency) {
                let Some(start) = component
                    .iter()
                    .copied()
                    .find(|index| assertion.target.matches(&packages[*index].directory))
                else {
                    continue;
                };
                let Some(path) = graph::cycle(start, &component, &adjacency) else {
                    continue;
                };
                let mut evidence = Vec::new();
                for pair in path.windows(2) {
                    let from = packages[pair[0]];
                    let to = packages[pair[1]];
                    let Some(edge) = from.dependencies.iter().find(|edge| {
                        edge.manifest.as_ref() == Some(&to.manifest)
                            && assertion.kinds.contains(&edge.kind)
                    }) else {
                        continue;
                    };
                    evidence.push(Evidence {
                        path: from.manifest.clone(),
                        span: Some(edge.span.clone()),
                        message: format!(
                            "{} -> {} ({:?}, alias `{}`{})",
                            from.name,
                            to.name,
                            edge.kind,
                            edge.alias,
                            edge.target
                                .as_ref()
                                .map(|target| format!(", target {target}"))
                                .unwrap_or_default()
                        ),
                    });
                }
                findings.push(Finding{
                    rule:Self::ID,path:packages[start].manifest.clone(),span:evidence.first().and_then(|e|e.span.clone()),related:evidence,configuration:assertion.setting.clone(),
                    message:format!("declared Cargo dependency cycle: {}",path.iter().map(|i|format!("{} [{}]",packages[*i].name,packages[*i].directory.display())).collect::<Vec<_>>().join(" -> ")),
                    instruction:"Remove a reverse dependency or move the shared contract to its owning lower layer.".into(),
                });
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
    use super::*;
    use std::fs;
    fn fixture(packages: &[(&str, &str)], workspace: &str, options: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            format!("[workspace]\nmembers=['*']\nresolver='3'\n{workspace}"),
        )
        .unwrap();
        for (name, dependencies) in packages {
            let path = root.path().join(name);
            fs::create_dir_all(path.join("src")).unwrap();
            fs::write(path.join("src/lib.rs"), "").unwrap();
            fs::write(
                path.join("Cargo.toml"),
                format!("[package]\nname='{name}'\nversion='0.1.0'\n{dependencies}"),
            )
            .unwrap();
        }
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/dependency-cycles\"]]\ntarget='*'\n{options}"),
        )
        .unwrap();
        root
    }
    fn check(root: &tempfile::TempDir) -> Result<linter::Report, Error> {
        linter::Registry::default()
            .register::<DependencyCycles>()
            .unwrap()
            .check(root.path())
    }
    #[test]
    fn detects_normal_and_build_not_development() {
        let root = fixture(
            &[
                ("a", "[dependencies]\nb={path='../b'}"),
                ("b", "[build-dependencies]\na={path='../a'}"),
                ("c", "[dev-dependencies]\nd={path='../d'}"),
                ("d", "[dev-dependencies]\nc={path='../c'}"),
            ],
            "",
            "",
        );
        let report = check(&root).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].related.len(), 2);
        assert!(report.findings[0].related.iter().all(|e| e.span.is_some()));
        fs::write(root.path().join("linter.toml"),"[[rules.\"rust/dependency-cycles\"]]\ntarget='*'\nkinds=['normal','build','development']").unwrap();
        assert_eq!(check(&root).unwrap().findings.len(), 2);
    }
    #[test]
    fn combined_ast_registry_accepts_real_cycle() {
        let root = fixture(
            &[
                ("a", "[dependencies]\nb={path='../b'}"),
                ("b", "[dependencies]\na={path='../a'}"),
            ],
            "",
            "",
        );
        fs::write(root.path().join("linter.toml"),"[[rules.\"rust/dependency-cycles\"]]\ntarget='*'\n[[rules.\"rust/function-length\"]]\ntarget='**/*.rs'\nmax_lines=50").unwrap();
        let report = linter::Registry::default()
            .register::<DependencyCycles>()
            .unwrap()
            .register::<crate::FunctionLength>()
            .unwrap()
            .check(root.path())
            .unwrap();
        assert_eq!(report.findings.len(), 1);
    }
    #[test]
    fn inherited_renamed_target_edges() {
        let root = fixture(
            &[
                (
                    "a",
                    "[target.'cfg(unix)'.dependencies]\nalias.workspace=true",
                ),
                ("b", "[dependencies]\na={path='../a'}"),
            ],
            "[workspace.dependencies]\nalias={package='b',path='b'}",
            "",
        );
        let report = check(&root).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(
            report.findings[0]
                .related
                .iter()
                .any(|e| e.message.contains("alias `alias`, target cfg(unix)"))
        );
    }
    #[test]
    fn external_names_are_not_local_edges() {
        let root = fixture(
            &[
                ("a", "[dependencies]\nb='1'"),
                ("b", "[dependencies]\na={path='../a'}"),
            ],
            "",
            "",
        );
        assert!(check(&root).unwrap().findings.is_empty());
    }
    #[test]
    fn self_cycles_and_scc_evidence() {
        let root = fixture(
            &[
                ("a", "[dependencies]\na={path='.'}"),
                ("b", "[dependencies]\nc={path='../c'}\nd={path='../d'}"),
                ("c", "[dependencies]\nb={path='../b'}"),
                ("d", "[dependencies]\nb={path='../b'}"),
            ],
            "",
            "",
        );
        let report = check(&root).unwrap();
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.findings[0].related.len(), 1);
    }
    #[test]
    fn target_does_not_cut_cycle_but_exclude_does() {
        let root = fixture(
            &[
                ("a", "[dependencies]\nb={path='../b'}"),
                ("b", "[dependencies]\na={path='../a'}"),
            ],
            "",
            "",
        );
        fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"rust/dependency-cycles\"]]\ntarget='b'",
        )
        .unwrap();
        assert_eq!(check(&root).unwrap().findings.len(), 1);
        fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"rust/dependency-cycles\"]]\ntarget='b'\nexclude='a'",
        )
        .unwrap();
        assert!(check(&root).unwrap().findings.is_empty());
    }
    #[test]
    fn invalid_meaningful_dependencies_fail() {
        for dependencies in [
            "[dependencies]\nb={path=12}",
            "[dependencies]\nb={workspace=true}",
            "[dependencies]\nb={path='../missing'}",
            "[dependencies]\nb={path='../b',package='wrong'}",
            "[dependencies]\nb=12",
            "[dependencies]\nb={workspace=false}",
        ] {
            let root = fixture(&[("a", dependencies), ("b", "")], "", "");
            assert!(check(&root).is_err(), "{dependencies}");
        }
    }
    #[test]
    fn explicit_workspace_and_invalid_unused_inheritance() {
        let root = fixture(
            &[
                ("a", "workspace='..'\n[dependencies]\nb.workspace=true"),
                ("b", "[dependencies]\na={path='../a'}"),
            ],
            "[workspace.dependencies]\nb={path='b'}",
            "",
        );
        assert_eq!(check(&root).unwrap().findings.len(), 1);
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\n[workspace.dependencies]\nunused={path=23}",
        )
        .unwrap();
        assert!(check(&root).is_err());
    }
    #[test]
    fn duplicate_package_names_keep_path_identity() {
        let root = fixture(
            &[
                ("a", "[dependencies]\nx={path='../b'}"),
                ("b", ""),
                ("c", ""),
            ],
            "",
            "",
        );
        fs::write(
            root.path().join("b/Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'",
        )
        .unwrap();
        fs::write(
            root.path().join("c/Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n[dependencies]\na={path='../a'}",
        )
        .unwrap();
        assert!(check(&root).unwrap().findings.is_empty());
    }
    #[test]
    fn missing_and_malformed_manifest_fields_fail() {
        for text in [
            "[package]\nname=42",
            "[package]\nname='a'\n[dependencies]\nb='not a version'",
            "[package]\nname='a'\n[target]\nunix=42",
            "[[broken",
        ] {
            let root = fixture(&[("a", "")], "", "");
            fs::write(root.path().join("a/Cargo.toml"), text).unwrap();
            assert!(check(&root).is_err(), "{text}");
        }
    }
    #[test]
    fn invalid_rule_configuration_fails() {
        for options in [
            "kinds=[]",
            "kinds=['normal','normal']",
            "kinds=['dev']",
            "unknown=true",
        ] {
            let root = fixture(&[], "", options);
            assert!(check(&root).is_err());
        }
    }
}
