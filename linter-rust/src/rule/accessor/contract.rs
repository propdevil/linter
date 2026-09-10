use crate::{
    Source,
    declaration::{Identity, Index, Structure},
};
use tree_sitter::Node;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Visibility {
    Public,
    Module(Vec<String>),
}
#[derive(Clone, PartialEq, Eq)]
enum Shape {
    Value,
    Shared,
    Mutable,
    Clone,
    Set,
}
pub(super) struct Access<'a> {
    pub source: &'a Source,
    pub method: Node<'a>,
    pub field: Node<'a>,
    pub name: String,
    pub field_name: String,
    pub exposed: bool,
    pub visibility: Visibility,
    shape: Shape,
    contract: String,
}
impl Access<'_> {
    pub fn equivalent(&self, other: &Self) -> bool {
        self.field_name == other.field_name
            && self.shape == other.shape
            && self.contract == other.contract
            && self.visibility == other.visibility
    }
}

impl<'a> Access<'a> {
    pub fn new(
        method: Node<'a>,
        source: &'a Source,
        index: &Index<'a>,
        structure: &Structure<'a>,
    ) -> Option<Self> {
        let (field_name, shape) = Shape::parse(method, source)?;
        let indexed = structure.fields.get(&field_name)?;
        let field = structure
            .node
            .child_by_field_name("body")?
            .named_descendant_for_byte_range(indexed.span.start, indexed.span.end)?;
        if boundary(field, structure.source) {
            return None;
        }
        let context = index.identity(source, method.parent()?.parent()?);
        let contract = shape.contract(method, source, index, indexed.ty.as_ref()?)?;
        let method_visibility = visibility(method, source, &context)?;
        let field_visibility =
            visibility_node(field).and_then(|_| visibility(field, structure.source, &structure.id));
        let exposed = field_visibility
            .as_ref()
            .is_some_and(|field| broader(field, &method_visibility));
        Some(Self {
            source,
            method,
            field,
            name: source.text[method.child_by_field_name("name")?.byte_range()].to_owned(),
            field_name,
            exposed,
            visibility: method_visibility,
            shape,
            contract,
        })
    }
}
impl Shape {
    fn parse(method: Node<'_>, source: &Source) -> Option<(String, Self)> {
        if boundary(method, source) {
            return None;
        }
        let prefix =
            &source.text[method.start_byte()..method.child_by_field_name("name")?.start_byte()];
        if prefix
            .split(|c: char| !c.is_ascii_alphabetic())
            .any(|word| matches!(word, "async" | "unsafe" | "extern"))
        {
            return None;
        }
        if method.child_by_field_name("type_parameters").is_some()
            || children(method)
                .iter()
                .any(|child| child.kind() == "where_clause")
        {
            return None;
        }
        let params = children(method.child_by_field_name("parameters")?);
        if params.first()?.kind() != "self_parameter" {
            return None;
        }
        let expressions: Vec<_> = children(method.child_by_field_name("body")?)
            .into_iter()
            .filter(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
            .collect();
        if expressions.len() != 1 {
            return None;
        }
        let expression = peel(expressions[0]);
        if let Some((field, shape)) = getter(expression, source) {
            return (params.len() == 1).then_some((field, shape));
        }
        Some((setter(expression, &params, method, source)?, Self::Set))
    }
    fn contract(
        &self,
        method: Node<'_>,
        source: &Source,
        index: &Index<'_>,
        field_type: &str,
    ) -> Option<String> {
        let params = children(method.child_by_field_name("parameters")?);
        let context = index.identity(source, method.parent()?.parent()?);
        let ty = if *self == Self::Set {
            index.resolve(source, params[1].child_by_field_name("type")?, &context)?
        } else {
            index.resolve(source, method.child_by_field_name("return_type")?, &context)?
        };
        let expected = match self {
            Self::Shared => format!("&{field_type}"),
            Self::Mutable => format!("&mut{field_type}"),
            _ => field_type.to_owned(),
        };
        if ty != expected {
            return None;
        }
        let receiver: String = source.text[params.first()?.byte_range()]
            .split_whitespace()
            .collect();
        let prefix =
            &source.text[method.start_byte()..method.child_by_field_name("name")?.start_byte()];
        let modifiers: String = prefix
            .split_whitespace()
            .filter(|word| matches!(*word, "const"))
            .collect();
        Some(format!("{receiver}:{ty}:{modifiers}"))
    }
}

fn setter(
    expression: Node<'_>,
    params: &[Node<'_>],
    method: Node<'_>,
    source: &Source,
) -> Option<String> {
    if expression.kind() != "assignment_expression" || params.len() != 2 {
        return None;
    }
    if method
        .child_by_field_name("return_type")
        .is_some_and(|ty| &source.text[ty.byte_range()] != "()")
    {
        return None;
    }
    let field = self_field(expression.child_by_field_name("left")?, source)?;
    let value = peel(expression.child_by_field_name("right")?);
    let parameter = params[1].child_by_field_name("pattern")?;
    (value.kind() == "identifier"
        && parameter.kind() == "identifier"
        && source.text[value.byte_range()] == source.text[parameter.byte_range()])
    .then_some(field)
}
fn getter(node: Node<'_>, source: &Source) -> Option<(String, Shape)> {
    let node = peel(node);
    if let Some(field) = self_field(node, source) {
        return Some((field, Shape::Value));
    }
    if node.kind() == "reference_expression" {
        let value = node.child_by_field_name("value")?;
        let mutable = children(node)
            .iter()
            .any(|child| child.kind() == "mutable_specifier");
        return Some((
            self_field(peel(value), source)?,
            if mutable {
                Shape::Mutable
            } else {
                Shape::Shared
            },
        ));
    }
    if node.kind() == "call_expression" {
        let function = node.child_by_field_name("function")?;
        if function.kind() != "field_expression"
            || !children(node.child_by_field_name("arguments")?).is_empty()
        {
            return None;
        }
        if &source.text[function.child_by_field_name("field")?.byte_range()] != "clone" {
            return None;
        }
        return Some((
            self_field(peel(function.child_by_field_name("value")?), source)?,
            Shape::Clone,
        ));
    }
    None
}
fn self_field(node: Node<'_>, source: &Source) -> Option<String> {
    if node.kind() != "field_expression" {
        return None;
    }
    let value = peel(node.child_by_field_name("value")?);
    let field = node.child_by_field_name("field")?;
    (source.text[value.byte_range()] == *"self" && field.kind() == "field_identifier")
        .then(|| source.text[field.byte_range()].to_owned())
}
fn peel(node: Node<'_>) -> Node<'_> {
    if matches!(
        node.kind(),
        "parenthesized_expression" | "expression_statement" | "return_expression"
    ) {
        children(node).first().copied().map_or(node, peel)
    } else {
        node
    }
}
pub(super) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}
fn visibility_node(node: Node<'_>) -> Option<Node<'_>> {
    children(node)
        .into_iter()
        .find(|child| child.kind() == "visibility_modifier")
}
fn visibility(node: Node<'_>, source: &Source, owner: &Identity) -> Option<Visibility> {
    let mut module: Vec<_> = owner
        .module
        .iter()
        .filter(|part| !part.starts_with('@'))
        .cloned()
        .collect();
    let Some(node) = visibility_node(node) else {
        return Some(Visibility::Module(module));
    };
    let value: String = source.text[node.byte_range()].split_whitespace().collect();
    match value.as_str() {
        "pub" => Some(Visibility::Public),
        "pub(crate)" => Some(Visibility::Module(Vec::new())),
        "pub(self)" => Some(Visibility::Module(module)),
        "pub(super)" => {
            module.pop();
            Some(Visibility::Module(module))
        }
        _ => {
            let value = value.strip_prefix("pub(in")?.strip_suffix(')')?;
            let parts: Vec<_> = value.split("::").collect();
            match parts.first().copied() {
                Some("crate") => module.clear(),
                Some("self") => {}
                Some("super") => {
                    module.pop();
                }
                _ => return None,
            }
            module.extend(parts.into_iter().skip(1).map(str::to_owned));
            Some(Visibility::Module(module))
        }
    }
}
fn broader(field: &Visibility, method: &Visibility) -> bool {
    match (field, method) {
        (Visibility::Public, _) => true,
        (Visibility::Module(field), Visibility::Module(method)) => method.starts_with(field),
        _ => false,
    }
}
pub(super) fn boundary(node: Node<'_>, source: &Source) -> bool {
    let attributes =
        std::iter::successors(node.prev_named_sibling(), |node| node.prev_named_sibling())
            .take_while(|node| {
                matches!(
                    node.kind(),
                    "attribute_item" | "line_comment" | "block_comment"
                )
            })
            .filter_map(|node| {
                children(node)
                    .into_iter()
                    .find(|child| child.kind() == "attribute")
            });
    attributes
        .filter_map(|node| {
            let text = &source.text[node.byte_range()];
            syn::parse_str::<syn::Meta>(text)
                .ok()
                .map(|meta| (text, meta))
        })
        .any(|(text, meta)| {
            let name = meta
                .path()
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
                .unwrap_or_default();
            let serialized = name == "derive"
                && text
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .any(|word| matches!(word, "Serialize" | "Deserialize"));
            matches!(
                name.as_str(),
                "deprecated"
                    | "serde"
                    | "repr"
                    | "no_mangle"
                    | "export_name"
                    | "link_name"
                    | "cfg_attr"
            ) || (name == "cfg" && text.replace(' ', "") != "cfg(test)")
                || serialized
        })
}
