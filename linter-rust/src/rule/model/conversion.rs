use crate::{
    Analysis, Source,
    declaration::Index,
    scope::{integration, mark_tests},
};
use std::collections::BTreeSet;
use tree_sitter::Node;

pub(super) struct Conversion<'a> {
    pub from: String,
    pub to: String,
    pub copied: bool,
    pub test: bool,
    pub source: &'a Source,
    pub node: Node<'a>,
}

#[derive(Default)]
pub(super) struct Evidence<'a> {
    pub behaviors: BTreeSet<(String, bool)>,
    pub conversions: Vec<Conversion<'a>>,
}

impl<'a> Evidence<'a> {
    pub fn collect(index: &Index<'a>, analysis: &'a Analysis, root: &std::path::Path) -> Self {
        let mut evidence = Self::default();
        for source in &analysis.sources {
            let mut tests = vec![false; source.text.len()];
            if integration(source, root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            evidence.visit(source.syntax.root_node(), source, index, &tests);
        }
        evidence
    }

    pub fn matching(
        &self,
        first: &str,
        second: &str,
        scope: super::Scope,
    ) -> Option<Vec<&Conversion<'a>>> {
        let matching: Vec<_> = self
            .conversions
            .iter()
            .filter(|conversion| {
                super::scope_selected(scope, conversion.test)
                    && ((conversion.from == first && conversion.to == second)
                        || (conversion.from == second && conversion.to == first))
            })
            .collect();
        (!matching.iter().any(|conversion| !conversion.copied)).then_some(matching)
    }

    fn visit(&mut self, node: Node<'a>, source: &'a Source, index: &Index<'a>, tests: &[bool]) {
        if node.kind() == "impl_item" {
            self.implementation(node, source, index, tests);
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, source, index, tests);
        }
    }

    fn implementation(
        &mut self,
        node: Node<'a>,
        source: &'a Source,
        index: &Index<'a>,
        tests: &[bool],
    ) {
        let Some(target) = node.child_by_field_name("type") else {
            return;
        };
        let owner = index.identity(source, node);
        let Some(to) = index.resolve(source, target, &owner) else {
            return;
        };
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let mut cursor = body.walk();
        let methods: Vec<_> = body
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "function_item")
            .collect();
        if node.child_by_field_name("trait").is_none() {
            self.behaviors.extend(
                methods
                    .iter()
                    .map(|method| (to.clone(), tests[method.start_byte()])),
            );
            return;
        }
        self.conversions
            .extend(Conversion::new(node, source, index, tests, to, &methods));
    }
}

impl<'a> Conversion<'a> {
    fn new(
        node: Node<'a>,
        source: &'a Source,
        index: &Index<'a>,
        tests: &[bool],
        to: String,
        methods: &[Node<'a>],
    ) -> Option<Self> {
        let owner = index.identity(source, node);
        let trait_node = node.child_by_field_name("trait")?;
        let name = trait_node.child_by_field_name("type")?;
        if !matches!(
            index.resolve(source, name, &owner).as_deref(),
            Some("std:From" | "std:TryFrom")
        ) {
            return None;
        }
        let arguments = trait_node.child_by_field_name("type_arguments")?;
        let mut cursor = arguments.walk();
        let argument = arguments.named_children(&mut cursor).next()?;
        let from = index.resolve(source, argument, &owner)?;
        if methods.len() != 1 {
            return None;
        }
        Some(Self {
            from,
            to,
            copied: Self::copied(methods[0], source),
            test: tests[methods[0].start_byte()],
            source,
            node,
        })
    }
}

impl Conversion<'_> {
    fn copied(method: Node<'_>, source: &Source) -> bool {
        let Some(parameters) = method.child_by_field_name("parameters") else {
            return false;
        };
        let mut cursor = parameters.walk();
        let parameters: Vec<_> = parameters
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "parameter")
            .collect();
        if parameters.len() != 1 {
            return false;
        }
        let Some(parameter) = parameters[0].child_by_field_name("pattern") else {
            return false;
        };
        if parameter.kind() != "identifier" {
            return false;
        }
        let parameter = &source.text[parameter.byte_range()];
        let Some(body) = method.child_by_field_name("body") else {
            return false;
        };
        let mut cursor = body.walk();
        let expressions: Vec<_> = body
            .named_children(&mut cursor)
            .filter(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
            .collect();
        if expressions.len() != 1 {
            return false;
        }
        let Some(expression) = result(expressions[0], source) else {
            return false;
        };
        let Some(fields) = expression.child_by_field_name("body") else {
            return false;
        };
        let mut cursor = fields.walk();
        let fields: Vec<_> = fields
            .named_children(&mut cursor)
            .filter(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
            .collect();
        !fields.is_empty()
            && fields
                .iter()
                .all(|field| Self::field(*field, parameter, source))
    }
    fn field(field: Node<'_>, parameter: &str, source: &Source) -> bool {
        if field.kind() != "field_initializer" {
            return false;
        }
        let Some(value) = field.child_by_field_name("value") else {
            return false;
        };
        if value.kind() != "field_expression" {
            return false;
        }
        value.child_by_field_name("value").is_some_and(|value| {
            value.kind() == "identifier" && &source.text[value.byte_range()] == parameter
        })
    }
}

fn result<'a>(node: Node<'a>, source: &Source) -> Option<Node<'a>> {
    match node.kind() {
        "struct_expression" => Some(node),
        "expression_statement" | "return_expression" => {
            let mut cursor = node.walk();
            result(node.named_children(&mut cursor).next()?, source)
        }
        "call_expression" => {
            let function = node.child_by_field_name("function")?;
            if &source.text[function.byte_range()] != "Ok" {
                return None;
            }
            let arguments = node.child_by_field_name("arguments")?;
            let mut cursor = arguments.walk();
            let args: Vec<_> = arguments.named_children(&mut cursor).collect();
            if args.len() != 1 {
                return None;
            }
            result(args[0], source)
        }
        _ => None,
    }
}
