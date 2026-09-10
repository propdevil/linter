use crate::{
    Analysis, Source,
    declaration::{Index, Structure},
};
use config::{Assertion, Scope};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::{collections::BTreeSet, fs};
use tree_sitter::Node;
mod config;
pub use config::Config;

pub struct DuplicateEntity(Vec<Assertion>);
impl Rule for DuplicateEntity {
    const ID: &'static str = "rust/duplicate-entity-base";
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
        let conversions = conversions(&index, analysis);
        let mut findings = Vec::new();
        for assertion in &self.0 {
            compare(&index, &conversions, assertion, &mut findings);
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn compare(
    index: &Index<'_>,
    conversions: &BTreeSet<(String, String)>,
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) {
    for (position, first) in index.structures.iter().enumerate() {
        for second in &index.structures[position + 1..] {
            let selected = [first, second].map(|item| selected(item, assertion));
            if !selected.iter().any(|value| *value)
                || first.platform
                || second.platform
                || first.id.package != second.id.package
            {
                continue;
            }
            if !same_scope(first, assertion.scope) || !same_scope(second, assertion.scope) {
                continue;
            }
            if !related(first, second) && !conversions.contains(&pair(first, second)) {
                continue;
            }
            let shared = shared(first, second);
            if shared.len() < assertion.min_shared_fields {
                continue;
            }
            let (subject, peer) = if selected[0] {
                (first, second)
            } else {
                (second, first)
            };
            findings.push(finding(subject, peer, &shared, assertion));
        }
    }
}

fn selected(item: &Structure<'_>, assertion: &Assertion) -> bool {
    assertion.target.matches(&item.source.path)
        && !assertion
            .exclude
            .as_ref()
            .is_some_and(|exclude| exclude.matches(&item.source.path))
}
fn same_scope(item: &Structure<'_>, scope: Scope) -> bool {
    match scope {
        Scope::Production => !item.test,
        Scope::Tests => item.test,
        Scope::All => true,
    }
}
fn shared(first: &Structure<'_>, second: &Structure<'_>) -> Vec<String> {
    first
        .fields
        .iter()
        .filter_map(|(name, field)| {
            let ty = field.ty.as_ref()?;
            (second.fields.get(name)?.ty.as_ref()? == ty).then(|| name.clone())
        })
        .collect()
}
fn related(first: &Structure<'_>, second: &Structure<'_>) -> bool {
    let first_words = words(&first.id.name);
    let second_words = words(&second.id.name);
    first_words.ends_with(&second_words) || second_words.ends_with(&first_words)
}
fn words(name: &str) -> Vec<String> {
    let characters: Vec<_> = name.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    for (index, ch) in characters.iter().copied().enumerate() {
        let boundary = ch.is_uppercase()
            && index > 0
            && (characters[index - 1].is_lowercase()
                || characters
                    .get(index + 1)
                    .is_some_and(|next| next.is_lowercase()));
        if (boundary || ch == '_') && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        if ch != '_' {
            current.extend(ch.to_lowercase());
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}
fn pair(first: &Structure<'_>, second: &Structure<'_>) -> (String, String) {
    ordered(
        format!("nominal:{}", first.id.key()),
        format!("nominal:{}", second.id.key()),
    )
}
fn ordered(first: String, second: String) -> (String, String) {
    if first < second {
        (first, second)
    } else {
        (second, first)
    }
}
fn finding(
    first: &Structure<'_>,
    second: &Structure<'_>,
    shared: &[String],
    assertion: &Assertion,
) -> Finding {
    let fields = shared
        .iter()
        .map(|name| {
            let field = &first.fields[name];
            format!(
                "{name} (line {})",
                Span::new(&first.source.text, field.span.clone()).line
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    Finding {
        rule: DuplicateEntity::ID, path: first.source.path.clone(), span: Some(Span::new(&first.source.text, first.node.byte_range())),
        related: vec![Evidence { path: second.source.path.clone(), span: Some(Span::new(&second.source.text, second.node.byte_range())), message: format!("Related struct `{}`; shared fields: {}", second.id.name, shared.join(", ")) }],
        configuration: format!("{}.min_shared_fields", assertion.setting),
        message: format!("`{}` and `{}` duplicate {} identically typed entity fields: {fields}", first.id.name, second.id.name, shared.len()),
        instruction: "Compose the shared entity into each specialization, preserving its identity and invariants. If these fields have distinct semantics, document that exact boundary with a reasoned directive.".into(),
    }
}
fn conversions(index: &Index<'_>, analysis: &Analysis) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    for source in &analysis.sources {
        collect_conversions(source.syntax.root_node(), source, index, &mut pairs);
    }
    pairs
}
fn collect_conversions(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    pairs: &mut BTreeSet<(String, String)>,
) {
    if node.kind() == "impl_item"
        && let Some(pair) = conversion(node, source, index)
    {
        pairs.insert(pair);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_conversions(child, source, index, pairs);
    }
}
fn conversion(node: Node<'_>, source: &Source, index: &Index<'_>) -> Option<(String, String)> {
    let owner = index.identity(source, node);
    let trait_node = node.child_by_field_name("trait")?;
    if trait_node.kind() != "generic_type" {
        return None;
    }
    let name = index.resolve(source, trait_node.child_by_field_name("type")?, &owner)?;
    if !matches!(name.as_str(), "std:From" | "std:TryFrom") {
        return None;
    }
    let arguments = trait_node.child_by_field_name("type_arguments")?;
    let first = index.resolve(source, arguments.named_child(0)?, &owner)?;
    let second = index.resolve(source, node.child_by_field_name("type")?, &owner)?;
    if !first.starts_with("nominal:") || !second.starts_with("nominal:") {
        return None;
    }
    Some(ordered(first, second))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(files: &[(&str, &str)], options: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/duplicate-entity-base\"]]\ntarget='**/*.rs'\n{options}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<DuplicateEntity>()
            .unwrap()
            .check(root.path())
            .unwrap()
    }
    fn findings(source: &str) -> Vec<Finding> {
        check(&[("lib.rs", source)], "").findings
    }

    #[test]
    fn donor_identity_relation_requires_three_matching_fields() {
        let found = findings(
            "struct Image { id: u64, name: String, path: String } struct DiscoveredImage { id: u64, name: String, path: String, score: u8 } struct Unrelated { id: u64, name: String, path: String } struct WrongTypes { id: String, name: String, path: String }",
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("`Image` and `DiscoveredImage`"));
        assert_eq!(found[0].related.len(), 1);
    }

    #[test]
    fn nominal_wallet_specialization_has_exact_evidence() {
        let found = findings(
            "struct WalletId(u64); struct Address(String); enum Chain { Bitcoin }\nstruct Wallet { id: WalletId, address: Address, chain: Chain }\nstruct ImportedWallet { id: WalletId, address: Address, chain: Chain, birthday: u64 }\nstruct GeneratedWallet { wallet: Wallet, birthday: u64 }",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].span.as_ref().unwrap().line, 2);
        assert_eq!(found[0].related[0].span.as_ref().unwrap().line, 3);
        assert!(found[0].message.contains("address"));
        assert!(found[0].message.contains("chain"));
        assert!(found[0].message.contains("id"));
    }

    #[test]
    fn primitive_wrappers_and_unrelated_models_are_not_duplicates() {
        assert!(findings("struct Email(String); struct WalletId(String); struct Name { value: String } struct ImportedName { value: String } struct Image { id:u64,name:String,status:bool } struct Invoice { id:u64,name:String,status:bool }").is_empty());
        assert!(findings("struct Art { id:u64,name:String,status:bool } struct Cart { id:u64,name:String,status:bool }").is_empty());
    }

    #[test]
    fn same_storage_does_not_erase_distinct_nominal_field_types() {
        assert!(findings("struct Email(String); struct WalletId(String); struct Wallet { id:Email, name:Email, address:Email } struct ImportedWallet { id:WalletId, name:WalletId, address:WalletId }").is_empty());
        assert!(findings("mod a { pub struct Id(String); pub struct Wallet { id:Id,name:Id,address:Id } } mod b { pub struct Id(String); pub struct ImportedWallet { id:Id,name:Id,address:Id } }").is_empty());
    }

    #[test]
    fn aliases_and_imports_preserve_actual_shared_type_identity() {
        let source = "mod ids { pub struct Id(String); } use crate::ids::Id; type Identifier = Id; struct Wallet { id:Identifier,name:String,address:String } mod child { use crate::ids::Id as Key; struct ImportedWallet { id:Key,name:String,address:String } }";
        assert_eq!(findings(source).len(), 1);
        assert!(findings("mod a { struct Wallet { id:Missing,name:Missing,address:Missing } } mod b { struct ImportedWallet { id:Missing,name:Missing,address:Missing } }").is_empty());
    }

    #[test]
    fn proven_conversion_links_different_entity_names() {
        let source = "struct Image { id:u64,name:String,path:String } struct Photo { id:u64,name:String,path:String } impl From<Image> for Photo { fn from(image:Image)->Self { Self { id:image.id,name:image.name,path:image.path } } }";
        assert_eq!(findings(source).len(), 1);
        assert!(findings(&format!("trait From<T> {{}} {source}")).is_empty());
    }

    #[test]
    fn platform_alternatives_and_test_only_models_are_excluded() {
        assert!(findings("#[cfg(unix)] struct Wallet { id:u64,name:String,address:String } #[cfg(windows)] struct ImportedWallet { id:u64,name:String,address:String }").is_empty());
        assert!(findings("struct Wallet { id:u64,name:String,address:String } #[test] fn fixture() { struct ImportedWallet { id:u64,name:String,address:String } }").is_empty());
        let source = "#[cfg(test)] mod tests { struct Wallet { id:u64,name:String,address:String } struct ImportedWallet { id:u64,name:String,address:String } }";
        assert_eq!(
            check(&[("lib.rs", source)], "scope='tests'").findings.len(),
            1
        );
        assert!(findings(source).is_empty());
    }

    #[test]
    fn distinct_packages_have_distinct_entity_ownership() {
        let source = "struct Wallet { id:u64,name:String,address:String }";
        assert!(
            check(
                &[
                    ("a/Cargo.toml", "[package]\nname='a'\nversion='0.1.0'"),
                    ("a/src/lib.rs", source),
                    ("b/Cargo.toml", "[package]\nname='b'\nversion='0.1.0'"),
                    ("b/src/lib.rs", source)
                ],
                ""
            )
            .findings
            .is_empty()
        );
    }

    #[test]
    fn thresholds_selectors_and_suppression_apply_to_the_reported_model() {
        let source = "struct Wallet { id:u64,name:String,address:String } struct ImportedWallet { id:u64,name:String,address:String }";
        assert!(
            check(&[("lib.rs", source)], "min_shared_fields=4")
                .findings
                .is_empty()
        );
        assert!(
            check(&[("lib.rs", source)], "exclude='lib.rs'")
                .findings
                .is_empty()
        );
        let suppressed = format!(
            "// linter:disable rust/duplicate-entity-base -- External contract preserves this identity shape.\n{source}"
        );
        let report = check(&[("lib.rs", &suppressed)], "");
        assert!(report.findings.is_empty());
        assert_eq!(report.suppressed.len(), 1);
    }

    #[test]
    fn configuration_rejects_unsafe_thresholds_and_unknown_fields() {
        for setting in [
            "min_shared_fields=2",
            "min_shared_fields=0",
            "min_shared_fields=-1",
            "minimum=3",
            "scope='maybe'",
            "exclude=[]",
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/duplicate-entity-base\"]]\ntarget='**/*.rs'\n{setting}"),
            )
            .unwrap();
            assert!(
                matches!(
                    linter::Registry::default()
                        .register::<DuplicateEntity>()
                        .unwrap()
                        .check(root.path()),
                    Err(Error::Configuration(_))
                ),
                "{setting}"
            );
        }
    }

    #[test]
    fn own_models_do_not_duplicate_entities() {
        assert!(findings(include_str!("mod.rs")).is_empty());
        assert!(findings(include_str!("config.rs")).is_empty());
    }
}
