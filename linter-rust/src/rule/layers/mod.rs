use std::{collections::BTreeMap, fs, path::Path};

use linter::{Error, Finding, Project, Rule, RuleResult, Status};

use crate::Analysis;
mod config;
pub use config::Config;
use config::Layer;

pub struct Layers {
    layers: Vec<Layer>,
}

impl Rule for Layers {
    const ID: &'static str = "rust/layers";
    type Config = Config;
    type Analysis = Analysis;

    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            layers: config.compile()?,
        })
    }
    fn configured(&self) -> bool {
        !self.layers.is_empty()
    }

    fn check(&self, project: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let root =
            fs::canonicalize(project.root()).map_err(|error| Error::Analysis(error.to_string()))?;
        let mut findings = Vec::new();
        let mut ownership = BTreeMap::new();
        for (manifest, package) in &analysis.packages {
            let directory = manifest
                .parent()
                .and_then(|path| path.strip_prefix(&root).ok())
                .ok_or_else(|| {
                    Error::Analysis(format!("{} is outside the project", manifest.display()))
                })?;
            let directory = if directory.as_os_str().is_empty() {
                Path::new(".")
            } else {
                directory
            };
            let matches: Vec<_> = self
                .layers
                .iter()
                .filter(|layer| layer.selector.matches(directory))
                .collect();
            match matches.as_slice() {
                [layer] => {
                    ownership.insert(manifest, *layer);
                }
                [] => findings.push(finding(
                    manifest.strip_prefix(&root).unwrap_or(manifest),
                    format!(
                        "package {} does not belong to a configured layer",
                        package.name
                    ),
                    "Add a layer path glob covering this package.".into(),
                )),
                _ => findings.push(finding(
                    manifest.strip_prefix(&root).unwrap_or(manifest),
                    format!("package {} matches multiple layers", package.name),
                    "Make layer path globs disjoint so package ownership is unambiguous.".into(),
                )),
            }
        }
        if analysis.packages.is_empty() {
            findings.push(finding(
                Path::new("."),
                "no Cargo packages found for configured layers".into(),
                "Check the project root and discovery exclusions.".into(),
            ));
        }
        for (manifest, source) in &analysis.packages {
            let Some(source_layer) = ownership.get(manifest) else {
                continue;
            };
            for dependency in &source.dependencies {
                let Some(path) = &dependency.path else {
                    continue;
                };
                let target = fs::canonicalize(path.join("Cargo.toml")).map_err(|error| {
                    Error::Analysis(format!("dependency {}: {error}", dependency.name))
                })?;
                let Some(target_layer) = ownership.get(&target) else {
                    if !analysis.packages.contains_key(&target) {
                        findings.push(finding(manifest.strip_prefix(&root).unwrap_or(manifest),
                            format!("{} depends on local package {} outside analyzed layers", source.name, dependency.name),
                            "Include the local dependency in the analyzed project and assign it a layer.".into()));
                    }
                    continue;
                };
                if !source_layer.dependencies.contains(&target_layer.name) {
                    let alias = dependency.rename.as_deref().unwrap_or(&dependency.name);
                    findings.push(finding(manifest.strip_prefix(&root).unwrap_or(manifest),
                        format!("{} -> {} via {alias}: layer {} cannot depend on {} ({:?}, target {})",
                            source.name, dependency.name, source_layer.name, target_layer.name, dependency.kind,
                            dependency.target.as_ref().map_or_else(|| "all".into(), ToString::to_string)),
                        format!("Remove or invert this dependency; place the reusable contract in a permitted layer. Allowed dependency layers: {}.",
                            source_layer.dependencies.iter().cloned().collect::<Vec<_>>().join(", "))));
                }
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn finding(path: &Path, message: String, instruction: String) -> Finding {
    Finding {
        rule: Layers::ID,
        path: path.into(),
        configuration: "rules.\"rust/layers\"".into(),
        message,
        instruction,
    }
}

#[cfg(test)]
mod tests {
    use super::Layers;
    use linter::{Registry, Status};
    use std::{fs, path::Path};

    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "Cargo.toml",
            "[workspace]\nresolver='3'\nmembers=['apps/*','usecase/*','packages/*']\n[workspace.dependencies]\nbase = {path='packages/base'}\n",
        );
        for (path, name) in [
            ("apps/one", "one"),
            ("apps/two", "two"),
            ("usecase/orders", "orders"),
            ("packages/base", "base"),
        ] {
            write(
                root.path(),
                &format!("{path}/Cargo.toml"),
                &format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2024'\n"),
            );
            write(
                root.path(),
                &format!("{path}/src/lib.rs"),
                "pub fn value() -> u8 { 1 }\n",
            );
        }
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules."rust/layers"]]
name = "apps"
target = ["apps/*"]
dependencies = ["usecase", "packages"]
[[rules."rust/layers"]]
name = "usecase"
target = ["usecase/*"]
dependencies = ["usecase", "packages"]
[[rules."rust/layers"]]
name = "packages"
target = ["packages/*"]
dependencies = ["packages"]
"#,
        );
        root
    }

    fn append(root: &Path, path: &str, text: &str) {
        let previous = fs::read_to_string(root.join(path)).unwrap();
        write(root, path, &(previous + text));
    }

    #[test]
    fn permits_declared_edges_and_rejects_apps_and_reverse_dependencies() {
        let root = fixture();
        let registry = Registry::default().register::<Layers>().unwrap();
        append(
            root.path(),
            "apps/one/Cargo.toml",
            "[dependencies]\nbase.workspace=true\norders={path='../../usecase/orders'}\n",
        );
        append(
            root.path(),
            "usecase/orders/Cargo.toml",
            "[dependencies]\nbase.workspace=true\n",
        );
        assert!(registry.check(root.path()).unwrap().findings.is_empty());
        append(root.path(), "apps/one/Cargo.toml", "two={path='../two'}\n");
        append(
            root.path(),
            "packages/base/Cargo.toml",
            "[dependencies]\ntwo={path='../../apps/two'}\n",
        );
        let report = registry.check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 2);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message.contains("layer apps cannot depend on apps"))
        );
        assert!(report.findings.iter().any(|finding| {
            finding
                .message
                .contains("layer packages cannot depend on apps")
        }));
        assert!(!root.path().join("Cargo.lock").exists());
    }

    #[test]
    fn includes_renamed_optional_build_dev_and_target_dependencies() {
        for declaration in [
            "[dependencies]\nalias={package='two',path='../two',optional=true}",
            "[build-dependencies]\ntwo={path='../two'}",
            "[dev-dependencies]\ntwo={path='../two'}",
            "[target.'cfg(windows)'.dependencies]\ntwo={path='../two'}",
        ] {
            let root = fixture();
            append(root.path(), "apps/one/Cargo.toml", declaration);
            let report = Registry::default()
                .register::<Layers>()
                .unwrap()
                .check(root.path())
                .unwrap();
            assert_eq!(report.findings.len(), 1, "{declaration}");
            assert!(report.findings[0].message.contains("one -> two"));
        }
    }

    #[test]
    fn rejects_unknown_layers_and_reports_unclassified_and_ambiguous_packages() {
        let root = fixture();
        let registry = Registry::default().register::<Layers>().unwrap();
        let original = fs::read_to_string(root.path().join("linter.toml")).unwrap();
        write(
            root.path(),
            "linter.toml",
            &original.replace(
                "dependencies = [\"packages\"]",
                "dependencies = [\"unknown\"]",
            ),
        );
        assert!(matches!(
            registry.check(root.path()),
            Err(linter::Error::Configuration(_))
        ));
        write(
            root.path(),
            "linter.toml",
            &original.replace("target = [\"apps/*\"]", "target = [\"missing/*\"]"),
        );
        assert_eq!(registry.check(root.path()).unwrap().findings.len(), 2);
        write(
            root.path(),
            "linter.toml",
            &original.replace(
                "target = [\"packages/*\"]",
                "target = [\"packages/*\", \"apps/*\"]",
            ),
        );
        assert_eq!(registry.check(root.path()).unwrap().findings.len(), 2);
    }

    #[test]
    fn parsing_failures_are_errors_and_disabled_rules_do_not_parse() {
        let root = fixture();
        write(root.path(), "packages/base/src/lib.rs", "fn broken( {");
        let registry = Registry::default().register::<Layers>().unwrap();
        assert!(matches!(
            registry.check(root.path()),
            Err(linter::Error::Analysis(_))
        ));
        write(
            root.path(),
            "linter.toml",
            "[rules.\"rust/layers\"]\nenabled=false",
        );
        assert_eq!(
            registry.check(root.path()).unwrap().rules["rust/layers"],
            Status::Disabled
        );
    }

    #[test]
    fn repository_library_directions_are_enforced() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "Cargo.toml",
            "[workspace]\nresolver='3'\nmembers=['linter','linter-rust','apps/cli']",
        );
        for (path, name) in [
            ("linter", "linter"),
            ("linter-rust", "linter-rust"),
            ("apps/cli", "cli"),
        ] {
            write(
                root.path(),
                &format!("{path}/Cargo.toml"),
                &format!("[package]\nname='{name}'\nversion='0.1.0'"),
            );
            write(
                root.path(),
                &format!("{path}/src/lib.rs"),
                "pub fn value() {} ",
            );
        }
        let policy = include_str!("../../../../linter.toml");
        let mut policy: toml::Value = toml::from_str(policy).unwrap();
        policy["rules"]
            .as_table_mut()
            .unwrap()
            .retain(|name, _| name == "rust/layers");
        write(
            root.path(),
            "linter.toml",
            &toml::to_string(&policy).unwrap(),
        );
        append(
            root.path(),
            "linter-rust/Cargo.toml",
            "\n[dependencies]\nlinter={path='../linter'}",
        );
        append(
            root.path(),
            "apps/cli/Cargo.toml",
            "\n[dependencies]\nlinter={path='../../linter'}\nlinter-rust={path='../../linter-rust'}",
        );
        let registry = Registry::default().register::<Layers>().unwrap();
        assert!(registry.check(root.path()).unwrap().findings.is_empty());
        append(
            root.path(),
            "linter/Cargo.toml",
            "\n[dependencies]\nlinter-rust={path='../linter-rust'}\ncli={path='../apps/cli'}",
        );
        let report = registry.check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 2);
        assert!(report.findings.iter().any(|finding| {
            finding
                .message
                .contains("layer linter cannot depend on rust")
        }));
        assert!(report.findings.iter().any(|finding| {
            finding
                .message
                .contains("layer linter cannot depend on apps")
        }));
    }
}
