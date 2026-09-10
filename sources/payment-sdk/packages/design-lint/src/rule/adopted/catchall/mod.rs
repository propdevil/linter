use std::collections::BTreeSet;

use syn::{ItemMod, visit::Visit};

use crate::{
    Policy, Result,
    model::{Finding, Location, Review},
    source::{SourceFile, Workspace, snake_case},
};

pub(crate) const ID: &str = "catch-all-module-name";

/// Detects catch-all module names.
pub struct CatchAllModule;

impl crate::Rule for CatchAllModule {
    fn id(&self) -> &'static str {
        ID
    }

    fn severity(&self) -> crate::Severity {
        crate::Severity::Error
    }

    fn check(
        &self,
        workspace: &crate::source::Workspace,
        policy: &crate::Policy,
    ) -> crate::Result<Vec<crate::Finding>> {
        check(workspace, policy)
    }
}

pub(crate) fn check(workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    let mut file_modules = BTreeSet::new();
    let source_paths = workspace
        .sources()
        .iter()
        .map(|source| source.path.clone())
        .collect::<BTreeSet<_>>();
    let mut overrides = BTreeSet::new();
    for source in workspace.sources() {
        OverriddenModules {
            source,
            paths: &mut overrides,
        }
        .visit_file(&source.syntax);
    }

    for source in workspace.sources() {
        if let Some(name) =
            file_module_name(&source.path).filter(|_| !overrides.contains(&source.path))
            && policy.rust.forbidden_modules.contains(&name)
            && file_modules.insert(source.path.clone())
        {
            findings.push(finding(
                ID,
                source,
                name,
                Location {
                    path: source.path.clone(),
                    line: 1,
                    column: 1,
                    source: String::new(),
                },
                "source file or module directory",
            ));
        }

        let mut visitor = ModuleVisitor {
            rule: ID,
            forbidden: &policy.rust.forbidden_modules,
            source,
            source_paths: &source_paths,
            findings: &mut findings,
        };
        visitor.visit_file(&source.syntax);
    }

    Ok(findings)
}

struct ModuleVisitor<'a> {
    rule: &'static str,
    forbidden: &'a [String],
    source: &'a SourceFile,
    source_paths: &'a BTreeSet<std::path::PathBuf>,
    findings: &'a mut Vec<Finding>,
}

impl<'ast> Visit<'ast> for ModuleVisitor<'_> {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        let name = snake_case(&item.ident.to_string());
        let external_source_loaded = item.content.is_none()
            && module_source_paths(self.source, item, &name).any(|path| {
                self.source_paths.contains(&path)
                    && file_module_name(&path).as_deref() == Some(&name)
            });
        if self.forbidden.contains(&name) && !external_source_loaded {
            self.findings.push(finding(
                self.rule,
                self.source,
                name,
                self.source.location(item.ident.span()),
                if item.content.is_some() {
                    "inline module declaration"
                } else {
                    "external module declaration"
                },
            ));
        }
        syn::visit::visit_item_mod(self, item);
    }
}

fn module_source_paths(
    source: &SourceFile,
    item: &ItemMod,
    name: &str,
) -> impl Iterator<Item = std::path::PathBuf> {
    let directory = source
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));
    let explicit = item.attrs.iter().find_map(|attribute| {
        if !attribute.path().is_ident("path") {
            return None;
        }
        let syn::Meta::NameValue(value) = &attribute.meta else {
            return None;
        };
        let syn::Expr::Lit(value) = &value.value else {
            return None;
        };
        let syn::Lit::Str(path) = &value.lit else {
            return None;
        };
        Some(directory.join(path.value()))
    });
    let paths = if let Some(explicit) = explicit {
        vec![explicit]
    } else {
        let target_directory = directory.file_name().and_then(|name| name.to_str());
        let standalone_target = matches!(target_directory, Some("tests" | "examples" | "bin"))
            || source
                .path
                .file_name()
                .is_some_and(|name| name == "build.rs");
        let directory = match source.path.file_stem().and_then(|stem| stem.to_str()) {
            Some("lib" | "main" | "mod") | None => directory.to_owned(),
            Some(_) if standalone_target => directory.to_owned(),
            Some(stem) => directory.join(stem),
        };
        vec![
            directory.join(format!("{name}.rs")),
            directory.join(name).join("mod.rs"),
        ]
    };
    paths
        .into_iter()
        .map(|path| path.canonicalize().unwrap_or(path))
}

fn file_module_name(path: &std::path::Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let name = if stem == "mod" {
        path.parent()?.file_name()?.to_str()?
    } else {
        stem
    };
    // Crate roots and binary targets are targets, not module identifiers.
    (!matches!(name, "lib" | "main")).then(|| snake_case(name))
}

struct OverriddenModules<'a> {
    source: &'a SourceFile,
    paths: &'a mut BTreeSet<std::path::PathBuf>,
}

impl<'ast> Visit<'ast> for OverriddenModules<'_> {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if item.content.is_none()
            && item
                .attrs
                .iter()
                .any(|attribute| attribute.path().is_ident("path"))
        {
            let name = snake_case(&item.ident.to_string());
            self.paths.extend(
                module_source_paths(self.source, item, &name)
                    .filter(|path| file_module_name(path).as_deref() != Some(&name)),
            );
        }
        syn::visit::visit_item_mod(self, item);
    }
}

fn finding(
    rule: &'static str,
    source: &SourceFile,
    name: String,
    location: Location,
    declaration: &str,
) -> Finding {
    let mut finding = Finding::error(rule, name.clone(), location);
    finding.message = format!(
        "{declaration} `{name}` is a catch-all name that describes convenience or reuse, not ownership"
    );
    finding.help = "rename the module for the entity, capability, algorithm, fixture domain, or external mechanism it owns; split unrelated contents before renaming".to_owned();
    let review = Review {
    metadata: vec![
        ("domain".to_owned(), source.domain.clone()),
        ("package".to_owned(), source.package.clone()),
        ("module".to_owned(), name),
        ("declaration".to_owned(), declaration.to_owned()),
        (
            "scope".to_owned(),
            if source.test { "test" } else { "production" }.to_owned(),
        ),
    ],
    questions: vec![
        "Which single entity, capability, algorithm, fixture domain, or external mechanism owns these items?"
            .to_owned(),
        "Does the module mix unrelated responsibilities that must be split before it can receive a precise name?"
            .to_owned(),
    ],
        ..Review::default()
    };
    finding.review = Some(review);
    finding
}

#[cfg(test)]
mod tests;
