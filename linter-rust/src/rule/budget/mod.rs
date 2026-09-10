use crate::{CargoDependency, CargoGraph};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Status};
use std::collections::BTreeMap;
mod config;
pub use config::Config;

pub struct DependencyBudget(Vec<config::Assertion>);
impl Rule for DependencyBudget {
    const ID: &'static str = "rust/dependency-budget";
    type Analysis = CargoGraph;
    type Config = Config;
    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self(config.compile()?))
    }
    fn configured(&self) -> bool {
        !self.0.is_empty()
    }
    fn check(&self, _: &Project, graph: &CargoGraph) -> Result<RuleResult, Error> {
        let mut findings = Vec::new();
        for assertion in &self.0 {
            findings.extend(
                graph
                    .packages
                    .values()
                    .filter_map(|package| assertion.check(package)),
            );
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

impl config::Assertion {
    fn check(&self, package: &crate::CargoPackage) -> Option<Finding> {
        if !self.target.matches(&package.directory)
            || self
                .exclude
                .as_ref()
                .is_some_and(|e| e.matches(&package.directory))
        {
            return None;
        }
        let dependencies = self.dependencies(package);
        if dependencies.len() <= self.max_dependencies {
            return None;
        }
        let related: Vec<_> = dependencies
            .iter()
            .flat_map(|((name, source), declarations)| {
                declarations
                    .iter()
                    .map(move |dependency| evidence(package, dependency, name, source))
            })
            .collect();
        Some(Finding {
            rule: DependencyBudget::ID,
            path: package.manifest.clone(),
            span: related.first().and_then(|e| e.span.clone()),
            related,
            configuration: format!("{}.max_dependencies", self.setting),
            message: format!(
                "crate `{}` declares {} distinct dependencies; maximum is {}",
                package.name,
                dependencies.len(),
                self.max_dependencies
            ),
            instruction: "Remove unnecessary dependencies or move unrelated responsibilities \
                to their owning package."
                .into(),
        })
    }
    fn dependencies<'a>(
        &self,
        package: &'a crate::CargoPackage,
    ) -> BTreeMap<(&'a str, String), Vec<&'a CargoDependency>> {
        let mut dependencies = BTreeMap::<_, Vec<_>>::new();
        for dependency in package
            .dependencies
            .iter()
            .filter(|d| self.kinds.contains(&d.kind))
        {
            let source = dependency
                .manifest
                .as_ref()
                .map(|manifest| format!("local:{}", manifest.display()))
                .or_else(|| dependency.source.clone())
                .unwrap_or_else(|| "registry:crates-io".into());
            dependencies
                .entry((dependency.package.as_str(), source))
                .or_default()
                .push(dependency);
        }
        dependencies
    }
}
fn evidence(
    package: &crate::CargoPackage,
    dependency: &CargoDependency,
    name: &str,
    source: &str,
) -> Evidence {
    let target = dependency
        .target
        .as_ref()
        .map(|target| format!(", target {target}"))
        .unwrap_or_default();
    let optional = if dependency.optional {
        ", optional"
    } else {
        ""
    };
    Evidence {
        path: package.manifest.clone(),
        span: Some(dependency.span.clone()),
        message: format!(
            "{name} [{source}]: alias `{}`, {:?}{target}{optional}",
            dependency.alias, dependency.kind
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn fixture(dependencies: &str, options: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers=['packages/*']\n[\
            workspace.dependencies]\njson={package='serde_json',version='1'}",
        )
        .unwrap();
        for (name, deps) in [("owner", dependencies), ("first", ""), ("second", "")] {
            let directory = root.path().join("packages").join(name);
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                directory.join("Cargo.toml"),
                format!("[package]\nname='{name}'\nversion='0.1.0'\n{deps}"),
            )
            .unwrap();
        }
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/dependency-budget\"]]\ntarget='packages/*'\n{options}"),
        )
        .unwrap();
        root
    }
    fn check(root: &tempfile::TempDir) -> Result<linter::Report, Error> {
        linter::Registry::default()
            .register::<DependencyBudget>()
            .unwrap()
            .check(root.path())
    }
    #[test]
    fn counts_distinct_local_targets_across_kinds() {
        let root = fixture(
            r#"[dependencies]
first={path='../first'}
[dev-dependencies]
second={path='../second'}"#,
            "max_dependencies=1",
        );
        assert!(check(&root).unwrap().findings.is_empty());
        fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"rust/dependency-budget\"]]\
            \ntarget='packages/*'\nmax_dependencies=1\nkinds=['normal','build','developm\
            ent']",
        )
        .unwrap();
        let report = check(&root).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(
            report.findings[0]
                .message
                .contains("2 distinct dependencies")
        );
        assert_eq!(report.findings[0].related.len(), 2);
    }
    #[test]
    fn aliases_targets_versions_and_kinds_do_not_inflate() {
        let root = fixture(
            "[dependencies]\none={package='first',path='../first'}\n[build-dependencies]\
                \nfirst={path='../first'}\n[target.'cfg(unix)'.dependencies]\nother={pac\
                kage='first',path='../first'}",
            "max_dependencies=1",
        );
        assert!(check(&root).unwrap().findings.is_empty());
        let root = fixture(
            "[dependencies]\na={package='serde',version='1'}\n[target.'cfg(unix)'.depend\
                encies]\nb={package='serde',version='2'}",
            "max_dependencies=1",
        );
        assert!(check(&root).unwrap().findings.is_empty());
    }
    #[test]
    fn inherited_external_optional_dependencies_count() {
        let root = fixture(
            "[dependencies]\njson={workspace=true,optional=true,features=['raw_value']}\
                \nfirst={path='../first'}",
            "max_dependencies=1",
        );
        let report = check(&root).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(
            report.findings[0]
                .related
                .iter()
                .any(|e| e.message.contains("serde_json") && e.message.contains("optional"))
        );
    }
    #[test]
    fn registries_and_git_revisions_are_distinct() {
        let root = fixture(
            "[dependencies]\na={package='value',version='1'}\nb={package='value',version\
                ='1',registry='private'}\nc={package='value',git='https://example.invali\
                d/value',rev='first'}\nd={package='value',git='https://example.invalid/v\
                alue',rev='second'}",
            "max_dependencies=3",
        );
        let report = check(&root).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(
            report.findings[0]
                .message
                .contains("4 distinct dependencies")
        );
    }
    #[test]
    fn excluding_owners_does_not_subtract_targets() {
        let root = fixture(
            "[dependencies]\nfirst={path='../first'}\nsecond={path='../second'}",
            "max_dependencies=1\nexclude='packages/first'",
        );
        assert_eq!(check(&root).unwrap().findings.len(), 1);
        let root = fixture(
            "[dependencies]\nfirst={path='../first'}\nsecond={path='../second'}",
            "max_dependencies=1\nexclude='packages/owner'",
        );
        assert!(check(&root).unwrap().findings.is_empty());
    }
    #[test]
    fn external_paths_retain_distinct_identity() {
        let root = fixture("", "max_dependencies=1");
        let external = tempfile::tempdir().unwrap();
        for name in ["left", "right"] {
            let path = external.path().join(name);
            fs::create_dir(&path).unwrap();
            fs::write(
                path.join("Cargo.toml"),
                "[package]\nname='same'\nversion='0.1.0'",
            )
            .unwrap();
        }
        fs::write(
            root.path().join("packages/owner/Cargo.toml"),
            format!(
                "[package]\nname\
            ='owner'\nversion='0.1.0'\n[dependencies]\nleft={{package='same',path={:?}}}\
            \nright={{package='same',path={:?}}}",
                external.path().join(
                    "\
            left"
                ),
                external.path().join(
                    "\
            right"
                )
            ),
        )
        .unwrap();
        let report = check(&root).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(
            report.findings[0]
                .message
                .contains("2 distinct dependencies")
        );
    }
    #[test]
    fn invalid_config_and_manifest_fail() {
        for options in [
            "",
            "max_dependencies=0",
            "max_dependencies=-1",
            "max_dependencies=1\nkinds=[]",
            "max_dependencies=1\nkinds=['normal','normal']",
            "max_dependencies=1\nkinds=['invalid']",
            "max_dependencies=1\nunknown=true",
        ] {
            let root = fixture("", options);
            assert!(check(&root).is_err(), "{options}");
        }
        let root = fixture("[dependencies]\nfirst={path=1}", "max_dependencies=1");
        assert!(check(&root).is_err());
    }
    #[test]
    fn readme_configuration_runs_at_exact_threshold() {
        let root = fixture(
            "[dependencies]\nfirst={path='../first'}\nsecond={path='../second'}",
            "max_dependencies=2",
        );
        assert!(check(&root).unwrap().findings.is_empty());
        let policy = include_str!("readme.md")
            .split("```toml\n")
            .nth(1)
            .unwrap()
            .split("```")
            .next()
            .unwrap();
        fs::write(root.path().join("linter.toml"), policy).unwrap();
        assert!(check(&root).unwrap().findings.is_empty());
    }
}
