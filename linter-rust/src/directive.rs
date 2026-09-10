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
            next.map(|next| Span::new(&source.text, node.end_byte()..next.end_byte())),
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
    use linter::Registry;
    fn report(comment: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"rust/function-length\"]]\ntarget='*.rs'\nmax_lines=1\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("input.rs"),
            format!("{comment}\nfn run() {{\n    let _value = 1;\n}}\n"),
        )
        .unwrap();
        Registry::default()
            .register::<crate::FunctionLength>()
            .unwrap()
            .check(root.path())
            .unwrap()
    }
    #[test]
    fn suppresses_attached_item_with_auditable_reason() {
        for prefix in ["//", "///"] {
            let result = report(&format!(
                "{prefix} linter:disable rust/function-length -- Fixed external contract."
            ));
            assert!(result.findings.is_empty(), "{:?}", result.findings);
            assert_eq!(result.suppressed.len(), 1);
            assert_eq!(result.suppressed[0].reason, "Fixed external contract.");
        }
    }
    #[test]
    fn rejects_unknown_and_unreasoned_directives() {
        for comment in [
            "// linter:disable typo -- reason",
            "// linter:disable rust/function-length",
            "// linter:disable rust/function-length -- ",
        ] {
            let result = report(comment);
            assert_eq!(result.findings.len(), 2);
            assert!(
                result
                    .findings
                    .iter()
                    .any(|finding| finding.rule == "directive")
            );
            assert!(result.suppressed.is_empty());
        }
    }
    #[test]
    fn detects_unused_and_unattached_directives() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"rust/function-length\"]]\ntarget='*.rs'\n",
        )
        .unwrap();
        std::fs::write(root.path().join("input.rs"), "// linter:disable rust/function-length -- reason\nfn run() {}\n// linter:disable rust/function-length -- reason").unwrap();
        let report = Registry::default()
            .register::<crate::FunctionLength>()
            .unwrap()
            .check(root.path())
            .unwrap();
        assert_eq!(report.findings.len(), 2);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message.contains("not attached"))
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message.contains("did not suppress"))
        );
    }
}
