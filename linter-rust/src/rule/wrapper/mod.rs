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
            for node in descendants(source.syntax.root_node())
                .into_iter()
                .filter(|n| n.kind() == "struct_item")
            {
                let id = index.identity(source, node);
                let platform = index
                    .structures
                    .iter()
                    .find(|s| s.id == id)
                    .is_some_and(|s| s.platform);
                structures.push(Structure {
                    source,
                    node,
                    id,
                    fields: BTreeMap::new(),
                    test: tests.get(node.start_byte()).copied().unwrap_or(false),
                    platform,
                });
            }
            for node in descendants(source.syntax.root_node())
                .into_iter()
                .filter(|n| n.kind() == "impl_item")
            {
                let owner = index.identity(source, node);
                if let Some(ty) = node.child_by_field_name("type")
                    && let Some(id) = index.resolve(source, ty, &owner)
                {
                    implementations.entry(id).or_default().push(Implementation {
                        source,
                        node,
                        tests: tests.clone(),
                    });
                }
            }
        }
        let mut findings = Vec::new();
        for structure in &structures {
            for assertion in &self.0 {
                if assertion.target.matches(&structure.source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|e| e.matches(&structure.source.path))
                    && match assertion.scope {
                        Scope::Production => !structure.test,
                        Scope::Tests => structure.test,
                        Scope::All => true,
                    }
                    && let Some(finding) =
                        candidate(structure, &structures, &index, &implementations, assertion)
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
fn candidate(
    structure: &Structure<'_>,
    structures: &[Structure<'_>],
    index: &Index<'_>,
    implementations: &BTreeMap<String, Vec<Implementation<'_>>>,
    assertion: &Assertion,
) -> Option<Finding> {
    let source = structure.source;
    let node = structure.node;
    if structure.platform
        || boundary(node, source)
        || !syntax::visibility(node, source).is_empty()
        || node.child_by_field_name("type_parameters").is_some()
    {
        return None;
    }
    let body = node.child_by_field_name("body")?;
    let fields: Vec<_> = children(body)
        .into_iter()
        .filter(|n| n.kind() != "attribute_item")
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
    let inner_id = inner.strip_prefix("nominal:")?;
    let matches: Vec<_> = structures
        .iter()
        .filter(|s| s.id.key() == inner_id)
        .collect();
    if matches.len() != 1
        || matches[0].id.package != structure.id.package
        || inner_id == structure.id.key()
    {
        return None;
    }
    let owner = format!("nominal:{}", structure.id.key());
    if structures.iter().filter(|s| s.id == structure.id).count() != 1 {
        return None;
    }
    let own = implementations.get(&owner)?;
    let inner_impls = implementations.get(&inner)?;
    let mut related = Vec::new();
    let mut count = 0;
    for implementation in own {
        if ignored(implementation, implementation.node, structure.test) {
            continue;
        }
        if implementation.node.child_by_field_name("trait").is_some()
            || children(implementation.node)
                .iter()
                .any(|n| matches!(n.kind(), "where_clause" | "type_parameters"))
            || boundary(implementation.node, implementation.source)
        {
            return None;
        }
        for method in children(implementation.node.child_by_field_name("body")?) {
            if ignored(implementation, method, structure.test) || method.kind() == "attribute_item"
            {
                continue;
            }
            if method.kind() != "function_item" || boundary(method, implementation.source) {
                return None;
            }
            if syntax::constructor(method, implementation.source, index, &field, &inner) {
                continue;
            }
            if !syntax::forwards(method, implementation.source, &field) {
                return None;
            }
            let signature = syntax::signature(method, implementation.source, index)?;
            let name = text(method.child_by_field_name("name")?, implementation.source);
            let mut matches = Vec::new();
            for inner_impl in inner_impls {
                if inner_impl.node.child_by_field_name("trait").is_some()
                    || ignored(inner_impl, inner_impl.node, structure.test)
                {
                    continue;
                }
                for candidate in children(inner_impl.node.child_by_field_name("body")?) {
                    if candidate.kind() == "function_item"
                        && !ignored(inner_impl, candidate, structure.test)
                        && candidate
                            .child_by_field_name("name")
                            .is_some_and(|n| text(n, inner_impl.source) == name)
                    {
                        matches.push((inner_impl.source, candidate));
                    }
                }
            }
            if matches.len() != 1
                || syntax::signature(matches[0].1, matches[0].0, index).as_ref() != Some(&signature)
            {
                return None;
            }
            count += 1;
            related.push(evidence(
                implementation.source,
                method,
                format!("Transparent forwarder `{name}`"),
            ));
            related.push(evidence(
                matches[0].0,
                matches[0].1,
                format!("Identical inner method `{name}`"),
            ));
        }
    }
    if count < assertion.min_methods {
        return None;
    }
    Some(Finding {
        rule: RedundantWrapper::ID,
        path: source.path.clone(),
        span: Some(Span::new(&source.text, node.byte_range())),
        related,
        configuration: assertion.setting.clone(),
        message: format!(
            "\
        `{}` only wraps local `{}` and forwards {count} methods with identical names and\
        \u{20}signatures",
            structure.id.name, matches[0].id.name
        ),
        instruction: "\
        Use the inner entity directly unless the wrapper owns an invariant, translation,\
        \u{20}synchronization, instrumentation, adapter, or compatibility contract."
            .into(),
    })
}
fn ignored(implementation: &Implementation<'_>, node: Node<'_>, test: bool) -> bool {
    !test
        && implementation
            .tests
            .get(node.start_byte())
            .copied()
            .unwrap_or(false)
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
        for inner in ["", "#[cfg(test)] fn read(&self,id:u64)->usize{0}"] {
            for outer in ["", "#[cfg(test)] fn fixture(&self)->bool{true}"] {
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
