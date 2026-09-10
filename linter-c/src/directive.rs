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

#[cfg(test)]
mod tests {
    #[test]
    fn suppresses_only_named_rule_and_rejects_obsolete_directive() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("linter.toml"), "[[rules.\"c/function-length\"]]\ntarget='*.c'\nmax_lines=1\n[[rules.\"c/file-length\"]]\ntarget='*.c'\nmax_lines=1").unwrap();
        let source = "// linter:disable c/function-length -- Fixed generated algorithm.\nint run(void) {\nreturn 0;\n}\n";
        std::fs::write(root.path().join("input.c"), source).unwrap();
        let registry = linter::Registry::default()
            .register::<crate::FunctionLength>()
            .unwrap()
            .register::<crate::FileLength>()
            .unwrap();
        let report = registry.check(root.path()).unwrap();
        assert_eq!(report.suppressed.len(), 1);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule, "c/file-length");
        std::fs::write(
            root.path().join("input.c"),
            "// linter:disable c/function-length -- No longer large.\nint run(void) { return 0; }",
        )
        .unwrap();
        let report = registry.check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule, "directive");
    }
}
