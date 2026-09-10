use crate::{
    Analysis, Source,
    scope::{integration, mark_tests},
};
use std::{collections::BTreeMap, ops::Range, path::Path};
use tree_sitter::Node;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Identity {
    pub package: String,
    pub module: Vec<String>,
    pub name: String,
}
impl Identity {
    pub fn key(&self) -> String {
        format!(
            "{}::{}::{}",
            self.package,
            self.module.join("::"),
            self.name
        )
    }
    fn named(&self, name: &str) -> Self {
        Self {
            name: name.into(),
            ..self.clone()
        }
    }
}
pub(crate) struct Field {
    pub ty: Option<String>,
    pub span: Range<usize>,
}
pub(crate) struct Structure<'a> {
    pub source: &'a Source,
    pub node: Node<'a>,
    pub id: Identity,
    pub fields: BTreeMap<String, Field>,
    pub test: bool,
    pub platform: bool,
}
struct Symbol<'a> {
    id: Identity,
    source: &'a Source,
    node: Node<'a>,
}
struct Import {
    owner: Identity,
    name: String,
    path: String,
}
pub(crate) struct Index<'a> {
    pub structures: Vec<Structure<'a>>,
    symbols: Vec<Symbol<'a>>,
    imports: Vec<Import>,
    owners: BTreeMap<std::path::PathBuf, Identity>,
}
impl<'a> Index<'a> {
    pub fn new(analysis: &'a Analysis, root: &Path) -> Self {
        let mut index = Self {
            structures: Vec::new(),
            symbols: Vec::new(),
            imports: Vec::new(),
            owners: BTreeMap::new(),
        };
        for source in &analysis.sources {
            let owner = owner(source, analysis, root);
            index.owners.insert(source.path.clone(), owner.clone());
            let mut tests = vec![false; source.text.len()];
            if integration(source, root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            index.collect(source.syntax.root_node(), source, &owner, &tests, false);
        }
        let fields: Vec<_> = index
            .structures
            .iter()
            .map(|structure| index.fields(structure))
            .collect();
        for (structure, fields) in index.structures.iter_mut().zip(fields) {
            structure.fields = fields;
        }
        index
    }

    pub fn identity(&self, source: &Source, node: Node<'_>) -> Identity {
        let mut owner = self.owners.get(&source.path).cloned().unwrap_or(Identity {
            package: source.path.display().to_string(),
            module: Vec::new(),
            name: String::new(),
        });
        let mut ancestry = Vec::new();
        let mut parent = node.parent();
        while let Some(current) = parent {
            ancestry.push(current);
            parent = current.parent();
        }
        for ancestor in ancestry.into_iter().rev() {
            descend(&mut owner, ancestor, source);
        }
        owner.name = named(node, source).unwrap_or_default().into();
        owner
    }

    pub fn resolve(&self, source: &Source, node: Node<'_>, owner: &Identity) -> Option<String> {
        self.resolve_type(source, node, owner, 0)
    }

    fn collect(
        &mut self,
        node: Node<'a>,
        source: &'a Source,
        context: &Identity,
        tests: &[bool],
        platform: bool,
    ) {
        let id = context.named(named(node, source).unwrap_or_default());
        let platform = platform || platform_gated(node, source);
        if matches!(
            node.kind(),
            "struct_item" | "enum_item" | "type_item" | "union_item" | "trait_item"
        ) {
            self.symbols.push(Symbol {
                id: id.clone(),
                source,
                node,
            });
        }
        if node.kind() == "struct_item"
            && node
                .child_by_field_name("body")
                .is_some_and(|body| body.kind() == "field_declaration_list")
        {
            self.structures.push(Structure {
                source,
                node,
                id,
                fields: BTreeMap::new(),
                test: tests.get(node.start_byte()) == Some(&true),
                platform,
            });
        }
        if node.kind() == "use_declaration"
            && let Some(argument) = node.child_by_field_name("argument")
        {
            self.imports(argument, source, context, "");
        }
        let mut child_context = context.clone();
        descend(&mut child_context, node, source);
        for child in children(node) {
            self.collect(child, source, &child_context, tests, platform);
        }
    }

    fn fields(&self, structure: &Structure<'_>) -> BTreeMap<String, Field> {
        let Some(body) = structure.node.child_by_field_name("body") else {
            return BTreeMap::new();
        };
        children(body)
            .into_iter()
            .filter(|field| field.kind() == "field_declaration")
            .filter_map(|field| {
                let name = named(field, structure.source)?;
                let node = field.child_by_field_name("type")?;
                Some((
                    name.into(),
                    Field {
                        ty: self.resolve(structure.source, node, &structure.id),
                        span: field.byte_range(),
                    },
                ))
            })
            .collect()
    }

    fn imports(&mut self, node: Node<'_>, source: &Source, owner: &Identity, prefix: &str) {
        let text = &source.text[node.byte_range()];
        match node.kind() {
            "use_list" => {
                for child in children(node) {
                    self.imports(child, source, owner, prefix);
                }
            }
            "scoped_use_list" => {
                let path = node
                    .child_by_field_name("path")
                    .map(|path| &source.text[path.byte_range()])
                    .unwrap_or_default();
                let prefix = join(prefix, path);
                if let Some(list) = node.child_by_field_name("list") {
                    self.imports(list, source, owner, &prefix);
                }
            }
            "use_as_clause" => {
                let Some(path) = node.child_by_field_name("path") else {
                    return;
                };
                let Some(alias) = node.child_by_field_name("alias") else {
                    return;
                };
                self.imports.push(Import {
                    owner: owner.clone(),
                    name: source.text[alias.byte_range()].into(),
                    path: join(prefix, &source.text[path.byte_range()]),
                });
            }
            "use_wildcard" => {} // Glob imports do not establish unambiguous identity.
            _ => {
                let path = join(prefix, text);
                let name = if text == "self" {
                    prefix.rsplit("::").next().unwrap_or_default()
                } else {
                    text.rsplit("::").next().unwrap_or_default()
                };
                self.imports.push(Import {
                    owner: owner.clone(),
                    name: name.into(),
                    path,
                });
            }
        }
    }

    fn resolve_type(
        &self,
        source: &Source,
        node: Node<'_>,
        owner: &Identity,
        depth: usize,
    ) -> Option<String> {
        if depth > 32 {
            return None;
        }
        let text = &source.text[node.byte_range()];
        match node.kind() {
            "primitive_type" => Some(format!("primitive:{text}")),
            "type_identifier" | "scoped_type_identifier" => {
                self.resolve_path(text, owner, depth + 1)
            }
            "generic_type" => {
                let constructor =
                    self.resolve_type(source, node.child_by_field_name("type")?, owner, depth + 1)?;
                let args = children(node.child_by_field_name("type_arguments")?)
                    .into_iter()
                    .map(|arg| self.resolve_type(source, arg, owner, depth + 1))
                    .collect::<Option<Vec<_>>>()?;
                Some(format!("{constructor}<{}>", args.join(",")))
            }
            "reference_type" | "pointer_type" => {
                let inner =
                    self.resolve_type(source, node.child_by_field_name("type")?, owner, depth + 1)?;
                let prefix: String = text
                    [..node.child_by_field_name("type")?.start_byte() - node.start_byte()]
                    .split_whitespace()
                    .collect();
                Some(format!("{prefix}{inner}"))
            }
            "tuple_type" => {
                let fields = children(node)
                    .into_iter()
                    .map(|child| self.resolve_type(source, child, owner, depth + 1))
                    .collect::<Option<Vec<_>>>()?;
                Some(format!("({})", fields.join(",")))
            }
            "array_type" => {
                let inner = self.resolve_type(
                    source,
                    node.child_by_field_name("element")?,
                    owner,
                    depth + 1,
                )?;
                let length = node
                    .child_by_field_name("length")
                    .map(|length| source.text[length.byte_range()].trim());
                if length
                    .is_some_and(|value| !value.chars().all(|ch| ch.is_ascii_digit() || ch == '_'))
                {
                    return None;
                }
                Some(format!("[{inner};{}]", length.unwrap_or("slice")))
            }
            _ => None,
        }
    }

    fn resolve_path(&self, path: &str, owner: &Identity, depth: usize) -> Option<String> {
        if depth > 32 {
            return None;
        }
        let path: String = path.split_whitespace().collect();
        let mut parts: Vec<_> = path.trim_start_matches("::").split("::").collect();
        let first = *parts.first()?;
        let mut base = owner.clone();
        match first {
            "crate" => {
                base.module.clear();
                parts.remove(0);
            }
            "self" => {
                parts.remove(0);
            }
            "super" => {
                while parts.first() == Some(&"super") {
                    base.module.pop()?;
                    parts.remove(0);
                }
            }
            _ => {
                let imports: Vec<_> = self
                    .imports
                    .iter()
                    .filter(|import| {
                        import.owner.package == owner.package
                            && import.owner.module == owner.module
                            && import.name == first
                    })
                    .collect();
                if imports.len() > 1 {
                    return None;
                }
                if let Some(import) = imports.first() {
                    let suffix = parts[1..].join("::");
                    return self.resolve_path(&join(&import.path, &suffix), owner, depth + 1);
                }
            }
        }
        let name = parts.pop()?;
        base.module.extend(parts.iter().map(|part| (*part).into()));
        base.name = name.into();
        let symbols: Vec<_> = self
            .symbols
            .iter()
            .filter(|symbol| symbol.id == base)
            .collect();
        if symbols.len() > 1 {
            return None;
        }
        if let Some(symbol) = symbols.first() {
            if symbol.node.kind() != "type_item" {
                return Some(format!("nominal:{}", base.key()));
            }
            if symbol.node.child_by_field_name("type_parameters").is_some() {
                return None;
            }
            return self.resolve_type(
                symbol.source,
                symbol.node.child_by_field_name("type")?,
                &base,
                depth + 1,
            );
        }
        standard(&path).map(str::to_owned)
    }
}

fn standard(path: &str) -> Option<&'static str> {
    match path {
        "From" | "std::convert::From" | "core::convert::From" => Some("std:From"),
        "TryFrom" | "std::convert::TryFrom" | "core::convert::TryFrom" => Some("std:TryFrom"),
        "String" | "std::string::String" | "alloc::string::String" => Some("std:String"),
        "Vec" | "std::vec::Vec" | "alloc::vec::Vec" => Some("std:Vec"),
        "Option" | "std::option::Option" | "core::option::Option" => Some("std:Option"),
        "Result" | "std::result::Result" | "core::result::Result" => Some("std:Result"),
        "Box" | "std::boxed::Box" | "alloc::boxed::Box" => Some("std:Box"),
        _ => None,
    }
}
fn join(prefix: &str, suffix: &str) -> String {
    match (prefix.is_empty(), suffix.is_empty()) {
        (true, _) => suffix.into(),
        (_, true) => prefix.into(),
        _ => format!("{prefix}::{suffix}"),
    }
}
fn named<'a>(node: Node<'_>, source: &'a Source) -> Option<&'a str> {
    Some(&source.text[node.child_by_field_name("name")?.byte_range()])
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}
fn descend(owner: &mut Identity, node: Node<'_>, source: &Source) {
    match node.kind() {
        "mod_item" => {
            if let Some(name) = named(node, source) {
                owner.module.push(name.into());
            }
        }
        "function_item" | "impl_item" => {
            owner
                .module
                .push(format!("@{}:{}", source.path.display(), node.start_byte()))
        }
        _ => {}
    }
}
fn platform_gated(node: Node<'_>, source: &Source) -> bool {
    let mut sibling = node.prev_named_sibling();
    while let Some(attribute) = sibling {
        match attribute.kind() {
            "attribute_item" => {
                let text = &source.text[attribute.byte_range()];
                if text.contains("cfg")
                    && ["unix", "windows", "target_"]
                        .iter()
                        .any(|word| text.contains(word))
                {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        sibling = attribute.prev_named_sibling();
    }
    false
}
fn owner(source: &Source, analysis: &Analysis, root: &Path) -> Identity {
    let absolute = root.join(&source.path);
    let package = analysis
        .packages
        .keys()
        .filter(|manifest| {
            manifest
                .parent()
                .is_some_and(|parent| absolute.starts_with(parent))
        })
        .max_by_key(|manifest| manifest.components().count());
    let directory = package
        .and_then(|manifest| manifest.parent())
        .unwrap_or(root);
    let relative = absolute.strip_prefix(directory).unwrap_or(&source.path);
    let relative = relative.strip_prefix("src").unwrap_or(relative);
    let mut module: Vec<String> = relative
        .parent()
        .into_iter()
        .flat_map(|parent| parent.components())
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    let stem = relative
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !matches!(stem, "lib" | "main" | "mod") {
        module.push(stem.into());
    }
    Identity {
        package: package
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| root.display().to_string()),
        module,
        name: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn analysis(text: &str) -> Analysis {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let syntax = parser.parse(text, None).unwrap();
        assert!(!syntax.root_node().has_error());
        Analysis {
            packages: BTreeMap::new(),
            sources: vec![Source {
                path: "lib.rs".into(),
                text: text.into(),
                syntax,
            }],
        }
    }
    #[test]
    fn resolves_aliases_imports_and_nominal_wrappers_without_erasing_identity() {
        let analysis = analysis(
            "mod ids { pub struct Email(String); pub struct WalletId(String); } use crate::ids::{Email, WalletId as Id}; type Address = Email; struct Value { a: Address, b: Email, c: Id, d: String, e: Vec<Email> }",
        );
        let index = Index::new(&analysis, Path::new("/project"));
        let fields = &index.structures[0].fields;
        assert_eq!(fields["a"].ty, fields["b"].ty);
        assert!(fields["a"].ty.as_ref().unwrap().contains("Email"));
        assert_ne!(fields["a"].ty, fields["c"].ty);
        assert_eq!(fields["d"].ty.as_deref(), Some("std:String"));
        assert!(
            fields["e"]
                .ty
                .as_ref()
                .unwrap()
                .starts_with("std:Vec<nominal:")
        );
    }
    #[test]
    fn unresolved_fields_and_ambiguous_declarations_do_not_establish_identity() {
        let analysis = analysis(
            "struct Id; struct Id; struct Value { id: Id, unknown: External, count: u64 }",
        );
        let index = Index::new(&analysis, Path::new("/project"));
        let fields = &index.structures[0].fields;
        assert!(fields["id"].ty.is_none());
        assert!(fields["unknown"].ty.is_none());
        assert_eq!(fields["count"].ty.as_deref(), Some("primitive:u64"));
    }
}
