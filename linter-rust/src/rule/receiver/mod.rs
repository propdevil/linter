use crate::{
    Analysis, Source,
    declaration::Index,
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};
use linter::{Error, Finding, Project, Rule, RuleResult, Span, Status};
use tree_sitter::Node;
use words::words;
mod config;
mod words;
pub use config::Config;
pub struct ReceiverName(Vec<Assertion>);
impl Rule for ReceiverName {
    const ID: &'static str = "rust/receiver-name-repetition";
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
                inspect(
                    source.syntax.root_node(),
                    source,
                    &index,
                    &tests,
                    assertion,
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
    index: &Index<'_>,
    tests: &[bool],
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) {
    if matches!(node.kind(), "function_item" | "function_signature_item") {
        let selected = match assertion.scope {
            Scope::Production => !tests[node.start_byte()],
            Scope::Tests => tests[node.start_byte()],
            Scope::All => true,
        };
        if selected
            && let Some(namespace) = namespace(node, source, index)
            && let Some(finding) = finding(node, source, index, &namespace, assertion)
        {
            findings.push(finding);
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, index, tests, assertion, findings);
    }
}
fn namespace(node: Node<'_>, source: &Source, index: &Index<'_>) -> Option<String> {
    let parameters = node.child_by_field_name("parameters")?;
    let mut cursor = parameters.walk();
    if !parameters.named_children(&mut cursor).any(|parameter| {
        parameter.kind() == "self_parameter"
            || parameter
                .child_by_field_name("pattern")
                .is_some_and(|pattern| source.text[pattern.byte_range()] == *"self")
    }) {
        return None;
    }
    let body = node
        .parent()
        .filter(|parent| parent.kind() == "declaration_list")?;
    let owner = body.parent()?;
    if owner.kind() == "trait_item" {
        return Some(source.text[owner.child_by_field_name("name")?.byte_range()].to_owned());
    }
    if owner.kind() != "impl_item" || owner.child_by_field_name("trait").is_some() {
        return None;
    }
    let mut ty = owner.child_by_field_name("type")?;
    if ty.kind() == "generic_type" {
        ty = ty.child_by_field_name("type")?;
    }
    let context = index.identity(source, owner);
    let resolved = index.resolve(source, ty, &context)?;
    resolved
        .strip_prefix("nominal:")?
        .split('<')
        .next()?
        .rsplit("::")
        .next()
        .map(str::to_owned)
}
fn finding(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    namespace: &str,
    assertion: &Assertion,
) -> Option<Finding> {
    let name = &source.text[node.child_by_field_name("name")?.byte_range()];
    if assertion
        .ignored_names
        .contains(name.trim_start_matches("r#"))
    {
        return None;
    }
    let receiver = words(namespace);
    let method = words(name);
    if receiver.is_empty()
        || method.len() <= receiver.len()
        || receiver
            .iter()
            .all(|word| word.len() < 3 || word.chars().all(|c| c.is_ascii_digit()))
        || conversion(node, source, index, namespace, &method)
    {
        return None;
    }
    let (position, suggestion) = if method.starts_with(&receiver) {
        ("prefix", method[receiver.len()..].join("_"))
    } else if method.ends_with(&receiver) {
        ("suffix", method[..method.len() - receiver.len()].join("_"))
    } else {
        return None;
    };
    Some(Finding {
        rule: ReceiverName::ID,
        path: source.path.clone(),
        span: Some(Span::new(&source.text, node.byte_range())),
        related: Vec::new(),
        configuration: assertion.setting.clone(),
        message: format!(
            "method `{name}` repeats receiver namespace `{namespace}` as a {position}"
        ),
        instruction: format!(
            "Prefer `{namespace}::{suggestion}`; retain repeated words only when they na\
                me a distinct domain concept."
        ),
    })
}
fn conversion(
    node: Node<'_>,
    source: &Source,
    index: &Index<'_>,
    namespace: &str,
    method: &[String],
) -> bool {
    let offset = match method.first().map(String::as_str) {
        Some("as" | "from" | "into" | "to") => 1,
        Some("try")
            if method
                .get(1)
                .is_some_and(|word| matches!(word.as_str(), "from" | "into")) =>
        {
            2
        }
        _ => return false,
    };
    let Some(mut ty) = node.child_by_field_name("return_type") else {
        return false;
    };
    while ty.kind() == "reference_type" {
        let Some(inner) = ty.child_by_field_name("type") else {
            return false;
        };
        ty = inner;
    }
    let mut context = index.identity(
        source,
        node.parent().and_then(|body| body.parent()).unwrap_or(node),
    );
    context.name = namespace.into();
    let Some(ty) = conversion_type(ty, source, index, &context) else {
        return false;
    };
    let destination = if &source.text[ty.byte_range()] == "Self" {
        Some(namespace.to_owned())
    } else {
        index.resolve(source, ty, &context).and_then(|identity| {
            identity
                .strip_prefix("nominal:")
                .and_then(|value| value.split('<').next()?.rsplit("::").next())
                .map(str::to_owned)
        })
    };
    destination.is_some_and(|destination| words(&destination) == method[offset..])
}
fn conversion_type<'a>(
    mut ty: Node<'a>,
    source: &Source,
    index: &Index<'_>,
    context: &crate::declaration::Identity,
) -> Option<Node<'a>> {
    if ty.kind() == "generic_type" {
        let base = ty.child_by_field_name("type")?;
        if matches!(
            index.resolve(source, base, context).as_deref(),
            Some("std:Result" | "std:Option")
        ) {
            let arguments = ty.child_by_field_name("type_arguments")?;
            let mut cursor = arguments.walk();
            let argument = arguments.named_children(&mut cursor).next()?;
            ty = argument;
        } else {
            ty = base;
        }
    }
    Some(ty)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn check(path: &str, source: &str, config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/receiver-name-repetition\"]]\ntarget='**/*.rs'\n{config}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<ReceiverName>()?
            .check(root.path())
            .map(|report| report.findings)
    }
    #[test]
    fn preserves_donor_traits_acronyms_versions_and_contract_exceptions() {
        let source = r#"
struct Directory;
impl Directory {
fn create_directory(&self) {}
fn directory_remove(&self) {}
fn create_file(&self) {}
fn from_directory(_: Directory) -> Self { Self }
fn into_directory(self) -> Directory { self }
fn try_into_directory(self) -> Result<Directory, ()> { Ok(self) }
fn try_again_directory(&self) {}
}
struct HTTPServerV2;
impl HTTPServerV2 {
fn restart_http_server_v2(&self) {}
fn restart_http_server(&self) {}
}
struct Id;
impl Id { fn parse_id(&self) {} }
trait Workspace {
fn remove_workspace(&self);
fn workspace_settings(&self);
}
trait Foreign { fn remove_directory(&self); }
impl Foreign for Directory { fn remove_directory(&self) {} }
"#;
        let found = check("lib.rs", source, "").unwrap();
        assert_eq!(found.len(), 6);
        for name in [
            "create_directory",
            "directory_remove",
            "try_again_directory",
            "restart_http_server_v2",
            "remove_workspace",
            "workspace_settings",
        ] {
            assert!(
                found
                    .iter()
                    .any(|finding| finding.message.contains(&format!("`{name}`"))),
                "{name}"
            );
        }
        assert_eq!(words("HTTPServerV2"), ["http", "server", "v", "2"]);
    }
    #[test]
    fn production_after_test_receivers_is_checked() {
        let source = r#"
#[cfg(test)] mod checks {struct Wallet;impl Wallet{fn wallet_address(&self){}}}
struct Wallet;
#[cfg(test)] impl Wallet {fn wallet_balance(&self){}}
impl Wallet {
#[cfg(test)] fn wallet_history(&self){}
fn wallet_address(&self){}
}
"#;
        let found = check("lib.rs", source, "").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].span.as_ref().unwrap().line, 7);
        assert_eq!(check("lib.rs", source, "scope='all'").unwrap().len(), 4);
        assert_eq!(check("lib.rs", source, "scope='tests'").unwrap().len(), 3);
    }
    #[test]
    fn conversion_prefix_alone_is_insufficient() {
        let source = "struct Wallet;struct Other;impl Wallet{fn to_wallet(&self){} fn as\
            _wallet(&self)->&Other{todo!()} fn into_wallet(self)->Self{self} fn to_walle\
            t_copy(&self){} fn wallet_address(&self){} }";
        let found = check("lib.rs", source, "").unwrap();
        assert_eq!(found.len(), 3);
        assert!(
            found
                .iter()
                .any(|finding| finding.message.contains("`to_wallet`"))
        );
        assert!(
            found
                .iter()
                .any(|finding| finding.message.contains("`as_wallet`"))
        );
        assert!(
            !found
                .iter()
                .any(|finding| finding.message.contains("`into_wallet`"))
        );
    }
    #[test]
    fn nominal_alias_generic_and_typed_receivers_are_resolved() {
        let source = "mod entities{pub struct Wallet<T>(T);}type Alias=entities::Wallet<\
            u8>;impl Alias{fn wallet_address(&self){}}impl<T> entities::Wallet<T>{fn wal\
            let_balance(self:Box<Self>){}fn wallet_metadata(&self){}}";
        assert_eq!(check("lib.rs", source, "").unwrap().len(), 3);
        assert!(
            check("lib.rs", "impl Unknown{fn unknown_value(&self){}}", "")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn ignores_only_configured_exact_method_names_and_respects_target_scope() {
        let source =
            "struct Wallet;impl Wallet{fn wallet_address(&self){}fn wallet_history(&self){}}";
        assert_eq!(
            check("lib.rs", source, "ignored_names=['wallet_address']")
                .unwrap()
                .len(),
            1
        );
        assert!(
            check("lib.rs", source, "exclude='*.rs'")
                .unwrap()
                .is_empty()
        );
        assert!(check("tests/input.rs", source, "").unwrap().is_empty());
        assert_eq!(
            check("tests/input.rs", source, "scope='tests'")
                .unwrap()
                .len(),
            2
        );
        for config in [
            "ignored_names=['bad name']",
            "ignored_names=['a','a']",
            "scope='unknown'",
            "exclude=[]",
            "unknown=true",
        ] {
            assert!(
                matches!(check("lib.rs", "", config), Err(Error::Configuration(_))),
                "{config}"
            );
        }
    }
    #[test]
    fn directives_and_implementation_pass() {
        let source = "struct Wallet;impl Wallet{\n// linter:disable rust/receiver-name-r\
            epetition -- distinguishes owner from peer address\nfn wallet_address(&self)\
            {}}";
        assert!(check("lib.rs", source, "").unwrap().is_empty());
        for text in [
            include_str!("mod.rs"),
            include_str!("config.rs"),
            include_str!("words.rs"),
        ] {
            assert!(check("lib.rs", text, "").unwrap().is_empty());
        }
    }
}
