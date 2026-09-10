use crate::Source;
use tree_sitter::Node;

pub(super) fn comments<'tree>(node: Node<'tree>, output: &mut Vec<Node<'tree>>) {
    if matches!(node.kind(), "line_comment" | "block_comment") {
        output.push(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        comments(child, output);
    }
}
pub(super) fn present(node: Node<'_>, source: &Source, comments: &[Node<'_>]) -> bool {
    if preceding(node.start_byte(), source, comments)
        || preceding(anchor(node).start_byte(), source, comments)
    {
        return true;
    }
    let body = if node.kind() == "unsafe_block" {
        node.child_by_field_name("body")
            .or_else(|| node.named_child(0))
    } else {
        node.next_named_sibling()
    };
    let Some(body) = body.filter(|body| source.text[body.byte_range()].starts_with('{')) else {
        return false;
    };
    let mut after = body.start_byte() + 1;
    let mut texts = Vec::new();
    for comment in comments.iter().filter(|comment| {
        comment.start_byte() >= body.start_byte() && comment.end_byte() <= body.end_byte()
    }) {
        if !source.text[after..comment.start_byte()]
            .chars()
            .all(char::is_whitespace)
        {
            break;
        }
        texts.push(&source.text[comment.byte_range()]);
        after = comment.end_byte();
    }
    explanation(&texts)
}
fn anchor(mut node: Node<'_>) -> Node<'_> {
    while let Some(parent) = node.parent() {
        if matches!(
            node.kind(),
            "let_declaration"
                | "expression_statement"
                | "function_item"
                | "impl_item"
                | "trait_item"
                | "macro_invocation"
                | "macro_definition"
        ) {
            break;
        }
        if matches!(
            parent.kind(),
            "block" | "source_file" | "declaration_list" | "token_tree"
        ) {
            break;
        }
        node = parent;
    }
    node
}
fn preceding(mut before: usize, source: &Source, comments: &[Node<'_>]) -> bool {
    let anchor = before;
    let mut texts = Vec::new();
    for comment in comments
        .iter()
        .rev()
        .filter(|comment| comment.end_byte() <= anchor)
    {
        let gap = &source.text[comment.end_byte()..before];
        if !gap.chars().all(char::is_whitespace)
            || gap.bytes().filter(|byte| *byte == b'\n').count() > 1
        {
            break;
        }
        texts.push(&source.text[comment.byte_range()]);
        before = comment.start_byte();
    }
    texts.reverse();
    explanation(&texts)
}
fn explanation(texts: &[&str]) -> bool {
    let content = texts
        .iter()
        .flat_map(|text| {
            text.trim_start_matches('/')
                .trim_start_matches('*')
                .trim_end_matches("*/")
                .lines()
        })
        .map(|line| line.trim().trim_start_matches('*').trim())
        .collect::<Vec<_>>()
        .join("\n");
    content
        .split_once("SAFETY:")
        .is_some_and(|(_, reason)| reason.chars().any(char::is_alphanumeric))
}
