use crate::{
    Analysis, Source,
    declaration::Index,
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use syntax::{children, text};
mod config;
mod syntax;
pub use config::Config;
pub struct RedundantNamespace(Vec<Assertion>);
impl Rule for RedundantNamespace {
    const ID: &'static str = "rust/redundant-namespace";
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
            for source in &analysis.sources {
                if assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
                    && let Some(finding) = candidate(source, analysis, &index, &root, assertion)
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
fn selected(
    source: &Source,
    node: tree_sitter::Node<'_>,
    root: &std::path::Path,
    analysis: &Analysis,
    scope: Scope,
) -> bool {
    let mut tests = vec![false; source.text.len()];
    if integration(source, root, analysis) {
        tests.fill(true);
    } else {
        mark_tests(source.syntax.root_node(), &source.text, &mut tests);
    }
    let test = tests.get(node.start_byte()).copied().unwrap_or(false);
    match scope {
        Scope::Production => !test,
        Scope::Tests => test,
        Scope::All => true,
    }
}
fn candidate(
    source: &Source,
    analysis: &Analysis,
    index: &Index<'_>,
    root: &std::path::Path,
    assertion: &Assertion,
) -> Option<Finding> {
    if source.path.file_name()?.to_str()? != "mod.rs" || syntax::boundary(source) {
        return None;
    }
    let items: Vec<_> = children(source.syntax.root_node())
        .into_iter()
        .filter(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
        .collect();
    if items.len() < 2
        || items
            .iter()
            .any(|item| !matches!(item.kind(), "mod_item" | "use_declaration"))
    {
        return None;
    }
    let modules: Vec<_> = items
        .iter()
        .filter(|item| item.kind() == "mod_item")
        .collect();
    if modules.len() != 1 {
        return None;
    }
    let child = *modules[0];
    if child.child_by_field_name("body").is_some()
        || syntax::attrs(child)
        || !selected(source, child, root, analysis, assertion.scope)
    {
        return None;
    }
    let child_name = text(child.child_by_field_name("name")?, source);
    if !items
        .iter()
        .filter(|item| item.kind() == "use_declaration")
        .all(|item| syntax::transparent(*item, source, child_name))
    {
        return None;
    }
    let name = source.path.parent()?.file_name()?.to_str()?;
    let package = index.identity(source, source.syntax.root_node()).package;
    let owning: Vec<_> = analysis
        .sources
        .iter()
        .filter(|other| index.identity(other, other.syntax.root_node()).package == package)
        .collect();
    let declarations: Vec<_> = owning
        .iter()
        .flat_map(|other| {
            syntax::declarations(other.syntax.root_node(), other, &source.path)
                .into_iter()
                .map(|node| (*other, node))
        })
        .collect();
    if declarations.len() != 1 {
        return None;
    }
    let (parent, declaration) = declarations[0];
    if !syntax::visibility(declaration, parent).is_empty()
        || !selected(parent, declaration, root, analysis, assertion.scope)
    {
        return None;
    }
    if owning.iter().any(|other| {
        other.path != source.path && syntax::references(other.syntax.root_node(), other, name)
    }) {
        return None;
    }
    let directory = source.path.parent()?;
    let paths = [
        directory.join(format!("{child_name}.rs")),
        directory.join(child_name).join("mod.rs"),
    ];
    let implementations: Vec<_> = owning
        .into_iter()
        .filter(|other| paths.contains(&other.path))
        .collect();
    if implementations.len() != 1 {
        return None;
    }
    let implementation = implementations[0];
    if syntax::boundary(implementation)
        || !selected(
            implementation,
            implementation.syntax.root_node(),
            root,
            analysis,
            assertion.scope,
        )
    {
        return None;
    }
    Some(Finding{rule:RedundantNamespace::ID,path:source.path.clone(),span:Some(Span::new(&source.text,child.byte_range())),related:vec![Evidence{path:parent.path.clone(),span:Some(Span::new(&parent.text,declaration.byte_range())),message:"Private parent declaration".into()},Evidence{path:implementation.path.clone(),span:Some(Span::new(&implementation.text,implementation.syntax.root_node().byte_range())),message:"Sole child implementation".into()}],configuration:assertion.setting.clone(),message:format!("private module `{name}` contains only child `{child_name}` and transparent re-exports"),instruction:"Flatten the child into the parent module unless a concrete public, platform, generation, FFI, or privacy boundary requires this namespace.".into()})
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn check(files: &[(&str, &str)], config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/redundant-namespace\"]]\ntarget='**/*.rs'\n{config}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<RedundantNamespace>()?
            .check(root.path())
            .map(|report| report.findings)
    }
    const FILES: [(&str, &str); 3] = [
        ("src/lib.rs", "mod shell;"),
        (
            "src/shell/mod.rs",
            "mod process;\npub(crate) use process::{Child,Status};",
        ),
        (
            "src/shell/process.rs",
            "pub struct Child;\npub struct Status;",
        ),
    ];
    #[test]
    fn reports_private_transparent_namespace_with_parent_and_child_evidence() {
        let found = check(&FILES, "").unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("transparent re-exports"));
        assert_eq!(found[0].related.len(), 2);
        assert!(found[0].span.is_some());
        assert!(found[0].related.iter().all(|item| item.span.is_some()));
    }
    #[test]
    fn preserves_public_qualified_imported_and_configured_boundaries() {
        for parent in [
            "pub mod shell;",
            "mod shell;fn consume(_:shell::Child){}",
            "mod shell;fn consume(_:crate::shell::Child){}",
            "mod shell;use crate::shell as api;",
            "mod shell;pub use crate::shell;",
        ] {
            let mut files = FILES;
            files[0].1 = parent;
            assert!(check(&files, "").unwrap().is_empty(), "{parent}");
        }
        for module in [
            "mod process;pub use process::Child;",
            "#[cfg(unix)] mod process;pub(crate) use process::Child;",
            "mod process;use process::Child;",
            "mod process;pub(crate) use process::Child;fn policy(){}",
            "mod process;pub(crate) use external::Child;",
        ] {
            let mut files = FILES;
            files[1].1 = module;
            assert!(check(&files, "").unwrap().is_empty(), "{module}");
        }
    }
    #[test]
    fn preserves_platform_ffi_documented_and_generated_children() {
        for source in [
            "#[repr(C)] pub struct Child;",
            "#[cfg(unix)] pub struct Child;",
            "unsafe extern \"C\"{fn child();}",
            "include!(\"generated.rs\");",
            "/// Public child contract\npub struct Child;",
            "#![allow(dead_code)]pub struct Child;",
        ] {
            let mut files = FILES;
            files[2].1 = source;
            assert!(check(&files, "").unwrap().is_empty(), "{source}");
        }
    }
    #[test]
    fn stateless_structs_markers_and_wrappers_are_not_namespace_candidates() {
        assert!(
            check(
                &[(
                    "lib.rs",
                    "struct Methods;impl Methods{fn build(){}}trait Marker{}struct Wrapper(String);"
                )],
                ""
            )
            .unwrap()
            .is_empty()
        );
        assert!(check(&FILES[..2], "").unwrap().is_empty());
    }
    #[test]
    fn scopes_exclusions_directives_and_invalid_settings() {
        assert!(check(&FILES, "exclude='src/shell/**'").unwrap().is_empty());
        let tests = [
            ("tests/input.rs", "mod shell;"),
            ("tests/input/shell/mod.rs", FILES[1].1),
            ("tests/input/shell/process.rs", FILES[2].1),
        ];
        assert!(check(&tests, "").unwrap().is_empty());
        assert_eq!(check(&tests, "scope='tests'").unwrap().len(), 1);
        let mut files = FILES;
        files[1].1 = "// linter:disable rust/redundant-namespace -- module remains a deliberate compatibility boundary\nmod process;pub(crate) use process::{Child,Status};";
        assert!(check(&files, "").unwrap().is_empty());
        for config in ["scope='unknown'", "exclude=[]", "unknown=true"] {
            assert!(matches!(check(&[], config), Err(Error::Configuration(_))));
        }
    }
    #[test]
    fn ordinary_comments_do_not_hide_extra_logic() {
        let mut files = FILES;
        files[1].1 = "// module groups process values\nmod process;pub(crate) use process::Child;const LIMIT:u8=1;";
        assert!(check(&files, "").unwrap().is_empty());
        for source in [
            include_str!("mod.rs"),
            include_str!("config.rs"),
            include_str!("syntax.rs"),
        ] {
            assert!(check(&[("lib.rs", source)], "").unwrap().is_empty());
        }
    }
}
