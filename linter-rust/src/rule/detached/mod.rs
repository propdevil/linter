use crate::{
    Analysis, Source,
    declaration::{Identity, Index},
    scope::{integration, mark_tests},
};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::{collections::BTreeMap, fs};
use tree_sitter::Node;
mod config;
mod construction;
pub use config::Config;
use config::{Assertion, Scope};

pub struct DetachedConstructor(Vec<Assertion>);
impl Rule for DetachedConstructor {
    const ID: &'static str = "rust/detached-constructor";
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
        let mut owners = BTreeMap::new();
        for source in &analysis.sources {
            declarations(source.syntax.root_node(), source, &index, &mut owners);
        }
        let mut findings = Vec::new();
        for source in &analysis.sources {
            let mut tests = vec![false; source.text.len()];
            if integration(source, &root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            for assertion in self.0.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                Scan {
                    source,
                    index: &index,
                    owners: &owners,
                    tests: &tests,
                    assertion,
                    findings: &mut findings,
                }
                .items(source.syntax.root_node());
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
pub(super) struct Owner<'a> {
    pub source: &'a Source,
    pub node: Node<'a>,
    pub id: Identity,
}
fn declarations<'a>(
    node: Node<'a>,
    source: &'a Source,
    index: &Index<'a>,
    owners: &mut BTreeMap<String, Owner<'a>>,
) {
    if matches!(node.kind(), "struct_item" | "enum_item") {
        let id = index.identity(source, node);
        owners.insert(format!("nominal:{}", id.key()), Owner { source, node, id });
    }
    if matches!(node.kind(), "source_file" | "declaration_list" | "mod_item") {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            declarations(child, source, index, owners);
        }
    }
}
struct Scan<'a, 'b> {
    source: &'a Source,
    index: &'b Index<'a>,
    owners: &'b BTreeMap<String, Owner<'a>>,
    tests: &'b [bool],
    assertion: &'b Assertion,
    findings: &'b mut Vec<Finding>,
}
impl Scan<'_, '_> {
    fn items(&mut self, node: Node<'_>) {
        if node.kind() == "function_item" {
            self.function(node);
            return;
        }
        if matches!(node.kind(), "source_file" | "declaration_list" | "mod_item") {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                self.items(child);
            }
        }
    }
    fn function(&mut self, node: Node<'_>) {
        let test = self.tests[node.start_byte()];
        if !match self.assertion.scope {
            Scope::Production => !test,
            Scope::Tests => test,
            Scope::All => true,
        } {
            return;
        }
        if node.child_by_field_name("type_parameters").is_some()
            || construction::entrypoint(node, self.source)
        {
            return;
        }
        let context = self.index.identity(self.source, node);
        let Some(returned) = node
            .child_by_field_name("return_type")
            .and_then(|output| construction::returned(output, self.source, self.index, &context))
        else {
            return;
        };
        let Some(owner) = self
            .owners
            .get(&returned)
            .filter(|owner| owner.id.package == context.package)
        else {
            return;
        };
        if construction::entity_input(node, self.source, self.index, &context) {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let evidence = construction::Evidence {
            source: self.source,
            function: node,
            index: self.index,
            context: &context,
            owners: self.owners,
        };
        let Some(terminal) = evidence.terminal(body) else {
            return;
        };
        if evidence.constructed(terminal).as_deref() != Some(&returned)
            || evidence.orchestration(body, terminal, &returned)
        {
            return;
        }
        let name = node
            .child_by_field_name("name")
            .map(|name| &self.source.text[name.byte_range()])
            .unwrap_or("<anonymous>");
        self.findings.push(Finding {
            rule: DetachedConstructor::ID,
            path: self.source.path.clone(),
            configuration: self.assertion.setting.clone(),
            span: Some(Span::new(&self.source.text, node.byte_range())),
            related: vec![
                Evidence {
                    path: owner.source.path.clone(),
                    span: Some(Span::new(&owner.source.text, owner.node.byte_range())),
                    message: format!("Resolved local owner '{}'.", owner.id.key()),
                },
                Evidence {
                    path: self.source.path.clone(),
                    span: Some(Span::new(&self.source.text, terminal.byte_range())),
                    message: "Returned expression constructs this owner.".into(),
                },
            ],
            message: format!(
                "free function '{name}' constructs and returns '{}', its natural owner",
                owner.id.name
            ),
            instruction: format!(
                "Move the factory into impl {} and call it as {}::{name}(...).",
                owner.id.name, owner.id.name
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::DetachedConstructor>()?
            .check(root)
    }
    fn report(source: &str, settings: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/detached-constructor\"]]\ntarget = '**/*.rs'\n{settings}"),
        )
        .unwrap();
        check(root.path()).unwrap()
    }
    fn findings(source: &str) -> Vec<linter::Finding> {
        report(source, "").findings
    }
    #[test]
    fn detached_concrete_and_wrapped_constructors_are_reported() {
        let values = findings(
            r"
struct Lease { value: usize }
struct Token(usize);
fn open() -> Result<Lease, Error> { Ok(Lease { value: 1 }) }
fn token() -> Option<Token> { Some(Token(1)) }
struct Error;
",
        );
        assert_eq!(
            values
                .iter()
                .map(|finding| finding.message.split('\'').nth(1).unwrap())
                .collect::<Vec<_>>(),
            ["open", "token"]
        );
        assert!(values[0].instruction.contains("Lease::open"));
    }

    #[test]
    fn associated_constructor_returning_self_is_already_owned() {
        let values = findings(
            r"
struct Leases;
impl Leases {
    fn open() -> Result<Self, Error> { Ok(Self) }
}
struct Error;
",
        );
        assert!(values.is_empty(), "got {values:?}");
    }

    #[test]
    fn uncertain_ownership_is_not_reported() {
        let values = findings(
            r"
struct Local;
struct Other;
fn generic<T>() -> T { todo!() }
fn dynamic() -> Box<dyn Send> { todo!() }
fn opaque() -> impl Send { Local }
fn forwarded() -> Local { other::make() }
fn orchestrated() -> Local { let _other = Other; Local }
fn converted(value: usize) -> Local { Local::from(value) }
mod other { pub(super) fn make() -> super::Local { super::Local } }
impl From<usize> for Local { fn from(_: usize) -> Self { Self } }
",
        );
        // The outer forwarding and multi-type orchestration are intentionally conservative; the
        // nested function that actually constructs `Local` is the only detached constructor.
        assert_eq!(
            values
                .iter()
                .map(|finding| finding.message.split('\'').nth(1).unwrap())
                .collect::<Vec<_>>(),
            ["make"]
        );
    }

    #[test]
    fn same_owner_factory_wrapper_is_reported_but_other_owner_is_not() {
        let values = findings(
            r"
struct Session;
impl Session { fn create() -> Self { Self } }
fn session() -> Session { Session::create() }
struct Plan;
struct Builder;
impl Builder { fn plan() -> Plan { Plan } }
fn plan() -> Plan { Builder::plan() }
",
        );
        assert_eq!(
            values
                .iter()
                .map(|finding| finding.message.split('\'').nth(1).unwrap())
                .collect::<Vec<_>>(),
            ["session"]
        );
    }
    #[test]
    fn resolves_local_aliases_without_collapsing_ambiguous_or_external_types() {
        let source = "mod owner { pub struct Value { pub x: u8 } } use owner::Value as A\
            lias; fn make() -> Alias { Alias { x: 1 } }";
        assert_eq!(findings(source).len(), 1);
        for source in [
            "struct Value; mod other { pub struct Value; } use other::*; fn make() -> Va\
                lue { Value }",
            "struct Value; fn make() -> external::Value { external::Value }",
            "mod one { pub struct Value; } mod two { pub struct Value; } fn make() -> on\
                e::Value { two::Value }",
            "struct Owner; struct Input; fn convert(input: Input) -> Owner { Owner }",
            "struct Owner; fn transform(input: &Owner) -> Owner { Owner }",
        ] {
            assert!(findings(source).is_empty(), "{source}");
        }
    }
    #[test]
    fn framework_entrypoints_generics_and_shadowed_values_are_not_factories() {
        for source in [
            "struct Value; fn main() -> Value { Value }",
            "struct Value; #[endpoint] fn create() -> Value { Value }",
            "struct Value; extern \"C\" fn create() -> Value { Value }",
            "struct Value; fn make<T>() -> Value { Value }",
            "struct Value; fn make() -> Value { let Value = other(); Value }",
            "struct Value(u8); fn make(Value: fn(u8) -> Value) -> Value { Value(1) }",
            "struct Value; fn Ok(value: Value) -> Result<Value, ()> { todo!() } fn make(\
                ) -> Result<Value, ()> { Ok(Value) }",
        ] {
            assert!(findings(source).is_empty(), "{source}");
        }
    }
    #[test]
    fn scope_exclusions_directives_and_evidence_are_consistent() {
        let source = "struct Value; #[cfg(test)] fn test_factory() -> Value { Value } fn\
            \u{20}make() -> Value { Value }";
        assert_eq!(report(source, "").findings.len(), 1);
        assert_eq!(report(source, "scope = 'tests'").findings.len(), 1);
        assert_eq!(report(source, "scope = 'all'").findings.len(), 2);
        assert!(report(source, "exclude = 'lib.rs'").findings.is_empty());
        let source = "struct Value;\n// linter:disable rust/detached-constructor -- Fram\
            ework requires this free function signature.\nfn make() -> Value { Value }";
        let result = report(source, "");
        assert_eq!(result.suppressed.len(), 1);
        assert!(result.findings.is_empty());
        let result = report("struct Value; fn make() -> Value { Value }", "");
        assert_eq!(result.findings[0].related.len(), 2);
    }
    #[test]
    fn unknown_and_invalid_configuration_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "",
            "target = []",
            "target = '*'\nscope = 'invalid'",
            "target = '*'\nexclude = []",
            "target = '*'\nunknown = true",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/detached-constructor\"]]\n{fields}"),
            )
            .unwrap();
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
    }
}
