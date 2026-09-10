use proc_macro2::{LineColumn, TokenStream, TokenTree};
use quote::ToTokens;

/// Read allow directives only from comments, never literal contents.
pub(super) fn collect(text: &str, syntax: &syn::File) -> Vec<(usize, String)> {
    let mut visible = text.as_bytes().to_vec();
    mask_literals(syntax.to_token_stream(), text, &mut visible);
    String::from_utf8_lossy(&visible)
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let (_, comment) = line.split_once("//")?;
            let directive = comment.trim().strip_prefix("design-lint: allow ")?;
            let (id, reason) = directive.split_once(" -- ")?;
            (!id.is_empty() && !id.chars().any(char::is_whitespace) && !reason.trim().is_empty())
                .then(|| (index + 1, id.to_owned()))
        })
        .collect()
}

fn mask_literals(tokens: TokenStream, text: &str, output: &mut [u8]) {
    for token in tokens {
        match token {
            TokenTree::Group(group) => mask_literals(group.stream(), text, output),
            TokenTree::Literal(literal) => {
                let span = literal.span();
                if let (Some(start), Some(end)) = (
                    byte_offset(text, span.start()),
                    byte_offset(text, span.end()),
                ) {
                    for byte in &mut output[start..end] {
                        if *byte != b'\n' && *byte != b'\r' {
                            *byte = b' ';
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
fn byte_offset(text: &str, location: LineColumn) -> Option<usize> {
    let mut start = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if index + 1 == location.line {
            return line
                .char_indices()
                .map(|(offset, _)| offset)
                .chain(std::iter::once(line.len()))
                .nth(location.column)
                .map(|offset| start + offset);
        }
        start += line.len();
    }
    None
}
