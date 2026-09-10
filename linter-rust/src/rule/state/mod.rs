use std::collections::{BTreeMap, BTreeSet};

use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};

use crate::{
    Analysis, Source,
    declaration::Index,
    scope::{integration, mark_tests},
};
use config::Assertion;

mod config;
mod scan;
mod syntax;
pub use config::Config;

pub struct StringState(Vec<Assertion>);

impl Rule for StringState {
    const ID: &'static str = "rust/string-backed-finite-state";
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
        let mut findings = Vec::new();
        for assertion in &self.0 {
            let mut concepts = BTreeMap::new();
            for source in &analysis.sources {
                if !assertion.target.matches(&source.path)
                    || assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
                {
                    continue;
                }
                let mut tests = vec![false; source.text.len()];
                if integration(source, &root, analysis) {
                    tests.fill(true);
                } else {
                    mark_tests(source.syntax.root_node(), &source.text, &mut tests);
                }
                scan::Scanner::new(source, &index, assertion, &tests, &mut concepts).run();
            }
            findings.extend(
                concepts
                    .into_values()
                    .filter_map(|concept| concept.finding(assertion)),
            );
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Field,
    Binding,
    Target,
}
#[derive(Clone, Copy)]
enum Use {
    Assignment,
    Decision,
}
struct Concept {
    name: String,
    kind: Kind,
    values: BTreeSet<String>,
    assignments: usize,
    decisions: usize,
    open: bool,
    evidence: Vec<Evidence>,
}
impl Concept {
    fn new(name: String, kind: Kind) -> Self {
        Self {
            name,
            kind,
            values: BTreeSet::new(),
            assignments: 0,
            decisions: 0,
            open: false,
            evidence: Vec::new(),
        }
    }
    fn record(&mut self, value: String, node: tree_sitter::Node<'_>, source: &Source, usage: Use) {
        self.values.insert(value.clone());
        let action = match usage {
            Use::Assignment => {
                self.assignments += 1;
                "Assigned"
            }
            Use::Decision => {
                self.decisions += 1;
                "Compared"
            }
        };
        self.evidence.push(Evidence {
            path: source.path.clone(),
            span: Some(Span::new(&source.text, node.byte_range())),
            message: format!("{action} state value {value:?}"),
        });
    }
    fn finding(mut self, assertion: &Assertion) -> Option<Finding> {
        let persistent = match self.kind {
            Kind::Field => self.assignments >= assertion.min_variants || self.decisions > 0,
            Kind::Binding => self.assignments > 0 && self.decisions > 0,
            Kind::Target => self.assignments >= assertion.min_variants,
        };
        if self.open || !persistent || self.values.len() < assertion.min_variants {
            return None;
        }
        self.evidence.sort_by(|left, right| {
            left.path.cmp(&right.path).then(
                left.span
                    .as_ref()
                    .map(|span| span.start)
                    .cmp(&right.span.as_ref().map(|span| span.start)),
            )
        });
        let first = self.evidence.first()?;
        Some(Finding { rule: StringState::ID, path: first.path.clone(), span: first.span.clone(), related: self.evidence.clone(), configuration: assertion.setting.clone(), message: format!("`{}` uses {} string literals as a finite state vocabulary: {}", self.name, self.values.len(), self.values.into_iter().collect::<Vec<_>>().join(", ")), instruction: "Represent the closed vocabulary with an enum; parse and serialize strings at the boundary.".into() })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    const WORDS: &str = "state_words=['state','status','phase','mode','kind','stage','condition','lifecycle','action']\nignored_words=['text','message','description','detail','error','name','title','label','path','url','uri','id','identifier','reference','command','query','header','body','content','log','output','input','format','mime','media','user','token','key','value','raw']";
    fn configured(relative: &str, source: &str, config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!(
                "[[rules.\"rust/string-backed-finite-state\"]]\ntarget='**/*.rs'\n{WORDS}\n{config}"
            ),
        )
        .unwrap();
        linter::Registry::default()
            .register::<StringState>()?
            .check(root.path())
            .map(|report| report.findings)
    }
    fn findings(source: &str) -> Vec<Finding> {
        findings_in("src/lib.rs", source)
    }
    fn findings_in(relative: &str, source: &str) -> Vec<Finding> {
        configured(relative, source, "").unwrap()
    }
    #[test]
    fn reports_field_states() {
        let findings = findings(
            r#"
struct Upload { status: String }
impl Upload {
    fn finished(&self) -> bool {
        match self.status.as_str() {
            "preparing" => false,
            "pushing" => false,
            "pushed" => true,
            _ => false,
        }
    }
}
"#,
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("status"));
        assert!(findings[0].message.contains("3 string literals"));
    }

    #[test]
    fn reports_three_values() {
        let findings = findings(
            r#"
fn ready(status: &str) -> bool {
    let mut phase = status;
    phase = "pending";
    phase = "running";
    phase = "complete";
    phase == "pending" || phase == "running" || phase == "complete"
}
"#,
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("phase"));
    }

    #[test]
    fn ignores_assignments_decisions() {
        let findings = findings(
            r#"
fn binary(status: &str) -> bool {
    status == "on" || status == "off"
}
fn labels() {
    let mut phase = "one";
    phase = "two";
    phase = "three";
}
"#,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn reports_states_construction() {
        let findings = findings(
            r#"
struct Lifecycle { state: &'static str }
fn lifecycle(code: u8) -> Lifecycle {
    match code {
        0 => Lifecycle { state: "created" },
        1 => Lifecycle { state: "running" },
        2 => Lifecycle { state: "paused" },
        _ => Lifecycle { state: "exited" },
    }
}
"#,
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("state"));
    }

    #[test]
    fn reports_setter_targets() {
        let findings = findings(
            r#"
struct Job;
impl Job {
    fn set_status(&mut self, _: &str) {}
}
fn advance(job: &mut Job) {
    job.set_status("queued");
    job.set_status("running");
    job.set_status("complete");
}
"#,
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("status"));
    }

    #[test]
    fn ignores_identifiers_paths() {
        let findings = findings(
            r#"
fn render(message: &str, path: &str, id: &str) -> bool {
    message == "starting" || message == "running" || message == "done"
        || path == "/a" || path == "/b" || path == "/c"
        || id == "one" || id == "two" || id == "three"
}
"#,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_test_modules() {
        let findings = findings(
            r#"
#[cfg(test)]
mod tests {
    fn state(status: &str) -> bool {
        matches!(status, "a" | "b" | "c")
    }
}
"#,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn reports_protocol_namespace() {
        let findings = findings_in(
            "src/protocol/status.rs",
            r#"
pub struct Transfer { status: String }
impl Transfer {
    pub fn complete(&self) -> bool {
        match self.status.as_str() {
            "preparing" => false,
            "sending" => false,
            "complete" => true,
            _ => self.status.is_empty(),
        }
    }
}
"#,
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("status"));
    }

    #[test]
    fn ignores_unknown_value() {
        let findings = findings_in(
            "src/protocol/status.rs",
            r#"
pub enum Status {
    Preparing,
    Sending,
    Complete,
    Unknown(String),
}
pub struct Transfer { status: String }
impl Transfer {
    pub fn status(&self) -> Status {
        match self.status.as_str() {
            "preparing" => Status::Preparing,
            "sending" => Status::Sending,
            "complete" => Status::Complete,
            unknown => Status::Unknown(unknown.to_owned()),
        }
    }
}
"#,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn location_vocabulary_evidence() {
        let findings = findings(
            r#"
struct Process { phase: String }
impl Process {
fn transition(&self) -> bool {
    match self.phase.as_str() {
        "queued" => false,
        "active" => false,
        "finished" => true,
        _ => false,
    }
}
}
"#,
        );
        let finding = &findings[0];
        assert_eq!(finding.span.as_ref().unwrap().line, 6);
        assert!(finding.related.len() >= 2);
        assert!(finding.message.contains("finished"));
    }
    const CLOSED: &str = "struct Upload { status:String } impl Upload { fn ready(&self)->bool { match self.status.as_str() {\"a\"=>true,\"b\"=>false,\"c\"=>false,_=>false} } }";

    #[test]
    fn distinct_owners_bindings_and_setter_receivers_never_combine() {
        let first = CLOSED.replace(",\"c\"=>false", "");
        let second = first.replace("\"a\"", "\"d\"").replace("\"b\"", "\"e\"");
        assert!(findings(&format!("mod first{{{first}}} mod second{{{second}}}")).is_empty());
        assert!(findings("fn sample(){let mut phase=\"a\"; phase==\"b\"; {let mut phase=\"c\"; phase==\"d\";}}").is_empty());
        assert!(findings("struct Job; fn update(a:&mut Job,b:&mut Job){a.set_status(\"a\");a.set_status(\"b\");b.set_status(\"c\");b.set_status(\"d\");}").is_empty());
    }

    #[test]
    fn unknown_preservation_requires_an_unguarded_fallback() {
        let open = "struct Upload { status:String } impl Upload { fn status(&self)->String { match self.status.as_str() {\"a\"=>\"one\".into(),\"b\"=>\"two\".into(),\"c\"=>\"three\".into(),_=>self.status.clone()} } }";
        assert!(findings(open).is_empty());
        let discarded = open.replace(
            "_=>self.status.clone()",
            "unknown=>{ log(unknown); Status::Unknown(String::new()) }",
        );
        assert_eq!(findings(&discarded).len(), 1);
        let guarded = open.replace(
            "_=>self.status.clone()",
            "unknown if unknown.len()>4=>Status::Unknown(unknown.to_owned()),_=>String::new()",
        );
        assert_eq!(findings(&guarded).len(), 1);
        assert!(
            findings("fn render(input:&str)->bool {input==\"a\"||input==\"b\"||input==\"c\"}")
                .is_empty()
        );
    }

    #[test]
    fn thresholds_exclusions_and_test_scope_apply() {
        assert!(
            configured("src/lib.rs", CLOSED, "min_variants=4")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            configured(
                "src/lib.rs",
                "fn process(){let phase=\"a\";phase==\"b\";}",
                "min_variants=2"
            )
            .unwrap()
            .len(),
            1
        );
        assert!(
            configured("src/lib.rs", CLOSED, "exclude='src/**'")
                .unwrap()
                .is_empty()
        );
        assert!(configured("tests/input.rs", CLOSED, "").unwrap().is_empty());
        assert_eq!(
            configured("tests/input.rs", CLOSED, "scope='tests'")
                .unwrap()
                .len(),
            1
        );
        assert!(findings(&format!("#[cfg(test)] mod checks {{{CLOSED}}}")).is_empty());
        assert_eq!(
            configured(
                "src/lib.rs",
                &format!("#[cfg(test)] mod checks {{{CLOSED}}}"),
                "scope='all'"
            )
            .unwrap()
            .len(),
            1
        );
    }

    #[test]
    fn custom_string_nominal_types_and_arbitrary_calls_do_not_qualify() {
        assert!(findings(&format!("struct String(u8); {CLOSED}")).is_empty());
        assert!(findings("fn process(){let mut phase=decode(\"a\");phase=decode(\"b\");phase=decode(\"c\");phase==decode(\"a\");}").is_empty());
    }

    #[test]
    fn rejects_bad_settings_and_requires_explicit_vocabulary() {
        for config in [
            "min_variants=1",
            "min_variants=-1",
            "min_variants='three'",
            "scope='unknown'",
            "exclude=[]",
            "unknown=true",
        ] {
            assert!(matches!(
                configured("lib.rs", "", config),
                Err(Error::Configuration(_))
            ));
        }
        let root = tempfile::tempdir().unwrap();
        for config in [
            "",
            "state_words=['status','STATUS']",
            "state_words=['two words']",
            "state_words=['status']\nignored_words=['']",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!(
                    "[[rules.\"rust/string-backed-finite-state\"]]\ntarget='**/*.rs'\n{config}"
                ),
            )
            .unwrap();
            assert!(matches!(
                linter::Registry::default()
                    .register::<StringState>()
                    .unwrap()
                    .check(root.path()),
                Err(Error::Configuration(_))
            ));
        }
    }

    #[test]
    fn configured_vocabulary_and_ignored_words_control_concepts() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("lib.rs"),
            CLOSED.replace("status", "progress"),
        )
        .unwrap();
        for (ignored, count) in [("[]", 1), ("['progress']", 0)] {
            fs::write(root.path().join("linter.toml"),format!("[[rules.\"rust/string-backed-finite-state\"]]\ntarget='*.rs'\nstate_words=['PROGRESS']\nignored_words={ignored}")).unwrap();
            let report = linter::Registry::default()
                .register::<StringState>()
                .unwrap()
                .check(root.path())
                .unwrap();
            assert_eq!(report.findings.len(), count);
        }
    }

    #[test]
    fn implementation_and_reasoned_directives_pass() {
        for source in [
            include_str!("mod.rs"),
            include_str!("scan.rs"),
            include_str!("syntax.rs"),
            include_str!("config.rs"),
        ] {
            assert!(findings(source).is_empty());
        }
        let suppressed=CLOSED.replace("fn ready", "// linter:disable rust/string-backed-finite-state -- wire contract preserves string states\nfn ready");
        assert!(findings(&suppressed).is_empty());
    }
}
