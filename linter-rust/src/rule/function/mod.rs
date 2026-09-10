use crate::{
    Analysis, Source,
    declaration::{Identity, Index},
    scope::{integration, mark_tests},
};
use config::{Assertion, Mode, Scope};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::{collections::BTreeMap, fs};
use tree_sitter::Node;
mod config;
mod references;
pub use config::Config;

pub struct FreeFunction(Vec<Assertion>);
impl Rule for FreeFunction {
    const ID: &'static str = "rust/free-function";
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
        let mut declarations = Vec::new();
        let mut owners = BTreeMap::new();
        for source in &analysis.sources {
            collect(
                source.syntax.root_node(),
                source,
                &index,
                &mut declarations,
                &mut owners,
            );
        }
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
            for declaration in declarations.iter().filter(|declaration| {
                assertion.target.matches(&declaration.source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&declaration.source.path))
            }) {
                let source_index = analysis
                    .sources
                    .iter()
                    .position(|source| source.path == declaration.source.path)
                    .unwrap_or(0);
                if !included(declaration.node, &masks[source_index], assertion.scope) {
                    continue;
                }
                if let Some(mut finding) = candidate(declaration, &index, &owners, assertion) {
                    finding.related = references::collect(
                        declaration,
                        &declarations,
                        analysis,
                        &index,
                        &masks,
                        assertion.scope,
                    );
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
struct Declaration<'a> {
    source: &'a Source,
    node: Node<'a>,
    id: Identity,
}
fn collect<'a>(
    node: Node<'a>,
    source: &'a Source,
    index: &Index<'_>,
    functions: &mut Vec<Declaration<'a>>,
    owners: &mut BTreeMap<String, Declaration<'a>>,
) {
    let id = index.identity(source, node);
    if node.kind() == "function_item" && !method(node) {
        functions.push(Declaration {
            source,
            node,
            id: id.clone(),
        });
    }
    if matches!(node.kind(), "struct_item" | "enum_item" | "union_item") {
        owners.insert(
            format!("nominal:{}", id.key()),
            Declaration { source, node, id },
        );
    }
    for child in children(node) {
        collect(child, source, index, functions, owners);
    }
}
fn candidate(
    declaration: &Declaration<'_>,
    index: &Index<'_>,
    owners: &BTreeMap<String, Declaration<'_>>,
    assertion: &Assertion,
) -> Option<Finding> {
    let node = declaration.node;
    let source = declaration.source;
    let parameters = node.child_by_field_name("parameters")?;
    let parameters: Vec<_> = children(parameters)
        .into_iter()
        .filter(|node| node.kind() == "parameter")
        .collect();
    if !matches!(parameters.len(), 1 | 2)
        || external(node, source)
        || attributes(node, source).iter().any(|attribute| {
            matches!(
                attribute.0.as_str(),
                "proc_macro" | "proc_macro_attribute" | "proc_macro_derive"
            )
        })
    {
        return None;
    }
    let name = &declaration.id.name;
    let qualified = format!(
        "crate::{}",
        declaration
            .id
            .module
            .iter()
            .chain(std::iter::once(name))
            .cloned()
            .collect::<Vec<_>>()
            .join("::")
    );
    if assertion
        .exceptions
        .iter()
        .any(|exception| exception.function == *name || exception.function == qualified)
    {
        return None;
    }
    let receiver = receiver(declaration, &parameters, index, owners, assertion);
    if matches!(assertion.mode, Mode::Receiver) && receiver.is_none() {
        return None;
    }
    if factory(node, source, index, &declaration.id, owners) {
        return None;
    }
    let message = receiver.map_or_else(
        || {
            format!(
                "free function `{name}` has {} arguments and requires an ownership classification",
                parameters.len()
            )
        },
        |owner| {
            format!(
                "free function `{name}` takes one declared `{}` value that can own this behavior",
                owner.id.name
            )
        },
    );
    Some(Finding {
        rule: FreeFunction::ID,
        path: source.path.clone(),
        span: Some(Span::new(&source.text, node.byte_range())),
        related: Vec::new(),
        configuration: format!(
            "\
        {}.mode",
            assertion.setting
        ),
        message,
        instruction: "Move cohesive behavior onto its existing receiver, or document an e\
            xact algorithm/framework boundary using a reasoned exception. Do not invent \
            a wrapper for one helper."
            .into(),
    })
}
fn receiver<'a>(
    declaration: &Declaration<'_>,
    parameters: &[Node<'_>],
    index: &Index<'_>,
    owners: &'a BTreeMap<String, Declaration<'a>>,
    assertion: &Assertion,
) -> Option<&'a Declaration<'a>> {
    if parameters.len() != 1 {
        return None;
    }
    let mut ty = parameters[0].child_by_field_name("type")?;
    while ty.kind() == "reference_type" {
        ty = ty.child_by_field_name("type")?;
    }
    if !matches!(ty.kind(), "type_identifier" | "scoped_type_identifier") {
        return None;
    }
    if declaration
        .node
        .child_by_field_name("type_parameters")
        .is_some_and(|parameters| {
            children(parameters).iter().any(|parameter| {
                parameter.child_by_field_name("name").is_some_and(|name| {
                    declaration.source.text[name.byte_range()]
                        == declaration.source.text[ty.byte_range()]
                })
            })
        })
    {
        return None;
    }
    let key = index.resolve(declaration.source, ty, &declaration.id)?;
    let owner = owners.get(&key)?;
    if owner.id.package != declaration.id.package {
        return None;
    }
    let boundary = attributes(owner.node, owner.source)
        .iter()
        .filter(|attribute| attribute.0 == "derive")
        .any(|attribute| {
            attribute.1.split(',').map(str::trim).any(|derive| {
                assertion
                    .boundary_derives
                    .iter()
                    .any(|allowed| derive == allowed)
            })
        });
    (!boundary).then_some(owner)
}
fn factory(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    owner: &Identity,
    owners: &BTreeMap<String, Declaration<'_>>,
) -> bool {
    let Some(ty) = node.child_by_field_name("return_type") else {
        return false;
    };
    let Some(key) = index
        .resolve(source, ty, owner)
        .filter(|key| owners.contains_key(key))
    else {
        return false;
    };
    node.child_by_field_name("body")
        .is_some_and(|body| constructed(body, source, index, owner, &key))
}
fn constructed(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    owner: &Identity,
    key: &str,
) -> bool {
    if node.kind() == "function_item" {
        return false;
    }
    if node.kind() == "struct_expression"
        && node
            .child_by_field_name("name")
            .is_some_and(|name| index.resolve(source, name, owner).as_deref() == Some(key))
    {
        return true;
    }
    children(node)
        .into_iter()
        .any(|child| constructed(child, source, index, owner, key))
}
fn method(node: Node<'_>) -> bool {
    node.parent()
        .filter(|parent| parent.kind() == "declaration_list")
        .and_then(|parent| parent.parent())
        .is_some_and(|parent| matches!(parent.kind(), "impl_item" | "trait_item"))
}
fn external(node: Node<'_>, source: &Source) -> bool {
    children(node)
        .iter()
        .filter(|child| child.kind() == "function_modifiers")
        .any(|modifiers| {
            source.text[modifiers.byte_range()]
                .split_whitespace()
                .any(|word| word == "extern")
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
fn attributes(node: Node<'_>, source: &Source) -> Vec<(String, String)> {
    let mut values = Vec::new();
    let mut previous = node.prev_named_sibling();
    while let Some(attribute) = previous {
        if !matches!(
            attribute.kind(),
            "attribute_item" | "line_comment" | "block_comment"
        ) {
            break;
        }
        if attribute.kind() == "attribute_item"
            && let Some(meta) = attribute.named_child(0)
            && let Some(path) = meta.named_child(0)
        {
            let arguments = meta
                .child_by_field_name("arguments")
                .map(|arguments| {
                    source.text[arguments.byte_range()]
                        .trim_start_matches('(')
                        .trim_end_matches(')')
                        .to_owned()
                })
                .unwrap_or_default();
            values.push((source.text[path.byte_range()].into(), arguments));
        }
        previous = attribute.prev_named_sibling();
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(files: &[(&str, &str)], options: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        let policy = include_str!("readme.md")
            .split("```toml\n")
            .nth(1)
            .unwrap()
            .split("```")
            .next()
            .unwrap();
        let policy = if options.contains("mode=") {
            policy.replace("mode = \"receiver\"", "")
        } else {
            policy.into()
        };
        fs::write(
            root.path().join("linter.toml"),
            format!("{policy}\n{options}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<FreeFunction>()
            .unwrap()
            .check(root.path())
            .unwrap()
    }
    fn findings(source: &str) -> Vec<Finding> {
        run(&[("lib.rs", source)], "").findings
    }
    fn findings_beside_sibling(source: &str, sibling: &str) -> Vec<Finding> {
        run(
            &[
                ("a/Cargo.toml", "[package]\nname='a'\nversion='0.1.0'"),
                ("a/src/lib.rs", source),
                (
                    "hl_runtime/Cargo.toml",
                    "[package]\nname='hl_runtime'\nversion='0.1.0'",
                ),
                ("hl_runtime/src/lib.rs", sibling),
            ],
            "",
        )
        .findings
    }
    #[test]
    fn attribute_references_become_related_context() {
        let values = findings(
            r#"
struct Options {
    #[serde(deserialize_with = "crate::flag")]
    first: bool,
    #[serde(deserialize_with = "crate::flag")]
    second: bool,
}
struct Flags;
fn flag(flags: Flags) -> bool {
    matches!(flags, Flags)
}
"#,
        );
        let [finding] = &values[..] else {
            panic!("one candidate, got {}", values.len());
        };
        assert!(finding.message.contains("`flag`"));
        assert_eq!(finding.related.len(), 2);
    }

    #[test]
    fn attribute_words_are_not_related_context() {
        let values = findings(
            r#"
struct Options {
    #[arg(long = "flag", value_name = "flag")]
    #[serde(rename = "flag")]
    first: bool,
}
struct Flags;
fn flag(flags: Flags) -> bool {
    matches!(flags, Flags)
}
"#,
        );
        let [finding] = &values[..] else {
            panic!("one candidate, got {}", values.len());
        };
        assert!(finding.related.is_empty());
    }

    #[test]
    fn a_local_binding_is_not_related_context() {
        let values = findings(
            r"
struct Flags;
fn flag(value: Flags) -> bool {
    matches!(value, Flags)
}
fn read() -> bool {
    flag(Flags)
}
fn shadow() -> u8 {
    let flag = 2;
    flag
}
",
        );
        let [finding] = &values[..] else {
            panic!("one candidate, got {}", values.len());
        };
        assert!(finding.message.contains("`flag`"));
        assert_eq!(finding.related.len(), 1);
    }

    #[test]
    fn a_function_over_only_foreign_types_has_no_receiver_to_become() {
        let values = findings(
            r"
fn excerpt(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}
fn ratio(value: u64, reference: u64) -> f64 {
    value as f64 / reference as f64
}
",
        );
        assert!(values.is_empty(), "got {values:?}");
    }

    #[test]
    fn a_second_argument_relates_two_things_rather_than_naming_a_receiver() {
        let values = findings(
            r"
pub enum Verdict { Pass }
fn render(limit: usize, verdict: &Verdict) -> usize {
    match verdict { Verdict::Pass => limit }
}
",
        );
        assert!(values.is_empty(), "got {values:?}");
    }

    #[test]
    fn a_collected_argument_is_a_transformation_with_no_receiver() {
        let values = findings(
            r"
pub struct Case;
fn plan(cases: Vec<Case>) -> usize {
    cases.len()
}
fn count(cases: &[Case]) -> usize {
    cases.len()
}
fn first(case: Option<Case>) -> bool {
    case.is_some()
}
pub type Result<T> = std::result::Result<T, String>;
fn outcome(case: Result<Case>) -> bool {
    case.is_ok()
}
",
        );
        assert!(values.is_empty(), "got {values:?}");
    }

    #[test]
    fn a_sole_declared_argument_is_the_receiver_the_method_form_takes() {
        let values = findings(
            r"
pub struct Build;
fn validate_build(build: &Build) -> bool {
    let _ = build;
    true
}
",
        );
        let [finding] = &values[..] else {
            panic!("one candidate, got {}", values.len());
        };
        assert!(finding.message.contains("`validate_build`"));
    }

    #[test]
    fn a_foreign_type_sharing_a_declared_name_is_not_this_tree_s_type() {
        let values = findings(
            r"
use std::path::Path;
pub struct Path;
fn portable_name(path: &Path) -> bool {
    path.is_absolute()
}
fn build_id(path: &std::path::Path) -> bool {
    path.is_absolute()
}
",
        );
        assert!(values.is_empty(), "got {values:?}");
    }

    #[test]
    fn a_sibling_crate_s_type_cannot_take_an_inherent_method_from_here() {
        let values = findings_beside_sibling(
            r"
fn prepare_tasks(assembly: &hl_runtime::Assembly) -> bool {
    let _ = assembly;
    true
}
",
            "pub struct Assembly;",
        );
        assert!(values.is_empty(), "got {values:?}");
    }

    #[test]
    fn a_command_line_argument_type_is_a_boundary_value_not_an_entity() {
        let values = findings(
            r"
#[derive(clap::Args)]
pub struct Options {
    pub verbose: bool,
}
pub fn run(options: Options) -> bool {
    options.verbose
}
",
        );
        assert!(values.is_empty(), "got {values:?}");
    }
    #[test]
    fn classification_mode_preserves_payment_scope_without_donor_annotations() {
        let source = "fn zero() {} fn one(value:usize) {} fn two(a:usize,b:usize) {} fn \
            three(a:usize,b:usize,c:usize) {} extern \"C\" fn ffi(value:usize) {} #[hl_d\
            esign::adapter] async fn handler(State(state):State<AppState>) {} async fn u\
            nreviewed_handler(State(state):State<AppState>) {} fn detached(state:AppStat\
            e) {} #[cfg(test)] fn test_only(value:usize) {} #[hl_design::classify(pkg)] \
            fn package(value:usize) {} #[hl_design::classify(domain=\"gpu\")] fn domain(\
            value:usize) {} #[hl_design::classify(domain=\"\")] fn malformed(value:usize\
            ) {}";
        let report = run(&[("lib.rs", source)], "mode='classification'");
        assert_eq!(report.findings.len(), 8);
        for name in [
            "one",
            "two",
            "handler",
            "unreviewed_handler",
            "detached",
            "package",
            "domain",
            "malformed",
        ] {
            assert!(
                report
                    .findings
                    .iter()
                    .any(|finding| finding.message.contains(&format!("`{name}`")))
            );
        }
    }
    #[test]
    fn proc_macros_and_nested_test_items_stay_outside_production() {
        let source = "#[proc_macro] fn derive(input:TokenStream)->TokenStream{input} #[p\
            roc_macro_attribute] fn decorate(attr:TokenStream,item:TokenStream)->TokenSt\
            ream{item} #[test] fn sample(){fn nested(value:usize){}} struct Sample; #[cf\
            g(test)] impl Sample{fn fixture(){fn nested(value:usize){}}} impl Sample{#[c\
            fg(test)] fn helper(){fn nested_method(value:usize){}} fn production(){fn re\
            tained(value:usize){}}}";
        let report = run(&[("lib.rs", source)], "mode='classification'");
        assert_eq!(report.findings.len(), 1);
        assert!(report.findings[0].message.contains("`retained`"));
    }
    #[test]
    fn receiver_resolution_respects_aliases_generics_and_local_owners() {
        let source = "struct Local; type Alias=Local; struct T; fn check(value:&Alias){}\
            \u{20}fn generic<T>(value:T){} fn many(value:Local,extra:u8){} fn wrapped(va\
            lue:Option<Local>){} fn method_like(value:Local){}";
        let found = findings(source);
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .any(|finding| finding.message.contains("`check`"))
        );
        assert!(
            found
                .iter()
                .any(|finding| finding.message.contains("`method_like`"))
        );
    }
    #[test]
    fn factory_rules_own_concrete_construction_findings() {
        let source = "struct Local; struct Output { value:Local } fn create(value:Local)\
            ->Output{Output{value}} fn inspect(value:Local)->bool{true}";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("`inspect`"));
    }
    #[test]
    fn references_distinguish_functions_and_shadowing_by_scope() {
        let files = [
            ("first.rs", "fn parse(value:usize){} fn run(){parse(1);}"),
            ("second.rs", "fn parse(value:usize){} fn run(){parse(2);}"),
        ];
        let found = run(&files, "mode='classification'").findings;
        assert_eq!(found.len(), 2);
        for finding in found {
            assert_eq!(finding.related.len(), 1);
            assert_eq!(finding.related[0].path, finding.path);
        }
        let source = "struct Local; fn check(value:Local){} fn use_it(){let pointer=chec\
            k;pointer(Local);} fn shadow(check:fn(Local)){check(Local);}";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].related.len(), 1);
    }
    #[test]
    fn exact_exceptions_directives_and_configuration_are_validated() {
        let source = "struct Local; fn inspect(value:Local){}";
        assert!(
            run(
                &[("lib.rs", source)],
                "exceptions=[{function='crate::inspect',reason='Public framework boundary.'}]"
            )
            .findings
            .is_empty()
        );
        assert!(
            run(&[("lib.rs", source)], "exclude='lib.rs'")
                .findings
                .is_empty()
        );
        let source = source.replace(
            "fn inspect",
            "// linter:disable rust/free-function -- \
            Framework owns this callback signature.\nfn inspect",
        );
        assert_eq!(run(&[("lib.rs", &source)], "").suppressed.len(), 1);
        for options in [
            "mode='unknown'",
            "scope='unknown'",
            "exceptions=[{function='inspect',reason=''}]",
            "boundary_derives=['']",
            "target=[]",
            "unknown=true",
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/free-function\"]]\ntarget='**/*.rs'\n{options}"),
            )
            .unwrap();
            assert!(matches!(
                linter::Registry::default()
                    .register::<FreeFunction>()
                    .unwrap()
                    .check(root.path()),
                Err(Error::Configuration(_))
            ));
        }
    }
    #[test]
    fn owned_implementation_has_no_detached_single_receiver() {
        assert!(findings(include_str!("mod.rs")).is_empty());
        assert!(findings(include_str!("config.rs")).is_empty());
        assert!(findings(include_str!("references.rs")).is_empty());
    }
}
