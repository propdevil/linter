use crate::{
    Analysis, Source,
    declaration::{Index, Structure},
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use tree_sitter::Node;
mod config;
pub use config::Config;
mod evidence;
use evidence::{Observation, inspect};

pub struct BooleanState(Vec<Assertion>);
impl Rule for BooleanState {
    const ID: &'static str = "rust/boolean-state-cluster";
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
        for assertion in &self.0 {
            let mut observations = BTreeMap::new();
            for source in &analysis.sources {
                let mut tests = vec![false; source.text.len()];
                if integration(source, &root, analysis) {
                    tests.fill(true);
                } else {
                    mark_tests(source.syntax.root_node(), &source.text, &mut tests);
                }
                collect(
                    source.syntax.root_node(),
                    source,
                    &index,
                    &tests,
                    assertion,
                    &mut observations,
                );
            }
            for structure in index
                .structures
                .iter()
                .filter(|item| selected(item, assertion))
            {
                let key = format!("nominal:{}", structure.id.key());
                if let Some(observation) = observations.remove(&key)
                    && let Some(finding) = finding(structure, observation, assertion)
                {
                    findings.push(finding);
                }
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn collect(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    tests: &[bool],
    assertion: &Assertion,
    observations: &mut BTreeMap<String, Observation>,
) {
    if !matches!(assertion.scope, Scope::All)
        && tests.get(node.start_byte()) == Some(&true)
        && matches!(assertion.scope, Scope::Production)
    {
        return;
    }
    let test = tests.get(node.start_byte()) == Some(&true);
    if scope(test, assertion.scope) {
        inspect(
            node,
            source,
            index,
            assertion.min_fields,
            observations,
            tests,
            assertion.scope,
        );
    }
    for child in children(node) {
        collect(child, source, index, tests, assertion, observations);
    }
}
fn scope(test: bool, scope: Scope) -> bool {
    match scope {
        Scope::Production => !test,
        Scope::Tests => test,
        Scope::All => true,
    }
}
fn selected(item: &Structure<'_>, assertion: &Assertion) -> bool {
    assertion.target.matches(&item.source.path)
        && !assertion
            .exclude
            .as_ref()
            .is_some_and(|exclude| exclude.matches(&item.source.path))
        && scope(item.test, assertion.scope)
}
fn bool_fields(item: &Structure<'_>) -> BTreeSet<String> {
    item.fields
        .iter()
        .filter(|(_, field)| field.ty.as_deref() == Some("primitive:bool"))
        .map(|(name, _)| name.clone())
        .collect()
}
fn finding(
    item: &Structure<'_>,
    mut observation: Observation,
    assertion: &Assertion,
) -> Option<Finding> {
    let fields = bool_fields(item);
    if fields.len() < assertion.min_fields {
        return None;
    }
    let states = observation
        .literals
        .iter()
        .map(|literal| one_hot(&literal.values, &fields))
        .collect::<Option<BTreeSet<_>>>();
    if states.is_some_and(|states| states.len() >= 2) {
        observation.evidence.extend(
            observation
                .literals
                .into_iter()
                .map(|literal| (fields.clone(), literal.evidence)),
        );
    }
    if observation.evidence.is_empty() {
        return None;
    }
    let implicated: BTreeSet<_> = observation
        .evidence
        .iter()
        .flat_map(|(fields, _)| fields.iter().cloned())
        .collect();
    let mut related: Vec<_> = observation
        .evidence
        .into_iter()
        .map(|(_, evidence)| evidence)
        .collect();
    related.sort_by_key(|evidence| {
        (
            evidence.path.clone(),
            evidence.span.as_ref().map(|span| span.start),
        )
    });
    related.dedup();
    Some(Finding {
        rule: BooleanState::ID, path: item.source.path.clone(), span: Some(Span::new(&item.source.text, item.node.byte_range())), related,
        configuration: format!("{}.min_fields", assertion.setting),
        message: format!("`{}` coordinates boolean fields as mutually exclusive state: {}", item.id.name, implicated.into_iter().collect::<Vec<_>>().join(", ")),
        instruction: "Replace the coordinated boolean state with a named enum or composed state entity; keep independent capabilities as booleans.".into(),
    })
}
fn one_hot(values: &BTreeMap<String, bool>, fields: &BTreeSet<String>) -> Option<String> {
    if fields.iter().any(|field| !values.contains_key(field)) {
        return None;
    }
    let active: Vec<_> = fields
        .iter()
        .filter(|field| values.get(*field) == Some(&true))
        .collect();
    (active.len() == 1).then(|| active[0].clone())
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| {
            !matches!(
                child.kind(),
                "line_comment" | "block_comment" | "attribute_item" | "inner_attribute_item"
            )
        })
        .collect()
}
fn evidence(source: &Source, node: Node<'_>, message: String) -> Evidence {
    Evidence {
        path: source.path.clone(),
        span: Some(Span::new(&source.text, node.byte_range())),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn findings(source: &str) -> Vec<Finding> {
        configured(source, "").findings
    }
    fn configured(source: &str, options: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/boolean-state-cluster\"]]\ntarget='**/*.rs'\n{options}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<BooleanState>()
            .unwrap()
            .check(root.path())
            .unwrap()
    }
    #[test]
    fn reports_exclusive_constructions() {
        let findings = findings(
            r"
struct Connection {
    disconnected: bool,
    connecting: bool,
    connected: bool,
}
fn disconnected() -> Connection {
    Connection { disconnected: true, connecting: false, connected: false }
}
fn connected() -> Connection {
    Connection { disconnected: false, connecting: false, connected: true }
}
",
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("`Connection`"));
        assert_eq!(findings[0].related.len(), 2);
    }

    #[test]
    fn reports_state_flags() {
        let findings = findings(
            r"
struct Session { idle: bool, opening: bool, active: bool }
impl Session {
    fn activate(&mut self) {
        self.idle = false;
        self.opening = false;
        self.active = true;
    }
}
",
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].related[0].message.contains("activate"));
    }

    #[test]
    fn ignores_independent_capabilities() {
        assert!(
            findings(
                r"
struct Permissions { readable: bool, writable: bool, executable: bool }
fn owner() -> Permissions {
    Permissions { readable: true, writable: true, executable: true }
}
"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_feature_toggles() {
        assert!(
            findings(
                r"
struct Features { clipboard: bool, audio: bool, gpu: bool }
impl Features { fn disable_audio(&mut self) { self.audio = false; } }
"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_independent_features() {
        assert!(
            findings(
                r"
struct Features { clipboard: bool, audio: bool, gpu: bool }
impl Features {
    fn disable_all(&mut self) {
        self.clipboard = false;
        self.audio = false;
        self.gpu = false;
    }
}
"
            )
            .is_empty()
        );
    }

    #[test]
    fn reports_transition_methods() {
        let findings = findings(
            r"
struct Phase { queued: bool, running: bool, finished: bool }
impl Phase {
    fn start(&mut self) {
        self.queued = false;
        self.running = true;
        self.finished = false;
    }
    fn finish(&mut self) {
        self.queued = false;
        self.running = false;
        self.finished = true;
    }
}
",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].related.len(), 2);
    }

    #[test]
    fn ignores_protocol_fields() {
        assert!(
            findings(
                r"
struct ProtocolFlags { urgent: bool, acknowledged: bool, compressed: bool }
fn decode(bits: u8) -> ProtocolFlags {
    ProtocolFlags {
        urgent: bits & 1 != 0,
        acknowledged: bits & 2 != 0,
        compressed: bits & 4 != 0,
    }
}
"
            )
            .is_empty()
        );
    }

    #[test]
    fn one_is_insufficient() {
        assert!(
            findings(
                r"
struct View { loading: bool, ready: bool, failed: bool }
fn initial() -> View { View { loading: true, ready: false, failed: false } }
"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_third_flag() {
        assert!(
            findings(
                r"
struct Transfer { paused: bool, active: bool, verbose: bool }
impl Transfer {
    fn resume(&mut self) {
        self.paused = false;
        self.active = true;
    }
}
"
            )
            .is_empty()
        );
    }

    #[test]
    fn reports_invalid_combinations() {
        let findings = findings(
            r"
struct Phase { queued: bool, running: bool, finished: bool }
impl Phase {
    fn valid(&self) -> bool {
        !(self.queued && self.running)
            && !(self.running && self.finished)
    }
}
",
        );
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].related[0]
                .message
                .contains("rejects mutually active")
        );
    }

    #[test]
    fn actions_are_independent() {
        assert!(
            findings(
                r"
struct Actions { notify: bool, persist: bool, retry: bool }
fn success() -> Actions {
    Actions { notify: true, persist: true, retry: false }
}
fn failure() -> Actions {
    Actions { notify: true, persist: false, retry: true }
}
"
            )
            .is_empty()
        );
    }
    #[test]
    fn payment_test_only_constructions_and_transitions_are_excluded() {
        let source = "struct Phase { queued:bool,running:bool,finished:bool } #[test] fn combinations() { let _ = Phase { queued:true,running:false,finished:false }; let _ = Phase { queued:false,running:true,finished:false }; } impl Phase { #[cfg(test)] fn fixture(&mut self) { self.queued=false;self.running=true;self.finished=false; } }";
        assert!(findings(source).is_empty());
        assert_eq!(configured(source, "scope='all'").findings.len(), 1);
    }
    #[test]
    fn payment_synchronizer_evidence_is_an_error_with_method_location() {
        let found = findings(
            "struct Synchronizer { catching_up:bool,ready:bool,failed:bool } impl Synchronizer { fn ready(&mut self) { self.catching_up=false; self.ready=true; self.failed=false; } }",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, BooleanState::ID);
        assert!(found[0].related[0].message.contains("ready"));
        assert!(found[0].related[0].span.is_some());
    }
    #[test]
    fn mutually_exclusive_paths_and_independent_conjunctions_are_not_combined() {
        let source = "struct State { a:bool,b:bool,c:bool } impl State { fn work(&mut self,x:bool) { if x { self.a=true; } else { self.b=false;self.c=false; } } fn valid(&self)->bool { (self.a&&self.b)&&(self.b&&self.c) } }";
        assert!(findings(source).is_empty());
        assert!(
            findings(&source.replace(
                "(self.a&&self.b)&&(self.b&&self.c)",
                "!!(self.a&&self.b)&&!!(self.b&&self.c)"
            ))
            .is_empty()
        );
    }
    #[test]
    fn contradictory_or_partial_constructions_do_not_prove_exclusion() {
        let source = "struct State { a:bool,b:bool,c:bool } fn a()->State { State { a:true,b:false,c:false } } fn b()->State { State { a:false,b:true,c:false } } fn c(x:bool)->State { State { a:x,b:false,c:true } }";
        assert!(findings(source).is_empty());
        assert!(findings(&source.replace("a:x", "a:true")).is_empty());
    }
    #[test]
    fn nominal_namespaces_and_local_shadowing_keep_constructions_separate() {
        let source = "mod a { pub struct State { pub a:bool,pub b:bool,pub c:bool } } mod b { pub struct State { pub a:bool,pub b:bool,pub c:bool } } fn one() { let _=a::State { a:true,b:false,c:false }; let _=b::State { a:false,b:true,c:false }; }";
        assert!(findings(source).is_empty());
        let source = "struct State { a:bool,b:bool,c:bool } fn a() { let _=State { a:true,b:false,c:false }; } fn b() { struct State { a:bool,b:bool,c:bool } let _=State { a:false,b:true,c:false }; }";
        assert!(findings(source).is_empty());
    }
    #[test]
    fn self_constructors_and_definitions_after_impls_are_resolved() {
        let source = "impl State { fn a()->Self { Self { a:true,b:false,c:false } } fn b()->Self { Self { a:false,b:true,c:false } } } struct State { a:bool,b:bool,c:bool }";
        assert_eq!(findings(source).len(), 1);
    }
    #[test]
    fn thresholds_selectors_and_directives_are_enforced() {
        let source = "struct State { a:bool,b:bool } impl State { fn a(&mut self) { self.a=true; self.b=false; } }";
        assert!(findings(source).is_empty());
        assert_eq!(configured(source, "min_fields=2").findings.len(), 1);
        assert!(
            configured(source, "min_fields=2\nexclude='lib.rs'")
                .findings
                .is_empty()
        );
        let source = format!(
            "// linter:disable rust/boolean-state-cluster -- External protocol fixes these state fields.\n{source}"
        );
        assert_eq!(configured(&source, "min_fields=2").suppressed.len(), 1);
        for setting in [
            "min_fields=1",
            "min_fields=0",
            "min_fields=-1",
            "minimum=3",
            "scope='maybe'",
            "exclude=[]",
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/boolean-state-cluster\"]]\ntarget='**/*.rs'\n{setting}"),
            )
            .unwrap();
            assert!(matches!(
                linter::Registry::default()
                    .register::<BooleanState>()
                    .unwrap()
                    .check(root.path()),
                Err(Error::Configuration(_))
            ));
        }
    }
    #[test]
    fn own_implementation_has_no_boolean_state_clusters() {
        assert!(findings(include_str!("mod.rs")).is_empty());
        assert!(findings(include_str!("evidence.rs")).is_empty());
        assert!(findings(include_str!("config.rs")).is_empty());
    }
}
