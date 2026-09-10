use crate::{
    Analysis, Source,
    declaration::{Index, Structure},
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::collections::BTreeMap;
use syntax::{boundary, children, descendants, text};
use tree_sitter::Node;
mod config;
mod syntax;
pub use config::Config;
pub struct RedundantWrapper(Vec<Assertion>);
struct Implementation<'a> {
    source: &'a Source,
    node: Node<'a>,
    tests: Vec<bool>,
}
impl Rule for RedundantWrapper {
    const ID: &'static str = "rust/redundant-wrapper";
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
            std::fs::canonicalize(project.root()).map_err(|e| Error::Analysis(e.to_string()))?;
        let index = Index::new(analysis, &root);
        let mut structures = Vec::new();
        let mut implementations = BTreeMap::<String, Vec<Implementation<'_>>>::new();
        for source in &analysis.sources {
            let mut tests = vec![false; source.text.len()];
            if integration(source, &root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            structures.extend(Wrapper::structures(source, &index, &tests));
            for (id, implementation) in Implementation::collect(source, &index, &tests) {
                implementations.entry(id).or_default().push(implementation);
            }
        }
        let mut findings = Vec::new();
        for structure in &structures {
            let Some(wrapper) = Wrapper::new(structure, &structures, &index) else {
                continue;
            };
            findings.extend(
                self.0
                    .iter()
                    .filter(|assertion| wrapper.selected(assertion))
                    .filter_map(|assertion| wrapper.finding(&implementations, &index, assertion)),
            );
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
struct Wrapper<'a, 'b> {
    structure: &'b Structure<'a>,
    field: String,
    inner: String,
    inner_name: String,
}
impl<'a, 'b> Wrapper<'a, 'b> {
    fn structures(source: &'a Source, index: &Index<'_>, tests: &[bool]) -> Vec<Structure<'a>> {
        descendants(source.syntax.root_node())
            .into_iter()
            .filter(|node| node.kind() == "struct_item")
            .map(|node| {
                let id = index.identity(source, node);
                let platform = index
                    .structures
                    .iter()
                    .find(|s| s.id == id)
                    .is_some_and(|s| s.platform);
                Structure {
                    source,
                    node,
                    id,
                    fields: BTreeMap::new(),
                    test: tests.get(node.start_byte()).copied().unwrap_or(false),
                    platform,
                }
            })
            .collect()
    }
    fn new(
        structure: &'b Structure<'a>,
        structures: &[Structure<'a>],
        index: &Index<'_>,
    ) -> Option<Self> {
        let source = structure.source;
        let node = structure.node;
        if structure.platform
            || boundary(node, source)
            || !syntax::visibility(node, source).is_empty()
            || node.child_by_field_name("type_parameters").is_some()
        {
            return None;
        }
        let fields: Vec<_> = children(node.child_by_field_name("body")?)
            .into_iter()
            .filter(|node| node.kind() != "attribute_item")
            .collect();
        if fields.len() != 1 || boundary(fields[0], source) {
            return None;
        }
        let (field, ty) = if fields[0].kind() == "field_declaration" {
            (
                text(fields[0].child_by_field_name("name")?, source).to_owned(),
                fields[0].child_by_field_name("type")?,
            )
        } else {
            ("0".into(), fields[0])
        };
        let inner = index.resolve(source, ty, &structure.id)?;
        let id = inner.strip_prefix("nominal:")?;
        let matches: Vec<_> = structures.iter().filter(|s| s.id.key() == id).collect();
        if matches.len() != 1
            || matches[0].id.package != structure.id.package
            || id == structure.id.key()
        {
            return None;
        }
        if structures.iter().filter(|s| s.id == structure.id).count() != 1 {
            return None;
        }
        Some(Self {
            structure,
            field,
            inner,
            inner_name: matches[0].id.name.clone(),
        })
    }
    fn selected(&self, assertion: &Assertion) -> bool {
        assertion.target.matches(&self.structure.source.path)
            && !assertion
                .exclude
                .as_ref()
                .is_some_and(|e| e.matches(&self.structure.source.path))
            && match assertion.scope {
                Scope::Production => !self.structure.test,
                Scope::Tests => self.structure.test,
                Scope::All => true,
            }
    }
    fn finding(
        &self,
        implementations: &BTreeMap<String, Vec<Implementation<'a>>>,
        index: &Index<'_>,
        assertion: &Assertion,
    ) -> Option<Finding> {
        let owner = format!("nominal:{}", self.structure.id.key());
        let own = implementations.get(&owner)?;
        let inner = implementations.get(&self.inner)?;
        let related = self.evidence(own, inner, index)?;
        let count = related.len() / 2;
        if count < assertion.min_methods {
            return None;
        }
        let source = self.structure.source;
        Some(Finding {
            rule: RedundantWrapper::ID,
            path: source.path.clone(),
            span: Some(Span::new(&source.text, self.structure.node.byte_range())),
            related,
            configuration: assertion.setting.clone(),
            message: format!(
                "`{}` only wraps local `{}` and forwards {count} methods with identical names \
                and signatures",
                self.structure.id.name, self.inner_name
            ),
            instruction: "Use the inner entity directly unless the wrapper owns an invariant, \
                translation, synchronization, instrumentation, adapter, or compatibility contract."
                .into(),
        })
    }
    fn evidence(
        &self,
        own: &[Implementation<'a>],
        inner: &[Implementation<'a>],
        index: &Index<'_>,
    ) -> Option<Vec<Evidence>> {
        let mut related = Vec::new();
        for implementation in own
            .iter()
            .filter(|item| !item.ignored(item.node, self.structure.test))
        {
            for method in implementation.forwarders(self, index)? {
                let name = text(method.child_by_field_name("name")?, implementation.source);
                let signature = syntax::signature(method, implementation.source, index)?;
                let (source, peer) = self.matching(name, &signature, inner, index)?;
                related.push(evidence(
                    implementation.source,
                    method,
                    format!("Transparent forwarder `{name}`"),
                ));
                related.push(evidence(
                    source,
                    peer,
                    format!("Identical inner method `{name}`"),
                ));
            }
        }
        Some(related)
    }
    fn matching(
        &self,
        name: &str,
        signature: &str,
        inner: &[Implementation<'a>],
        index: &Index<'_>,
    ) -> Option<(&'a Source, Node<'a>)> {
        let mut matching = Vec::new();
        for implementation in inner.iter().filter(|item| {
            item.node.child_by_field_name("trait").is_none()
                && !item.ignored(item.node, self.structure.test)
        }) {
            let body = implementation.node.child_by_field_name("body")?;
            let candidates = children(body)
                .into_iter()
                .filter(|node| {
                    node.kind() == "function_item"
                        && !implementation.ignored(*node, self.structure.test)
                })
                .filter(|node| {
                    node.child_by_field_name("name")
                        .is_some_and(|n| text(n, implementation.source) == name)
                });
            matching.extend(candidates.map(|node| (implementation.source, node)));
        }
        let [pair] = matching.as_slice() else {
            return None;
        };
        (syntax::signature(pair.1, pair.0, index).as_deref() == Some(signature)).then_some(*pair)
    }
}
impl<'a> Implementation<'a> {
    fn collect(source: &'a Source, index: &Index<'_>, tests: &[bool]) -> Vec<(String, Self)> {
        descendants(source.syntax.root_node())
            .into_iter()
            .filter(|node| node.kind() == "impl_item")
            .filter_map(|node| {
                let owner = index.identity(source, node);
                let id = index.resolve(source, node.child_by_field_name("type")?, &owner)?;
                Some((
                    id,
                    Self {
                        source,
                        node,
                        tests: tests.to_vec(),
                    },
                ))
            })
            .collect()
    }
    fn ignored(&self, node: Node<'_>, test: bool) -> bool {
        !test && self.tests.get(node.start_byte()).copied().unwrap_or(false)
    }
    fn forwarders(&self, wrapper: &Wrapper<'_, '_>, index: &Index<'_>) -> Option<Vec<Node<'a>>> {
        if self.node.child_by_field_name("trait").is_some()
            || children(self.node)
                .iter()
                .any(|n| matches!(n.kind(), "where_clause" | "type_parameters"))
            || boundary(self.node, self.source)
        {
            return None;
        }
        let mut forwarders = Vec::new();
        for method in children(self.node.child_by_field_name("body")?) {
            if self.ignored(method, wrapper.structure.test) || method.kind() == "attribute_item" {
                continue;
            }
            if method.kind() != "function_item" || boundary(method, self.source) {
                return None;
            }
            if syntax::constructor(method, self.source, index, &wrapper.field, &wrapper.inner) {
                continue;
            }
            if !syntax::forwards(method, self.source, &wrapper.field) {
                return None;
            }
            forwarders.push(method);
        }
        Some(forwarders)
    }
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
    fn check(source: &str, config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("lib.rs"), source).unwrap();
        std::fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/redundant-wrapper\"]]\ntarget='**/*.rs'\n{config}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<RedundantWrapper>()?
            .check(root.path())
            .map(|r| r.findings)
    }
    const SOURCE: &str = r#"
 struct Store;
 impl Store {
 fn read(&self,id:u64)->usize {id as usize}
 fn write(&mut self,id:u64)->bool {id>0}
 fn remove(&mut self,id:u64)->bool {id>0}
 }
 struct Storage {inner:Store}
 impl Storage {
 fn new(inner:Store)->Self {Self{inner}}
 fn read(&self,id:u64)->usize {self.inner.read(id)}
 fn write(&mut self,id:u64)->bool {self.inner.write(id)}
 fn remove(&mut self,id:u64)->bool {self.inner.remove(id)}
 }
 "#;
    #[test]
    fn finds_exact_local_forwarding_wrapper() {
        let found = check(SOURCE, "").unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("identical names and signatures"));
        assert_eq!(found[0].related.len(), 6);
        assert!(found[0].related.iter().all(|e| e.span.is_some()));
    }
    #[test]
    fn retains_public_trait_validation_and_wire_boundaries() {
        for prefix in [
            "pub ",
            "pub(crate) ",
            "#[derive(serde::Serialize)] ",
            "#[repr(transparent)] ",
        ] {
            assert!(
                check(
                    &SOURCE.replace("struct Storage", &format!("{prefix}struct Storage")),
                    ""
                )
                .unwrap()
                .is_empty()
            );
        }
        for suffix in [
            "trait Port {} impl Port for Storage {}",
            "impl Storage {fn validate(&self)->bool{true}}",
            "impl Storage {const LIMIT:u8=2;}",
        ] {
            assert!(check(&format!("{SOURCE}{suffix}"), "").unwrap().is_empty());
        }
        assert!(
            check(
                &SOURCE.replace("Self{inner}", "{assert!(valid(&inner));Self{inner}}"),
                ""
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            check(
                &SOURCE.replace("self.inner.read(id)", "self.inner.read(id+1)"),
                ""
            )
            .unwrap()
            .is_empty()
        );
    }
    #[test]
    fn preserves_nominal_signatures_ownership_and_thin_primitive_wrappers() {
        let input = SOURCE
            .replace(
                "struct Store;",
                "struct Store; struct WalletId(u64); struct TransferId(u64);",
            )
            .replace(
                "fn read(&self,id:u64)->usize {self.inner.read(id)}",
                "fn read(&self,id:WalletId)->usize {self.inner.read(id)}",
            );
        assert!(check(&input, "").unwrap().is_empty());
        assert!(
            check(
                &SOURCE.replace(
                    "fn read(&self,id:u64)->usize {self.inner.read(id)}",
                    "fn read(self,id:u64)->usize {self.inner.read(id)}"
                ),
                ""
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            check(
                "struct WalletId(String);struct TransferId(String);impl WalletId{f\
            n as_str(&self)->&str{self.0.as_str()}}",
                ""
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            check(&SOURCE.replace("inner:Store", "inner:String"), "")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn resolves_aliases_and_ignores_unrelated_same_named_types() {
        let input = SOURCE.replace(
            "struct Storage {inner:Store}",
            "type Alias=Store;struct Storage {inner:Alias}",
        );
        assert_eq!(check(&input, "").unwrap().len(), 1);
        assert_eq!(
            check(
                &format!(
                    "{SOURCE} mod another {{struct Store; impl Store{{fn r\
            ead(&self,id:u64)->usize{{0}}}}}}"
                ),
                ""
            )
            .unwrap()
            .len(),
            1
        );
    }
    #[test]
    fn test_helpers_do_not_change_production_evidence() {
        let inner = "#[cfg(test)] fn read(&self,id:u64)->usize{0}";
        let outer = "#[cfg(test)] fn fixture(&self)->bool{true}";
        for (inner, outer) in [("", ""), (inner, ""), ("", outer), (inner, outer)] {
            let input = SOURCE
                .replace(
                    "fn read(&self,id:u64)->usize {id as usize}",
                    &format!(
                        "#[cfg(not(test))] fn read(&self,id:u64)->usize {{id as usize}} {inner}"
                    ),
                )
                .replace(
                    "fn new(inner:Store)",
                    &format!("{outer} fn new(inner:Store)"),
                );
            assert_eq!(check(&input, "").unwrap().len(), 1, "{input}");
        }
    }
    #[test]
    fn tuple_forwarders_and_debug_derives_preserve_donor_support() {
        let tuple = SOURCE
            .replace("struct Storage {inner:Store}", "struct Storage(Store);")
            .replace("fn new(inner:Store)->Self {Self{inner}}", "")
            .replace("self.inner", "self.0");
        assert_eq!(check(&tuple, "").unwrap().len(), 1);
        assert_eq!(
            check(
                &SOURCE.replace("struct Storage", "#[derive(Debug,Clone)] struct Storage"),
                ""
            )
            .unwrap()
            .len(),
            1
        );
        assert_eq!(
            check(&SOURCE.replace("Self{inner}", "Self{inner,}"), "")
                .unwrap()
                .len(),
            1
        );
        assert!(
            check(&SOURCE.replace("fn new", "async fn new"), "")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn thresholds_scope_directives_and_invalid_configuration() {
        assert!(check(SOURCE, "min_methods=4").unwrap().is_empty());
        assert!(check(SOURCE, "exclude='lib.rs'").unwrap().is_empty());
        let tests = format!("#[cfg(test)] mod cases {{{SOURCE}}}");
        assert!(check(&tests, "").unwrap().is_empty());
        assert_eq!(check(&tests, "scope='tests'").unwrap().len(), 1);
        assert!(
            check(
                &SOURCE.replace(
                    "struct Storage",
                    "// linter:disable rust/redundant\
            -wrapper -- preserves a deliberate compatibility contract\nstruct Storage"
                ),
                ""
            )
            .unwrap()
            .is_empty()
        );
        for config in [
            "min_methods=2",
            "min_methods=0",
            "scope='bad'",
            "extra=true",
        ] {
            assert!(matches!(check("", config), Err(Error::Configuration(_))));
        }
        for input in [
            include_str!("mod.rs"),
            include_str!("syntax.rs"),
            include_str!("config.rs"),
        ] {
            assert!(check(input, "").unwrap().is_empty());
        }
    }
}
