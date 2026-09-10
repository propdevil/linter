use crate::{
    Cases, Finding, LintError, Linter, Policy, Registry, Reporter, Result, Review, Rule, Severity,
    Summary, source::Workspace, test_support::Fixture,
};
use std::{cell::RefCell, fs, rc::Rc};

struct Probe {
    id: &'static str,
    severity: Severity,
    fail: bool,
}
impl Rule for Probe {
    fn id(&self) -> &'static str {
        self.id
    }
    fn severity(&self) -> Severity {
        self.severity
    }
    fn check(&self, workspace: &Workspace, _: &Policy) -> Result<Vec<Finding>> {
        if self.fail {
            return Err(LintError::configuration("injected rule failure"));
        }
        let mut finding = Finding::error(
            self.id,
            "subject",
            workspace.sources()[0].location(proc_macro2::Span::call_site()),
        );
        finding.severity = self.severity;
        finding.review = Some(Review::default());
        Ok(vec![finding])
    }
}
#[derive(Default)]
struct Collected {
    findings: Vec<Finding>,
    began: bool,
}
impl Reporter for Collected {
    fn begin(&mut self, _: &Workspace) -> Result<()> {
        self.began = true;
        Ok(())
    }
    fn finding(&mut self, finding: &Finding) -> Result<()> {
        self.findings.push(finding.clone());
        Ok(())
    }
    fn finish(&mut self, _: &[Summary]) -> Result<()> {
        Ok(())
    }
}

struct ObservedRule {
    id: &'static str,
    calls: Rc<RefCell<Vec<&'static str>>>,
}

impl Rule for ObservedRule {
    fn id(&self) -> &'static str {
        self.id
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, _: &Workspace, _: &Policy) -> Result<Vec<Finding>> {
        self.calls.borrow_mut().push(self.id);
        Ok(Vec::new())
    }
}

#[test]
fn registry_runs_rules_in_registration_order() {
    let fixture = Fixture::new(&[("src/lib.rs", "const VALUE: u8 = 1;")]);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let registry = Registry::new()
        .register(ObservedRule {
            id: "second",
            calls: calls.clone(),
        })
        .register(ObservedRule {
            id: "first",
            calls: calls.clone(),
        });
    let summaries = Linter::new(Policy::default(), registry)
        .run(vec![fixture.path().to_owned()], &mut Collected::default())
        .unwrap();
    assert_eq!(*calls.borrow(), ["second", "first"]);
    assert_eq!(
        summaries
            .iter()
            .map(|summary| summary.rule)
            .collect::<Vec<_>>(),
        ["second", "first"]
    );
}

#[test]
fn invalid_registry_fails_before_source_loading_or_execution() {
    let fixture = Fixture::new(&[
        ("valid.rs", "const VALUE: u8 = 1;"),
        ("invalid.rs", "not Rust source"),
    ]);
    for ids in [["valid", "same", "same"], ["valid", "other", ""]] {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let registry = ids.into_iter().fold(Registry::new(), |registry, id| {
            registry.register(ObservedRule {
                id,
                calls: calls.clone(),
            })
        });
        assert!(registry.validate().is_err());
        let linter = Linter::new(Policy::default(), registry);
        for path in ["valid.rs", "invalid.rs"] {
            let mut reporter = Collected::default();
            let error = linter
                .run(vec![fixture.path().join(path)], &mut reporter)
                .unwrap_err();
            assert!(error.to_string().contains("empty or duplicate rule ID"));
            assert!(calls.borrow().is_empty());
            assert!(!reporter.began);
        }
    }
}

#[test]
fn full_and_configured_catalogs_keep_their_existing_order() {
    let expected = [
        "dependency-direction",
        "owned-vocabulary",
        "file-length",
        "forbidden-path",
        "empty-directory",
        "chain-layout",
        "single-file-directory",
        "trait-method-count",
        "empty-struct",
        "struct-word-count",
        "self-constructor-static",
        "receiver-name-repetition",
        "catch-all-module-name",
        "struct-noun-naming",
        "unclassified-free-function",
        "single-use-free-function",
        "deep-control-flow",
        "environment-variable-access",
        "platform-command-boundary",
        "ignored-fallible-result",
        "async-blocking-operation",
        "boolean-state-cluster",
        "string-backed-finite-state",
        "god-object-growth",
        "redundant-accessor",
        "duplicate-entity-base",
        "wire-domain-model-duplication",
        "ceremonial-structure",
    ];
    assert_eq!(
        Registry::all()
            .unwrap()
            .iter()
            .map(Rule::id)
            .collect::<Vec<_>>(),
        expected
    );
    let policy: Policy = toml::from_str(include_str!("../../../lint.toml")).unwrap();
    let expected = expected
        .into_iter()
        .filter(|id| {
            ![
                "receiver-name-repetition",
                "struct-noun-naming",
                "unclassified-free-function",
                "duplicate-entity-base",
            ]
            .contains(id)
        })
        .collect::<Vec<_>>();
    let linter = Linter::standard_with_policy(policy).unwrap();
    assert_eq!(
        linter.registry.iter().map(Rule::id).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(linter.registry.iter().count(), 24);
}

#[test]
fn registry_keeps_sdk_rules_and_policy_selection() {
    let policy = Policy::default();
    assert_eq!(Registry::standard(&policy).unwrap().iter().count(), 11);
    assert_eq!(Registry::all().unwrap().iter().count(), 28);
    let mut policy = policy;
    policy.rules.enabled = vec!["missing".into()];
    assert!(Registry::standard(&policy).is_err());
    policy.rules.enabled = vec!["single-use-free-function".into()];
    let registry = Registry::standard(&policy).unwrap();
    assert_eq!(registry.iter().count(), 12);
    assert_eq!(
        registry
            .iter()
            .filter(|rule| rule.severity() == Severity::Error)
            .count(),
        11
    );
}
#[test]
fn review_evidence_never_hides_errors_or_warnings() {
    let fixture = Fixture::new(&[("src/lib.rs", "const VALUE: u8 = 1;")]);
    for severity in [Severity::Error, Severity::Warning] {
        let registry = Registry::new().register(Probe {
            id: "probe",
            severity,
            fail: false,
        });
        let mut reporter = Collected::default();
        let summary = Linter::new(Policy::default(), registry)
            .run(vec![fixture.path().to_owned()], &mut reporter)
            .unwrap();
        assert!(reporter.began);
        assert_eq!(summary[0].findings, 1);
        assert_eq!(summary[0].severity, severity);
        assert!(reporter.findings[0].is_violation());
    }
}
#[test]
fn failed_analysis_leaves_existing_cases_untouched() {
    let fixture = Fixture::new(&[
        ("src/lib.rs", "const VALUE: u8 = 1;"),
        ("lint/errors/prior.md", "prior evidence"),
        ("lint/check/notes.md", "user note"),
        ("lint/errors/.gitkeep", ""),
    ]);
    let registry = Registry::new().register(Probe {
        id: "fail",
        severity: Severity::Error,
        fail: true,
    });
    let mut cases = Cases::with_output(fixture.path().join("lint"), Vec::new());
    assert!(
        Linter::new(Policy::default(), registry)
            .run(vec![fixture.path().join("src")], &mut cases)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("lint/errors/prior.md")).unwrap(),
        "prior evidence"
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("lint/check/notes.md")).unwrap(),
        "user note"
    );
    assert!(fixture.path().join("lint/errors/.gitkeep").exists());
}
#[test]
fn only_reasoned_comments_suppress_adopted_findings() {
    for (prefix, count) in [
        (
            "// design-lint: allow struct-noun-naming -- intentional command vocabulary",
            0,
        ),
        ("// design-lint: allow struct-noun-naming -- ", 1),
        (
            "const NOTE: &str = \"// design-lint: allow struct-noun-naming -- literal\";",
            1,
        ),
    ] {
        let text = format!("{prefix}\nstruct Selected {{ id: u8 }}");
        let fixture = Fixture::new(&[("src/lib.rs", &text)]);
        let mut policy = Policy::default();
        policy.rules.enabled.push("struct-noun-naming".into());
        let mut reporter = Collected::default();
        Linter::standard_with_policy(policy)
            .unwrap()
            .run(vec![fixture.path().to_owned()], &mut reporter)
            .unwrap();
        assert_eq!(
            reporter
                .findings
                .iter()
                .filter(|finding| finding.rule == "struct-noun-naming")
                .count(),
            count,
            "{prefix}"
        );
    }
}
#[test]
fn boundary_selectors_have_stable_scope_and_reject_invalid_policy() {
    let fixture = Fixture::new(&[
        ("Cargo.toml", "[workspace]\nmembers=['apps/api']"),
        (
            "apps/api/Cargo.toml",
            "[package]\nname='api'\nversion='0.0.0'",
        ),
        (
            "apps/api/src/config.rs",
            "fn load() { std::env::var(\"TOKEN\"); }",
        ),
    ]);
    let selector = crate::SourceSelector {
        package: Some("api".into()),
        path: Some("apps/api/src/config.rs".into()),
    };
    for path in [
        fixture.path().to_owned(),
        fixture.path().join("apps/api/src/config.rs"),
    ] {
        let workspace = Workspace::load(vec![path], &Policy::default().source).unwrap();
        assert!(selector.matches(&workspace.sources()[0], &workspace));
    }
    for text in [
        "[[boundaries.environment]]",
        "[[boundaries.process]]\npath='../escape'",
        "[[dependency.layers]]\nname='a'\nmay_depend_on=['unknown']",
        "[rules]\nenabled=['missing']",
    ] {
        let policy: Policy = toml::from_str(text).unwrap();
        assert!(policy.validate().is_err(), "{text}");
    }
}
