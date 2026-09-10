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
        let Some(trait_node) = node.child_by_field_name("trait") else {
            if !methods.is_empty() {
                for method in &methods {
                    self.behaviors
                        .insert((to.clone(), tests[method.start_byte()]));
                }
            }
            return;
        };
        let Some(name) = trait_node.child_by_field_name("type") else {
            return;
        };
        if !matches!(
            index.resolve(source, name, &owner).as_deref(),
            Some("std:From" | "std:TryFrom")
        ) {
            return;
        }
        let Some(arguments) = trait_node.child_by_field_name("type_arguments") else {
            return;
        };
        let mut cursor = arguments.walk();
        let Some(argument) = arguments.named_children(&mut cursor).next() else {
            return;
        };
        let Some(from) = index.resolve(source, argument, &owner) else {
            return;
        };
        if methods.len() != 1 {
            return;
        }
        self.conversions.push(Conversion {
            from,
            to,
            copied: copied(methods[0], source),
            test: tests[methods[0].start_byte()],
            source,
            node,
        });
    }
}

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
        && fields.iter().all(|field| {
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
        })
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
