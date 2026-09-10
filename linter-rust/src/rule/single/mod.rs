use crate::{
    Analysis, Source,
    declaration::{Identity, Index},
};
use config::{Assertion, Scope};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::{collections::BTreeMap, fs};
use tree_sitter::Node;
mod config;
mod references;
pub use config::Config;

pub struct SingleUse(Vec<Assertion>);
impl Rule for SingleUse {
    const ID: &'static str = "rust/single-use-free-function";
    type Analysis = Analysis;
    type Config = Config;
    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self(config.compile()?))
    }
    fn configured(&self) -> bool {
        !self.0.is_empty()
    }
    fn check(&self, project: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let root = fs::canonicalize(project.root()).map_err(|e| Error::Analysis(e.to_string()))?;
        let index = Index::new(analysis, &root);
        let mut functions = Vec::new();
        let mut owners = BTreeMap::new();
        for source in &analysis.sources {
            collect(
                source.syntax.root_node(),
                source,
                &index,
                &mut functions,
                &mut owners,
            );
        }
        let masks: Vec<_> = analysis
            .sources
            .iter()
            .map(|source| source.test_mask(&root, analysis))
            .collect();
        let references = references::References::new(&functions, analysis, &index, &masks);
        let mut findings = Vec::new();
        for assertion in &self.0 {
            findings.extend(assertion.check(
                &functions,
                analysis,
                &masks,
                &index,
                &owners,
                &references,
            ));
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
impl Assertion {
    fn check(
        &self,
        functions: &[Declaration<'_>],
        analysis: &Analysis,
        masks: &[Vec<bool>],
        index: &Index<'_>,
        owners: &BTreeMap<String, Declaration<'_>>,
        references: &references::References,
    ) -> Vec<Finding> {
        let mut findings = Vec::new();
        for (position, declaration) in functions.iter().enumerate() {
            let mask = &masks[analysis
                .sources
                .iter()
                .position(|source| source.path == declaration.source.path)
                .unwrap_or(0)];
            if !declaration.eligible(self, mask, index, owners) {
                continue;
            }
            let (related, uncertain) = references.get(position, &declaration.id.name, self.scope);
            if uncertain || related.len() != 1 {
                continue;
            }
            findings.push(declaration.finding(self, related));
        }
        findings
    }
}
struct Declaration<'a> {
    source: &'a Source,
    node: Node<'a>,
    id: Identity,
}
impl Declaration<'_> {
    fn eligible(
        &self,
        assertion: &Assertion,
        mask: &[bool],
        index: &Index<'_>,
        owners: &BTreeMap<String, Declaration<'_>>,
    ) -> bool {
        assertion.target.matches(&self.source.path)
            && !assertion
                .exclude
                .as_ref()
                .is_some_and(|s| s.matches(&self.source.path))
            && included(self.node, mask, assertion.scope)
            && self.id.name != "main"
            && !external(self.node, self.source)
            && !children(self.node)
                .iter()
                .any(|n| n.kind() == "visibility_modifier")
            && !attributes(self.node, self.source)
                .iter()
                .any(|a| a.0 != "cfg" && a.0 != "allow" && a.0 != "doc")
            && !factory(self.node, self.source, index, &self.id, owners)
    }
    fn finding(&self, assertion: &Assertion, related: Vec<Evidence>) -> Finding {
        Finding {
            rule: SingleUse::ID,
            path: self.source.path.clone(),
            span: Some(Span::new(&self.source.text, self.node.byte_range())),
            related,
            configuration: assertion.setting.clone(),
            message: format!(
                "private free function `{}` has exactly one resolved use",
                self.id.name
            ),
            instruction: "Inline the function at its sole use, or document its deliberate \
                semantic boundary with a reasoned directive."
                .into(),
        }
    }
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
    std::iter::successors(node.prev_named_sibling(), |node| node.prev_named_sibling())
        .take_while(|node| {
            matches!(
                node.kind(),
                "attribute_item" | "line_comment" | "block_comment"
            )
        })
        .filter(|node| node.kind() == "attribute_item")
        .filter_map(|node| attribute(node, source))
        .collect()
}
fn attribute(node: Node<'_>, source: &Source) -> Option<(String, String)> {
    let meta = node.named_child(0)?;
    let path = meta.named_child(0)?;
    let arguments = meta
        .child_by_field_name("arguments")
        .map(|arguments| {
            source.text[arguments.byte_range()]
                .trim_start_matches('(')
                .trim_end_matches(')')
                .to_owned()
        })
        .unwrap_or_default();
    Some((source.text[path.byte_range()].into(), arguments))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(files: &[(&str, &str)], options: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/single-use-free-function\"]]\ntarget='**/*.rs'\n{options}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<SingleUse>()
            .unwrap()
            .check(root.path())
            .unwrap()
    }
    #[test]
    fn counts_private_functions_and_values() {
        let report = run(
            &[(
                "lib.rs",
                "fn main(){once();visual();public();ffi();twice();twice();register(callb\
                ack);}\nfn once(){}\n// hl-lint: visual-section\nfn visual(){}\npub fn p\
                ublic(){}\nextern \"C\" fn ffi(){}\nfn twice(){}\nfn callback(){}",
            )],
            "",
        );
        assert_eq!(report.findings.len(), 3);
        assert!(
            report
                .findings
                .iter()
                .all(|f| f.related.len() == 1 && f.span.is_some())
        );
    }
    #[test]
    fn recursion_is_not_external_use() {
        assert!(
            run(&[("lib.rs", "fn recursive(){recursive();}")], "")
                .findings
                .is_empty()
        );
        assert_eq!(
            run(
                &[(
                    "lib.rs",
                    "fn recursive(){recursive();} fn run(){recursive();}"
                )],
                ""
            )
            .findings
            .len(),
            1
        );
    }
    #[test]
    fn modules_are_resolved_separately() {
        assert_eq!(
            run(
                &[
                    ("src/first.rs", "fn parse(){} fn run(){parse();}"),
                    ("src/second.rs", "fn parse(){} fn run(){parse();}")
                ],
                ""
            )
            .findings
            .len(),
            2
        );
        assert!(
            run(
                &[("lib.rs", "mod a {fn parse(){}} mod b {fn run(){parse();}}")],
                ""
            )
            .findings
            .is_empty()
        );
    }
    #[test]
    fn ambiguity_prevents_claims() {
        for extra in [
            "fn other(){opaque!(helper);}",
            "use crate::helper as alias; fn other(){alias();}",
            "use crate::other::*;",
            "fn other(){let f=|helper| helper();}",
            "fn other(){for helper in values {helper();}}",
            "fn other(){helper::<u8>();}",
        ] {
            assert!(
                run(
                    &[(
                        "lib.rs",
                        &format!("fn helper(){{}} fn run(){{helper();}} {extra}")
                    )],
                    ""
                )
                .findings
                .is_empty(),
                "{extra}"
            );
        }
    }
    #[test]
    fn shadowed_values_are_not_references() {
        assert!(
            run(
                &[(
                    "lib.rs",
                    "fn helper(){} fn run(helper:fn()){helper();} fn second(\
            ){let helper=other;helper();}"
                )],
                ""
            )
            .findings
            .is_empty()
        );
    }
    #[test]
    fn tests_visibility_and_constructors() {
        assert!(
            run(
                &[
                    (
                        "lib.rs",
                        "fn helper(){} #[test] fn case(){helper();} pub(crate) f\
            n visible(){} fn run(){visible();} struct Value{x:u8} fn build()->Value{Valu\
            e{x:0}} fn caller(){build();}"
                    ),
                    (
                        "\
            tests/check.rs",
                        "\
            fn case(){helper();}"
                    )
                ],
                ""
            )
            .findings
            .is_empty()
        );
        assert_eq!(
            run(
                &[("lib.rs", "#[test] fn case(){helper();} fn helper(){}")],
                "scope='all'"
            )
            .findings
            .len(),
            1
        );
    }
    #[test]
    fn directives_and_excludes() {
        let report = run(
            &[(
                "lib.rs",
                "// linter:disable rust/single-use-free-function -- deliberate startup s\
                ection\nfn section(){} fn main(){section();}",
            )],
            "",
        );
        assert!(report.findings.is_empty());
        assert_eq!(report.suppressed.len(), 1);
        assert!(
            run(
                &[("lib.rs", "fn helper(){} fn main(){helper();}")],
                "exclude='lib.rs'"
            )
            .findings
            .is_empty()
        );
    }
    #[test]
    fn config_rejects_unknowns() {
        assert!(toml::from_str::<Config>("unknown=true").is_err());
        assert!(SingleUse::new(Config::default()).unwrap().0.is_empty());
    }
    #[test]
    fn invalid_assertions_fail_registry() {
        for options in ["mystery=true", "scope='invalid'", "exclude='['"] {
            let root = tempfile::tempdir().unwrap();
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/single-use-free-function\"]]\ntarget='**/*.rs'\n{options}"),
            )
            .unwrap();
            assert!(
                linter::Registry::default()
                    .register::<SingleUse>()
                    .unwrap()
                    .check(root.path())
                    .is_err()
            );
        }
    }
    #[test]
    fn qualified_and_unresolved_references() {
        assert_eq!(
            run(
                &[("lib.rs", "mod a {fn helper(){} fn run(){self::helper();}}")],
                ""
            )
            .findings
            .len(),
            1
        );
        assert!(
            run(
                &[(
                    "lib.rs",
                    "fn helper(){} fn run(){helper(); unknown::helper();}"
                )],
                ""
            )
            .findings
            .is_empty()
        );
        assert!(
            run(
                &[(
                    "lib.rs",
                    "fn helper(){} fn run(){helper(); let x = helper as usize;}"
                )],
                ""
            )
            .findings
            .is_empty()
        );
    }
    #[test]
    fn indexed_references_separate_scopes_and_names() {
        let text = "mod a {fn helper(){} fn run(){helper();} #[test] fn case(){helper();\
            }} mod b {fn helper(){} fn run(){helper();helper();}}";
        assert_eq!(run(&[("lib.rs", text)], "").findings.len(), 1);
        assert!(run(&[("lib.rs", text)], "scope='all'").findings.is_empty());
        let text = "fn helper(){} fn run(){helper();} #[test] fn case(){opaque!(helper);}";
        assert_eq!(run(&[("lib.rs", text)], "").findings.len(), 1);
        assert!(run(&[("lib.rs", text)], "scope='all'").findings.is_empty());
    }
    #[test]
    fn indexes_large_production_fixture() {
        let mut files = Vec::new();
        for module in 0..10 {
            let mut text = String::new();
            for function in 0..100 {
                text.push_str(&format!(
                    "fn helper_{function}() {{\n{}\n}}\n",
                    "    let _ = 1;\n".repeat(42)
                ));
            }
            text.push_str("pub fn run() {\n");
            for function in 0..100 {
                text.push_str(&format!("helper_{function}();\n"));
            }
            text.push_str("}\n");
            files.push((format!("src/module_{module}.rs"), text));
        }
        let borrowed: Vec<_> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let started = std::time::Instant::now();
        assert_eq!(run(&borrowed, "").findings.len(), 1000);
        eprintln!(
            "single-use: 1000 candidates, 46020 lines: {:?}",
            started.elapsed()
        );
    }
    #[test]
    fn own_sources_run() {
        run(
            &[
                ("mod.rs", include_str!("mod.rs")),
                ("config.rs", include_str!("config.rs")),
                ("references.rs", include_str!("references.rs")),
            ],
            "",
        );
    }
}
