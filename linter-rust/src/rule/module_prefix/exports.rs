use super::context::Context;
use crate::{Analysis, Source, declaration::Index};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
use tree_sitter::Node;

type Key = (String, Vec<String>);
type Origin = (PathBuf, usize);
#[derive(Clone, PartialEq, Eq)]
struct Binding {
    origins: BTreeSet<Origin>,
    public: bool,
}
struct Import {
    owner: Key,
    path: Vec<String>,
    alias: Option<String>,
    public: bool,
}

pub(super) struct Exports {
    paths: BTreeMap<Origin, Vec<Vec<String>>>,
}
impl Exports {
    pub fn new(analysis: &Analysis, index: &Index<'_>, contexts: &Context) -> Self {
        let mut bindings = BTreeMap::new();
        let mut modules = BTreeSet::new();
        let mut imports = Vec::new();
        for source in &analysis.sources {
            let Some(namespace) = contexts.namespace(&source.path) else {
                continue;
            };
            let package = index.identity(source, source.syntax.root_node()).package;
            collect(
                source.syntax.root_node(),
                source,
                &(package, namespace),
                &mut bindings,
                &mut modules,
                &mut imports,
            );
        }
        // Import chains are bounded by the number of actual import declarations.
        for _ in 0..=imports.len() {
            let mut changed = false;
            for import in &imports {
                let Some(target) = resolve(&import.owner.1, &import.path) else {
                    continue;
                };
                let matching: Vec<_> = if let Some(alias) = &import.alias {
                    bindings
                        .get(&(import.owner.0.clone(), target))
                        .map(|value| vec![(alias.clone(), value.clone())])
                        .unwrap_or_default()
                } else {
                    bindings
                        .iter()
                        .filter(|((package, path), binding)| {
                            package == &import.owner.0
                                && binding.public
                                && path.len() == target.len() + 1
                                && path.starts_with(&target)
                        })
                        .map(|((_, path), binding)| {
                            (path.last().cloned().unwrap_or_default(), binding.clone())
                        })
                        .collect()
                };
                for (name, mut binding) in matching {
                    let mut path = import.owner.1.clone();
                    path.push(name);
                    binding.public = import.public;
                    let key = (import.owner.0.clone(), path);
                    match bindings.get_mut(&key) {
                        Some(existing) => {
                            let old = existing.clone();
                            existing.origins.extend(binding.origins);
                            existing.public |= binding.public;
                            changed |= *existing != old;
                        }
                        None => {
                            bindings.insert(key, binding);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut paths = BTreeMap::<Origin, Vec<Vec<String>>>::new();
        for ((package, mut path), binding) in bindings {
            if !binding.public {
                continue;
            }
            path.pop();
            if (1..=path.len())
                .all(|length| modules.contains(&(package.clone(), path[..length].to_vec())))
            {
                for origin in binding.origins {
                    paths.entry(origin).or_default().push(path.clone());
                }
            }
        }
        Self { paths }
    }
    pub fn omits(&self, source: &Source, node: Node<'_>, module: &str) -> bool {
        let mut owner = node;
        let mut parent = node.parent();
        while let Some(item) = parent {
            if item.kind() == "trait_item" {
                owner = item;
                break;
            }
            if item.kind() == "mod_item" {
                break;
            }
            parent = item.parent();
        }
        self.paths
            .get(&(source.path.clone(), owner.start_byte()))
            .is_some_and(|paths| {
                paths
                    .iter()
                    .any(|path| !path.iter().any(|part| part == module))
            })
    }
}
fn collect(
    node: Node<'_>,
    source: &Source,
    owner: &Key,
    bindings: &mut BTreeMap<Key, Binding>,
    modules: &mut BTreeSet<Key>,
    imports: &mut Vec<Import>,
) {
    if node.kind() == "impl_item" {
        return;
    }
    let mut context = owner.clone();
    if matches!(
        node.kind(),
        "struct_item"
            | "enum_item"
            | "trait_item"
            | "function_item"
            | "type_item"
            | "const_item"
            | "static_item"
    ) {
        if let Some(name) = node.child_by_field_name("name") {
            let mut path = owner.1.clone();
            path.push(text(name, source).trim_start_matches("r#").into());
            bindings.insert(
                (owner.0.clone(), path),
                Binding {
                    origins: BTreeSet::from([(source.path.clone(), node.start_byte())]),
                    public: public(node, source),
                },
            );
        }
        return;
    }
    if node.kind() == "mod_item"
        && let Some(name) = node.child_by_field_name("name")
    {
        context
            .1
            .push(text(name, source).trim_start_matches("r#").into());
        if public(node, source) {
            modules.insert(context.clone());
        }
    }
    if node.kind() == "use_declaration"
        && let Some(argument) = node.child_by_field_name("argument")
    {
        flatten(argument, source, &[], owner, public(node, source), imports);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect(child, source, &context, bindings, modules, imports);
    }
}
fn public(node: Node<'_>, source: &Source) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| child.kind() == "visibility_modifier" && text(child, source) == "pub")
}
fn text<'a>(node: Node<'_>, source: &'a Source) -> &'a str {
    &source.text[node.byte_range()]
}
fn parts(value: &str) -> Vec<String> {
    value
        .split("::")
        .filter(|part| !part.is_empty())
        .map(|part| part.trim().trim_start_matches("r#").to_owned())
        .collect()
}
fn flatten(
    node: Node<'_>,
    source: &Source,
    prefix: &[String],
    owner: &Key,
    public: bool,
    imports: &mut Vec<Import>,
) {
    match node.kind() {
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                flatten(child, source, prefix, owner, public, imports);
            }
        }
        "scoped_use_list" => {
            let mut prefix = prefix.to_vec();
            if let Some(path) = node.child_by_field_name("path") {
                prefix.extend(parts(text(path, source)));
            }
            if let Some(list) = node.child_by_field_name("list") {
                flatten(list, source, &prefix, owner, public, imports);
            }
        }
        "use_as_clause" => {
            if let (Some(path), Some(alias)) = (
                node.child_by_field_name("path"),
                node.child_by_field_name("alias"),
            ) {
                let mut target = prefix.to_vec();
                target.extend(parts(text(path, source)));
                imports.push(Import {
                    owner: owner.clone(),
                    path: target,
                    alias: Some(text(alias, source).trim_start_matches("r#").into()),
                    public,
                });
            }
        }
        "use_wildcard" => {
            let mut target = prefix.to_vec();
            let mut cursor = node.walk();
            if let Some(path) = node.named_children(&mut cursor).next() {
                target.extend(parts(text(path, source)));
            }
            imports.push(Import {
                owner: owner.clone(),
                path: target,
                alias: None,
                public,
            });
        }
        "identifier" | "scoped_identifier" | "self" | "super" | "crate" => {
            let mut target = prefix.to_vec();
            target.extend(parts(text(node, source)));
            let alias = target.last().cloned();
            imports.push(Import {
                owner: owner.clone(),
                path: target,
                alias,
                public,
            });
        }
        _ => {}
    }
}
fn resolve(owner: &[String], path: &[String]) -> Option<Vec<String>> {
    let mut result = owner.to_vec();
    let mut index = 0;
    match path.first().map(String::as_str) {
        Some("crate") => {
            result.clear();
            index = 1;
        }
        Some("self") => index = 1,
        Some("super") => {
            while path.get(index).is_some_and(|part| part == "super") {
                result.pop()?;
                index += 1;
            }
        }
        _ => {}
    }
    result.extend(path[index..].iter().cloned());
    Some(result)
}
