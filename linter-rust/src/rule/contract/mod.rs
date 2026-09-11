use crate::{
    Analysis, Source,
    declaration::Index,
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
mod profile;
pub use config::Config;
use profile::{cohesive, methods};
pub struct BroadTrait(Vec<Assertion>);
impl Rule for BroadTrait {
    const ID: &'static str = "rust/broad-trait-responsibilities";
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
        let masks: Vec<_> = analysis
            .sources
            .iter()
            .map(|source| {
                let mut mask = vec![false; source.text.len()];
                if integration(source, &root, analysis) {
                    mask.fill(true);
                } else {
                    mark_tests(source.syntax.root_node(), &source.text, &mut mask);
                }
                mask
            })
            .collect();
        let mut findings = Vec::new();
        for assertion in &self.0 {
            let implementors = implementors(analysis, &index, &masks, assertion.scope);
            for (source, mask) in analysis.sources.iter().zip(&masks).filter(|(source, _)| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                inspect(
                    source.syntax.root_node(),
                    source,
                    mask,
                    assertion,
                    &index,
                    &implementors,
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
    mask: &[bool],
    assertion: &Assertion,
    index: &Index<'_>,
    implementors: &BTreeMap<String, Vec<Evidence>>,
    findings: &mut Vec<Finding>,
) {
    if node.kind() == "trait_item"
        && included(node, mask, assertion.scope)
        && !excluded(node, source, assertion)
        && let Some(mut finding) = finding(node, source, mask, assertion)
    {
        let key = format!("nominal:{}", index.identity(source, node).key());
        finding
            .related
            .extend(implementors.get(&key).into_iter().flatten().cloned());
        findings.push(finding);
    }
    for child in children(node) {
        inspect(
            child,
            source,
            mask,
            assertion,
            index,
            implementors,
            findings,
        );
    }
}
fn finding(
    node: Node<'_>,
    source: &Source,
    mask: &[bool],
    assertion: &Assertion,
) -> Option<Finding> {
    let methods = methods(node, source, mask, assertion);
    if methods.len() < assertion.min_methods {
        return None;
    }
    let name = &source.text[node.child_by_field_name("name")?.byte_range()];
    let clusters = profile::Clusters::new(&methods, assertion.min_methods_per_cluster);
    if clusters.count() < assertion.min_clusters
        || clusters.method_count() < methods.len().div_ceil(2)
        || cohesive(name, &methods, assertion)
        || clusters.separated() < assertion.min_clusters - 1
    {
        return None;
    }
    let summary = clusters.summary();
    Some(Finding {
        rule: BroadTrait::ID,
        path: source.path.clone(),
        span: Some(Span::new(&source.text, node.byte_range())),
        related: methods
            .iter()
            .map(|method| method.evidence(source))
            .collect(),
        configuration: format!("{}.min_clusters", assertion.setting),
        message: format!(
            "`{name}` has {} methods spanning {} distinct capability cluster\
            s; {summary}",
            methods.len(),
            clusters.count()
        ),
        instruction: "Split the contract into cohesive capabilities that consumers can re\
            quest independently; retain composition only where the complete set is requi\
            red."
            .into(),
    })
}
fn included(node: Node<'_>, mask: &[bool], scope: Scope) -> bool {
    let test = mask.get(node.start_byte()) == Some(&true);
    match scope {
        Scope::Production => !test,
        Scope::Tests => test,
        Scope::All => true,
    }
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}
fn excluded(node: Node<'_>, source: &Source, assertion: &Assertion) -> bool {
    let header = &source.text[node.start_byte()
        ..node
            .child_by_field_name("body")
            .map_or(node.end_byte(), |body| body.start_byte())];
    if header.split_whitespace().any(|word| word == "unsafe") {
        return true;
    }
    if node.child_by_field_name("bounds").is_some_and(|bounds| {
        children(bounds)
            .iter()
            .any(|bound| source.text[bound.byte_range()].split("::").last() == Some("Sealed"))
    }) {
        return true;
    }
    if source.text.lines().take(8).any(|line| {
        assertion
            .generated_markers
            .iter()
            .any(|marker| line.contains(marker))
    }) {
        return true;
    }
    std::iter::successors(node.prev_named_sibling(), |node| node.prev_named_sibling())
        .take_while(|node| {
            matches!(
                node.kind(),
                "attribute_item" | "line_comment" | "block_comment"
            )
        })
        .filter(|node| node.kind() == "attribute_item")
        .filter_map(|node| {
            node.named_child(0)
                .and_then(|attribute| attribute.named_child(0))
        })
        .any(|name| {
            assertion
                .generated_attributes
                .iter()
                .any(|expected| expected == &source.text[name.byte_range()])
        })
}
fn implementors(
    analysis: &Analysis,
    index: &Index<'_>,
    masks: &[Vec<bool>],
    scope: Scope,
) -> BTreeMap<String, Vec<Evidence>> {
    let mut found = BTreeMap::new();
    for (source, mask) in analysis.sources.iter().zip(masks) {
        implementation(
            source.syntax.root_node(),
            source,
            index,
            mask,
            scope,
            &mut found,
        );
    }
    found
}
fn implementation(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    mask: &[bool],
    scope: Scope,
    found: &mut BTreeMap<String, Vec<Evidence>>,
) {
    if node.kind() == "impl_item"
        && included(node, mask, scope)
        && let Some(trait_node) = node.child_by_field_name("trait")
    {
        let trait_node = if trait_node.kind() == "generic_type" {
            trait_node.child_by_field_name("type").unwrap_or(trait_node)
        } else {
            trait_node
        };
        if let Some(key) = index.resolve(source, trait_node, &index.identity(source, node)) {
            let owner = node
                .child_by_field_name("type")
                .map(|owner| &source.text[owner.byte_range()])
                .unwrap_or_default();
            found.entry(key).or_default().push(Evidence {
                path: source.path.clone(),
                span: Some(Span::new(&source.text, node.byte_range())),
                message: format!("implemented by `{owner}`"),
            });
        }
    }
    for child in children(node) {
        implementation(child, source, index, mask, scope, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> &'static str {
        // Fixed regression vocabulary; independent of user-facing examples.
        concat!(
            "[[rules.\"rust/broad-trait-responsibilities\"]]\ntarget = \"**/*.r",
            "s\"\nmin_methods = 8\nmin_clusters = 3\nmin_methods_per_cluster = ",
            "2\nignored_type_words = [\"result\", \"option\", \"vec\", \"box\", \"arc",
            "\", \"dyn\", \"impl\", \"where\", \"send\", \"sync\", \"static\", \"error\", ",
            "\"bool\", \"str\", \"string\", \"usize\", \"isize\", \"u8\", \"u16\", \"u32\",",
            " \"u64\", \"u128\", \"i8\", \"i16\", \"i32\", \"i64\", \"i128\"]\ncohesive_su",
            "ffixes = [\"Protocol\", \"Codec\", \"Visitor\", \"Renderer\", \"Command",
            "s\"]\ngenerated_markers = [\"@generated\", \"automatically generate",
            "d\"]\ngenerated_attributes = [\"automatically_derived\", \"proc_mac",
            "ro_derive\"]\ncapabilities = [\n  { name = \"persistence\", verbs =",
            " [\"create\", \"open\", \"read\", \"write\", \"save\", \"load\", \"delete\",",
            " \"remove\", \"list\", \"find\", \"get\", \"put\"] },\n  { name = \"lifecy",
            "cle\", verbs = [\"start\", \"stop\", \"pause\", \"resume\", \"restart\", ",
            "\"kill\", \"launch\", \"terminate\"] },\n  { name = \"observation\", ve",
            "rbs = [\"inspect\", \"status\", \"stats\", \"health\", \"metrics\", \"des",
            "cribe\", \"query\"], nouns = [\"metric\", \"metrics\", \"stat\", \"stats",
            "\", \"status\", \"health\"] },\n  { name = \"configuration\", verbs = ",
            "[\"configure\", \"set\", \"update\", \"apply\", \"reset\", \"enable\", \"di",
            "sable\"] },\n  { name = \"events\", verbs = [\"subscribe\", \"unsubsc",
            "ribe\", \"watch\", \"emit\", \"notify\", \"poll\"], nouns = [\"event\", \"",
            "events\", \"notification\", \"notifications\"] },\n  { name = \"trans",
            "fer\", verbs = [\"upload\", \"download\", \"push\", \"pull\", \"import\",",
            " \"export\", \"copy\"] },\n  { name = \"authorization\", verbs = [\"lo",
            "gin\", \"logout\", \"authenticate\", \"authorize\", \"grant\", \"revoke\"",
            "], nouns = [\"auth\", \"permission\", \"permissions\", \"credential\",",
            " \"credentials\"] },\n  { name = \"connection\", verbs = [\"connect\"",
            ", \"disconnect\", \"bind\", \"listen\", \"accept\", \"send\", \"receive\"]",
            " },\n  { name = \"rendering\", verbs = [\"render\", \"draw\", \"presen",
            "t\", \"commit\", \"frame\", \"paint\"] },\n  { name = \"traversal\", ver",
            "bs = [\"visit\", \"walk\", \"fold\", \"traverse\"] },\n  { name = \"code",
            "c\", verbs = [\"encode\", \"decode\", \"serialize\", \"deserialize\", \"",
            "parse\", \"format\"] },\n  { name = \"clipboard\", nouns = [\"clipboa",
            "rd\"] },\n  { name = \"window\", nouns = [\"window\", \"windows\", \"su",
            "rface\", \"interaction\"] },\n]\n",
        )
    }
    fn check(source: &str, suffix: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("{}\n{suffix}", policy()),
        )
        .unwrap();
        linter::Registry::default()
            .register::<BroadTrait>()
            .unwrap()
            .check(root.path())
            .unwrap()
    }
    fn findings(source: &str) -> Vec<Finding> {
        check(source, "").findings
    }

    #[test]
    fn reports_capability_clusters() {
        let found = findings(
            r#"
pub trait RuntimeService {
    fn create(&self, config: Config) -> Result<Id, Error>;
    fn remove(&self, id: Id) -> Result<(), Error>;
    fn start(&self, id: Id) -> Result<(), Error>;
    fn stop(&self, id: Id) -> Result<(), Error>;
    fn configure(&self, id: Id, config: Config) -> Result<(), Error>;
    fn update(&self, id: Id, config: Config) -> Result<(), Error>;
    fn inspect(&self, id: Id) -> Result<Snapshot, Error>;
    fn status(&self, id: Id) -> Result<Status, Error>;
}

impl RuntimeService for Host {
    fn create(&self, _: Config) -> Result<Id, Error> { todo!() }
    fn remove(&self, _: Id) -> Result<(), Error> { todo!() }
    fn start(&self, _: Id) -> Result<(), Error> { todo!() }
    fn stop(&self, _: Id) -> Result<(), Error> { todo!() }
    fn configure(&self, _: Id, _: Config) -> Result<(), Error> { todo!() }
    fn update(&self, _: Id, _: Config) -> Result<(), Error> { todo!() }
    fn inspect(&self, _: Id) -> Result<Snapshot, Error> { todo!() }
    fn status(&self, _: Id) -> Result<Status, Error> { todo!() }
}
"#,
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("4 distinct capability clusters"));
        assert!(found[0].message.contains("lifecycle: start, stop"));
        assert!(
            found[0]
                .related
                .iter()
                .any(|item| item.message.contains("implemented by `Host`"))
        );
    }

    #[test]
    fn noun_capability_presenters() {
        let found = findings(
            r#"
pub trait Presenter {
    fn poll_events(&mut self);
    fn take_events(&mut self) -> Vec<Event>;
    fn set_clipboard_text(&mut self, text: &str);
    fn take_clipboard_text(&mut self) -> Option<String>;
    fn reconcile_window(&mut self, state: &WindowState);
    fn destroy_window(&mut self, id: SurfaceId);
    fn begin_interaction(&mut self, id: SurfaceId, interaction: Interaction);
    fn present(&mut self, image: Image) -> Feedback;
}
"#,
        );
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .message
                .contains("clipboard: set_clipboard_text, take_clipboard_text")
        );
        assert!(
            found[0]
                .message
                .contains("events: poll_events, take_events")
        );
        assert!(
            found[0]
                .message
                .contains("window: reconcile_window, destroy_window, begin_interaction")
        );
    }

    #[test]
    fn count_is_insufficient() {
        let found = findings(
            r#"
pub trait RecordRepository {
    fn create(&self, value: Record) -> Result<Id, Error>;
    fn open(&self, id: Id) -> Result<Record, Error>;
    fn read(&self, id: Id) -> Result<Record, Error>;
    fn write(&self, value: Record) -> Result<(), Error>;
    fn save(&self, value: Record) -> Result<(), Error>;
    fn load(&self, id: Id) -> Result<Record, Error>;
    fn delete(&self, id: Id) -> Result<(), Error>;
    fn list(&self) -> Result<Vec<Record>, Error>;
    fn find(&self, query: Query) -> Result<Vec<Record>, Error>;
}
"#,
        );
        assert_eq!(found.len(), 0);
    }

    #[test]
    fn preserves_renderer_contracts() {
        let found = findings(
            r#"
pub trait Codec {
    fn encode_header(&self, value: Header) -> Bytes;
    fn encode_body(&self, value: Body) -> Bytes;
    fn encode_tail(&self, value: Tail) -> Bytes;
    fn decode_header(&self, value: Bytes) -> Header;
    fn decode_body(&self, value: Bytes) -> Body;
    fn decode_tail(&self, value: Bytes) -> Tail;
    fn serialize(&self, value: Frame) -> Bytes;
    fn deserialize(&self, value: Bytes) -> Frame;
}
pub trait Visitor {
    fn visit_a(&mut self, value: A);
    fn visit_b(&mut self, value: B);
    fn visit_c(&mut self, value: C);
    fn visit_d(&mut self, value: D);
    fn visit_e(&mut self, value: E);
    fn visit_f(&mut self, value: F);
    fn visit_g(&mut self, value: G);
    fn visit_h(&mut self, value: H);
}
pub trait Renderer {
    fn render_a(&mut self, value: A);
    fn render_b(&mut self, value: B);
    fn draw_a(&mut self, value: A);
    fn draw_b(&mut self, value: B);
    fn present_a(&mut self, value: A);
    fn present_b(&mut self, value: B);
    fn frame_a(&mut self, value: A);
    fn frame_b(&mut self, value: B);
}
pub trait SurfaceProtocol {
    fn create_surface(&self, id: SurfaceId);
    fn remove_surface(&self, id: SurfaceId);
    fn configure_surface(&self, id: SurfaceId);
    fn update_surface(&self, id: SurfaceId);
    fn present_surface(&self, id: SurfaceId);
    fn commit_surface(&self, id: SurfaceId);
    fn inspect_surface(&self, id: SurfaceId) -> SurfaceSnapshot;
    fn status_surface(&self, id: SurfaceId) -> SurfaceStatus;
}
"#,
        );
        assert_eq!(found.len(), 0);
    }

    #[test]
    fn ignores_test_traits() {
        let found = findings(
            r#"
pub unsafe trait Abi {
    fn create(&self); fn remove(&self);
    fn start(&self); fn stop(&self);
    fn inspect(&self); fn status(&self);
    fn configure(&self); fn update(&self);
}
pub trait Hidden: sealed::Sealed {
    fn create(&self); fn remove(&self);
    fn start(&self); fn stop(&self);
    fn inspect(&self); fn status(&self);
    fn configure(&self); fn update(&self);
}
#[cfg(test)]
pub trait Fixture {
    fn create(&self); fn remove(&self);
    fn start(&self); fn stop(&self);
    fn inspect(&self); fn status(&self);
    fn configure(&self); fn update(&self);
}
"#,
        );
        assert_eq!(found.len(), 0);
    }

    #[test]
    fn ignores_name_guessing() {
        let found = findings(
            r#"
pub trait Odd {
    fn create(&self);
    fn remove(&self);
    fn start(&self);
    fn stop(&self);
    fn alpha(&self);
    fn beta(&self);
    fn gamma(&self);
    fn delta(&self);
    fn epsilon(&self);
    fn zeta(&self);
}
"#,
        );
        assert_eq!(found.len(), 0);
    }
    const RUNTIME: &str = "trait Runtime { fn load_wallet(&self,id:WalletId)->Wallet;fn \
        save_wallet(&self,wallet:Wallet);fn start_sync(&self,scope:Scope);fn stop_sync(&\
        self,scope:Scope);fn inspect_checkpoint(&self,scope:Scope)->Checkpoint;fn query_\
        height(&self,scope:Scope)->Height;fn configure_rpc(&self,endpoint:Endpoint);fn u\
        pdate_rpc(&self,timeout:Timeout); }";

    #[test]
    fn payment_signatures_and_test_only_methods_supply_exact_evidence() {
        let found = findings(RUNTIME);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].related.len(), 8);
        assert!(found[0].message.contains("lifecycle"));
        assert!(
            findings(
                "trait Wallet { fn address(&self);fn history(&self);fn transfer\
            (&self);fn balance(&self); }"
            )
            .is_empty()
        );
        let text = RUNTIME
            .replace("fn start_sync", "#[cfg(test)] fn start_sync")
            .replace("fn stop_sync", "#[cfg(test)] fn stop_sync");
        assert!(findings(&text).is_empty());
        assert_eq!(check(&text, "scope='all'").findings.len(), 1);
    }
    #[test]
    fn unsafe_sealed_and_generated_boundaries_have_real_exemptions() {
        assert!(findings(&RUNTIME.replace("trait Runtime", "unsafe trait Runtime")).is_empty());
        assert!(
            findings(&RUNTIME.replace("trait Runtime", "trait Runtime: sealed::Sealed")).is_empty()
        );
        assert!(findings(&format!("#[automatically_derived]\n{RUNTIME}")).is_empty());
        assert!(findings(&format!("// @generated\n{RUNTIME}")).is_empty());
        assert_eq!(
            findings(&format!("#[doc = \"automatically_derived\"]\n{RUNTIME}")).len(),
            1
        );
    }
    #[test]
    fn targets_and_directives_are_respected() {
        assert!(check(RUNTIME, "exclude='lib.rs'").findings.is_empty());
        let report = check(
            &format!(
                "// linter:disable rust/broad-trait-responsibilities -- External runtime\
                \u{20}protocol requires this combined capability.\n{RUNTIME}"
            ),
            "",
        );
        assert!(report.findings.is_empty());
        assert_eq!(report.suppressed.len(), 1);
        assert!(findings(&format!("#[cfg(test)] {RUNTIME}")).is_empty());
        assert_eq!(
            check(&format!("#[cfg(test)] {RUNTIME}"), "scope='tests'")
                .findings
                .len(),
            1
        );
    }
    #[test]
    fn vocabulary_is_required_and_invalid_thresholds_fail() {
        for policy in [
            "[[rules.\"rust/broad-trait-responsibilities\"]]\ntarget='*'".to_owned(),
            policy().replace("min_methods = 8", "min_methods = 0"),
            policy().replace("min_clusters = 3", "min_clusters = 1"),
            policy().replace("min_clusters = 3", "min_clusters = 99"),
            policy().replace("min_methods_per_cluster = 2", "min_methods_per_cluster = 4"),
            policy().replace("\"stop\"", "\"start\""),
            format!("{}\nunknown=true", policy()),
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::write(root.path().join("linter.toml"), policy).unwrap();
            assert!(matches!(
                linter::Registry::default()
                    .register::<BroadTrait>()
                    .unwrap()
                    .check(root.path()),
                Err(Error::Configuration(_))
            ));
        }
    }
    #[test]
    fn own_rule_implementation_is_not_a_broad_contract() {
        assert!(findings(include_str!("mod.rs")).is_empty());
        assert!(findings(include_str!("config.rs")).is_empty());
        assert!(findings(include_str!("profile.rs")).is_empty());
    }
}
