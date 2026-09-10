use crate::{Analysis, Source, declaration::Index};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use tree_sitter::Node;

pub(crate) struct Context {
    fallback: BTreeMap<PathBuf, Vec<String>>,
    parents: BTreeMap<PathBuf, Vec<(PathBuf, Vec<String>)>>,
}
impl Context {
    pub fn new(analysis: &Analysis, index: &Index<'_>, root: &Path) -> Self {
        let mut value = Self {
            fallback: BTreeMap::new(),
            parents: BTreeMap::new(),
        };
        for source in &analysis.sources {
            let mut namespace = index.identity(source, source.syntax.root_node()).module;
            if analysis.packages.values().any(|package| {
                package
                    .targets
                    .iter()
                    .any(|target| target.src_path.as_std_path() == root.join(&source.path))
            }) {
                namespace.clear();
            }
            value.fallback.insert(source.path.clone(), namespace);
        }
        for source in &analysis.sources {
            value.collect(source.syntax.root_node(), source, root, &[]);
        }
        value
    }
    pub fn namespace(&self, path: &Path) -> Option<Vec<String>> {
        self.resolve(path, &mut BTreeSet::new())
    }
    fn resolve(&self, path: &Path, seen: &mut BTreeSet<PathBuf>) -> Option<Vec<String>> {
        if !seen.insert(path.to_owned()) {
            return None;
        }
        let output = if let Some(parents) = self.parents.get(path) {
            let mut choices = BTreeSet::new();
            for (parent, suffix) in parents {
                let mut resolved = self.resolve(parent, seen)?;
                resolved.extend(suffix.clone());
                choices.insert(resolved);
            }
            if choices.len() == 1 {
                choices.into_iter().next()
            } else {
                None
            }
        } else {
            self.fallback.get(path).cloned()
        };
        seen.remove(path);
        output
    }
    fn collect(&mut self, node: Node<'_>, source: &Source, root: &Path, inline: &[String]) {
        let mut next = inline.to_vec();
        if node.kind() == "mod_item" {
            let Some(name) = node.child_by_field_name("name") else {
                return;
            };
            let name = source.text[name.byte_range()]
                .trim_start_matches("r#")
                .to_owned();
            next.push(name.clone());
            if node.child_by_field_name("body").is_none() {
                let parent = source.path.parent().unwrap_or(Path::new(""));
                let stem = source
                    .path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default();
                let mut directory = parent.to_owned();
                let explicit = explicit(node, source);
                if !self.fallback.get(&source.path).is_some_and(Vec::is_empty)
                    && !matches!(stem, "lib" | "main" | "mod")
                    && (explicit.is_none() || !inline.is_empty())
                {
                    directory.push(stem);
                }
                for component in inline {
                    directory.push(component);
                }
                let candidates = if let Some(path) = explicit {
                    vec![directory.join(path)]
                } else {
                    vec![
                        directory.join(format!("{name}.rs")),
                        directory.join(&name).join("mod.rs"),
                    ]
                };
                for path in candidates {
                    let Some(path) = std::fs::canonicalize(root.join(path))
                        .ok()
                        .and_then(|path| path.strip_prefix(root).ok().map(Path::to_owned))
                    else {
                        continue;
                    };
                    if self.fallback.contains_key(&path) {
                        self.parents
                            .entry(path)
                            .or_default()
                            .push((source.path.clone(), next.clone()));
                    }
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.collect(child, source, root, &next);
        }
    }
}
fn explicit(node: Node<'_>, source: &Source) -> Option<String> {
    let mut previous = node.prev_named_sibling();
    while let Some(attribute) = previous {
        if !matches!(
            attribute.kind(),
            "attribute_item" | "line_comment" | "block_comment"
        ) {
            break;
        }
        let mut cursor = attribute.walk();
        if let Some(meta) = attribute
            .named_children(&mut cursor)
            .find(|node| node.kind() == "attribute")
            && let Ok(syn::Meta::NameValue(value)) = syn::parse_str(&source.text[meta.byte_range()])
            && value.path.is_ident("path")
            && let syn::Expr::Lit(value) = value.value
            && let syn::Lit::Str(value) = value.lit
        {
            return Some(value.value());
        }
        previous = attribute.prev_named_sibling();
    }
    None
}
