use crate::Source;
use linter::{Directive, Span};
use tree_sitter::Node;

pub(crate) fn collect(source: &Source) -> Vec<Directive> {
    let mut output = Vec::new();
    visit(source.syntax.root_node(), source, &mut output);
    output
}

fn visit(node: Node<'_>, source: &Source, output: &mut Vec<Directive>) {
    if matches!(node.kind(), "line_comment" | "block_comment" | "comment") {
        let mut next = node.next_named_sibling();
        while next.is_some_and(|next| {
            matches!(
                next.kind(),
                "attribute_item" | "line_comment" | "block_comment" | "comment"
            )
        }) {
            next = next.and_then(|next| next.next_named_sibling());
        }
        if let Some(directive) = Directive::parse(
            &source.path,
            &source.text[node.byte_range()],
            Span::new(&source.text, node.byte_range()),
            next.map(|next| Span::new(&source.text, next.byte_range())),
        ) {
            output.push(directive);
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(child, source, output);
    }
}
