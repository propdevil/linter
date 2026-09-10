use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    path::Path,
};

use crate::{Error, Project, Report, Rule, RuleResult, Status, config::Configuration};

type Check = Box<dyn Fn(&Project, &mut Analyses) -> Result<RuleResult, Error> + Send + Sync>;
type Analyses = BTreeMap<TypeId, (Box<dyn Any + Send + Sync>, Vec<crate::Directive>)>;

type Factory = fn(Option<toml::Value>) -> Result<Check, Error>;

#[derive(Default)]
pub struct Registry {
    rules: BTreeMap<&'static str, Factory>,
}

impl Registry {
    pub fn register<R: Rule>(mut self) -> Result<Self, Error> {
        if R::ID.is_empty() || self.rules.insert(R::ID, prepare::<R>).is_some() {
            return Err(Error::Configuration(format!(
                "duplicate or empty rule ID {:?}",
                R::ID
            )));
        }
        Ok(self)
    }

    /// Reads configuration, validates every registered rule, then runs enabled checks.
    pub fn check(&self, root: &Path) -> Result<Report, Error> {
        let mut configuration = Configuration::load(root)?;
        for id in configuration.rules.keys() {
            if !self.rules.contains_key(id.as_str()) {
                return Err(Error::Configuration(format!("unknown rule {id:?}")));
            }
        }
        let exclusions = configuration.files.compile()?;
        let mut checks = Vec::new();
        for (&id, factory) in &self.rules {
            let settings = configuration
                .rules
                .remove(id)
                .map(|value| crate::config::Settings::parse(value, id))
                .transpose()?
                .unwrap_or_default();
            // Invalid disabled settings are still errors, never silently ignored.
            checks.push((id, settings.enabled, factory(settings.config)?));
        }
        let project = Project::load(
            root,
            &exclusions,
            checks.iter().any(|(_, enabled, _)| *enabled),
        )?;
        let mut report = Report {
            suppressed: Vec::new(),
            rules: BTreeMap::new(),
            findings: Vec::new(),
        };
        let mut analyses = Analyses::new();
        for (id, enabled, check) in checks {
            if enabled {
                let result = check(&project, &mut analyses)?;
                report.rules.insert(id, result.status);
                report.findings.extend(result.findings);
            } else {
                report.rules.insert(id, Status::Disabled);
            }
        }
        crate::directive::apply(
            &mut report,
            analyses
                .values()
                .flat_map(|(_, directives)| directives.iter().cloned())
                .collect(),
            self.rules.keys().copied().collect(),
        );
        report.findings.sort_by(|left, right| {
            (&left.path, left.rule, &left.configuration, &left.message).cmp(&(
                &right.path,
                right.rule,
                &right.configuration,
                &right.message,
            ))
        });
        Ok(report)
    }
}

fn prepare<R: Rule>(config: Option<toml::Value>) -> Result<Check, Error> {
    let config = match config {
        None => R::Config::default(),
        Some(value) => value
            .try_into()
            .map_err(|error| Error::Configuration(format!("rules.{}: {error}", R::ID)))?,
    };
    let rule = R::new(config)?;
    Ok(Box::new(move |project, analyses| {
        if !rule.configured() {
            return Ok(RuleResult {
                status: Status::Unconfigured,
                findings: Vec::new(),
            });
        }
        let key = TypeId::of::<R::Analysis>();
        if let std::collections::btree_map::Entry::Vacant(entry) = analyses.entry(key) {
            let analysis = <R::Analysis as crate::Analysis>::load(project)?;
            let directives = crate::Analysis::directives(&analysis);
            entry.insert((Box::new(analysis), directives));
        }
        let analysis = analyses[&key]
            .0
            .downcast_ref::<R::Analysis>()
            .ok_or_else(|| Error::Analysis("registered analysis type mismatch".into()))?;
        rule.check(project, analysis)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Default, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct Config {
        reject: bool,
    }

    struct Custom;
    impl Rule for Custom {
        const ID: &'static str = "custom";
        type Config = Config;
        type Analysis = ();

        fn new(config: Config) -> Result<Self, Error> {
            if config.reject {
                return Err(Error::Configuration("custom rejected its settings".into()));
            }
            Ok(Self)
        }

        fn check(&self, project: &Project, _: &()) -> Result<RuleResult, Error> {
            Ok(RuleResult {
                status: Status::Completed,
                findings: vec![crate::Finding {
                    span: None,
                    related: Vec::new(),
                    rule: Self::ID,
                    path: project.entries().next().unwrap().path.clone(),
                    configuration: "rules.custom.config".into(),
                    message: "custom finding".into(),
                    instruction: "Review the custom policy.".into(),
                }],
            })
        }
    }

    #[test]
    fn registration_runs_custom_rules_and_rejects_duplicates() {
        let root = tempfile::tempdir().unwrap();
        let registry = Registry::default().register::<Custom>().unwrap();
        let report = registry.check(root.path()).unwrap();
        assert_eq!(report.rules.len(), 1);
        assert_eq!(report.rules["custom"], Status::Completed);
        assert_eq!(report.findings[0].rule, "custom");
        assert!(registry.register::<Custom>().is_err());
    }

    #[test]
    fn disabled_custom_rules_still_validate_typed_settings() {
        let root = tempfile::tempdir().unwrap();
        let registry = Registry::default().register::<Custom>().unwrap();
        for settings in ["reject = true", "unknown = true", "reject = 'wrong type'"] {
            std::fs::write(
                root.path().join("linter.toml"),
                format!("[rules.custom]\nenabled = false\n[rules.custom.config]\n{settings}"),
            )
            .unwrap();
            assert!(matches!(
                registry.check(root.path()),
                Err(Error::Configuration(_))
            ));
        }
        std::fs::write(
            root.path().join("linter.toml"),
            "[rules.custom]\nenabled = false",
        )
        .unwrap();
        let report = registry.check(root.path()).unwrap();
        assert_eq!(report.rules["custom"], Status::Disabled);
        assert!(report.findings.is_empty());
    }
    #[test]
    fn analysis_is_shared_within_a_run_and_reloaded_between_runs() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static LOADS: AtomicUsize = AtomicUsize::new(0);
        struct Counted;
        impl crate::Analysis for Counted {
            fn load(_: &Project) -> Result<Self, Error> {
                LOADS.fetch_add(1, Ordering::SeqCst);
                Ok(Self)
            }
        }
        struct Probe<const N: usize>;
        impl<const N: usize> Rule for Probe<N> {
            const ID: &'static str = if N == 0 { "first" } else { "second" };
            type Config = Config;
            type Analysis = Counted;
            fn new(_: Config) -> Result<Self, Error> {
                Ok(Self)
            }
            fn check(&self, _: &Project, _: &Counted) -> Result<RuleResult, Error> {
                Ok(RuleResult {
                    status: Status::Completed,
                    findings: Vec::new(),
                })
            }
        }
        let root = tempfile::tempdir().unwrap();
        let registry = Registry::default()
            .register::<Probe<0>>()
            .unwrap()
            .register::<Probe<1>>()
            .unwrap();
        registry.check(root.path()).unwrap();
        assert_eq!(LOADS.load(Ordering::SeqCst), 1);
        registry.check(root.path()).unwrap();
        assert_eq!(LOADS.load(Ordering::SeqCst), 2);
    }
}
