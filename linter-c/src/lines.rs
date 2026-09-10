use crate::Source;
use linter::Error;
use std::ops::Range;
use tree_sitter::Node;

/// Preserve source offsets and newlines while removing syntax-identified comments.
pub(crate) fn without_comments(source: &Source) -> Result<String, Error> {
    let mut bytes = source.text.as_bytes().to_vec();
    erase_comments(source.syntax.root_node(), &mut bytes);
    String::from_utf8(bytes).map_err(|error| {
        Error::Analysis(format!(
            "{}: invalid comment spans: {error}",
            source.path.display()
        ))
    })
}

pub(crate) fn effective(clean: &str, range: Range<usize>) -> usize {
    clean[range]
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

fn erase_comments(node: Node<'_>, bytes: &mut [u8]) {
    if node.kind() == "comment" {
        for byte in &mut bytes[node.byte_range()] {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        erase_comments(child, bytes);
    }
}
