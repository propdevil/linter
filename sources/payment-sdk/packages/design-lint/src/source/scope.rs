use super::{Package, SourceFile};
use crate::rule::production::test_only;
use std::path::Path;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub(super) fn test_path(path: &Path) -> bool {
    path.components().any(|part| part.as_os_str() == "tests")
        || path
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "test" | "tests") || name.ends_with("_test"))
}

pub(super) fn domain(path: &Path, package: Option<&Package>) -> String {
    let relative = package.and_then(|package| path.strip_prefix(package.root.join("src")).ok());
    let module = relative
        .and_then(|path| path.components().next())
        .map(|part| snake_case(&part.as_os_str().to_string_lossy()))
        .unwrap_or_else(|| "root".to_owned());
    format!(
        "{}/{module}",
        package.map_or("unknown-package", |package| package.name.as_str())
    )
}

pub fn snake_case(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.trim_start_matches("r#").chars() {
        if character.is_ascii_alphanumeric() {
            if (character.is_ascii_uppercase()
                && output
                    .chars()
                    .last()
                    .is_some_and(|character| character.is_ascii_lowercase()))
                || (separator && !output.is_empty())
            {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    output.trim_matches('_').to_owned()
}

/// Propagate proven test-only ownership through Rust module declarations.
pub(super) fn classify(sources: &mut [SourceFile]) {
    let indices = sources
        .iter()
        .enumerate()
        .map(|(index, source)| (source.path.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut edges = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let parent = source.path.parent().unwrap_or(Path::new("."));
        let stem = source
            .path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let directory = if matches!(stem, "lib" | "main" | "mod") {
            parent.to_owned()
        } else {
            parent.join(stem)
        };
        module_edges(
            index,
            &source.syntax.items,
            &directory,
            parent,
            false,
            &indices,
            &mut edges,
        );
    }
    let incoming = edges
        .iter()
        .map(|(_, target, _)| *target)
        .collect::<BTreeSet<_>>();
    let mut reached = BTreeSet::new();
    let mut pending = sources
        .iter()
        .enumerate()
        .filter(|(index, source)| !incoming.contains(index) || source.test)
        .map(|(index, source)| (index, source.test))
        .collect::<Vec<_>>();
    while let Some((index, testing)) = pending.pop() {
        let testing = testing || sources[index].test;
        if !reached.insert((index, testing)) {
            continue;
        }
        pending.extend(
            edges
                .iter()
                .filter(|(from, _, _)| *from == index)
                .map(|(_, target, conditional)| (*target, testing || *conditional)),
        );
    }
    for (index, source) in sources.iter_mut().enumerate() {
        source.test |= reached.contains(&(index, true)) && !reached.contains(&(index, false));
    }
}

fn module_edges(
    source: usize,
    items: &[syn::Item],
    directory: &Path,
    file_directory: &Path,
    testing: bool,
    indices: &BTreeMap<PathBuf, usize>,
    edges: &mut Vec<(usize, usize, bool)>,
) {
    for item in items {
        let syn::Item::Mod(module) = item else {
            continue;
        };
        let testing = testing || test_only(&module.attrs);
        let explicit = module.attrs.iter().find_map(|attribute| {
            if !attribute.path().is_ident("path") {
                return None;
            }
            let syn::Meta::NameValue(value) = &attribute.meta else {
                return None;
            };
            let syn::Expr::Lit(value) = &value.value else {
                return None;
            };
            let syn::Lit::Str(value) = &value.lit else {
                return None;
            };
            Some(value.value())
        });
        if let Some((_, items)) = &module.content {
            let child = directory.join(explicit.unwrap_or_else(|| module.ident.to_string()));
            module_edges(source, items, &child, &child, testing, indices, edges);
        } else {
            let candidates = if let Some(path) = explicit {
                vec![file_directory.join(path)]
            } else {
                vec![
                    directory.join(format!("{}.rs", module.ident)),
                    directory.join(module.ident.to_string()).join("mod.rs"),
                ]
            };
            for path in candidates {
                if let Ok(path) = path.canonicalize()
                    && let Some(target) = indices.get(&path)
                {
                    edges.push((source, *target, testing));
                }
            }
        }
    }
}
