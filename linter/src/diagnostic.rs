use serde::Serialize;
use std::{ops::Range, path::PathBuf};

/// Byte range and one-based source position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(text: &str, range: Range<usize>) -> Self {
        let start = range.start.min(text.len());
        let prefix = &text.as_bytes()[..start];
        let line = prefix.iter().filter(|byte| **byte == b'\n').count() + 1;
        let column = prefix
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(start + 1, |newline| start - newline);
        Self {
            start,
            end: range.end.min(text.len()),
            line,
            column,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Evidence {
    pub path: PathBuf,
    pub span: Option<Span>,
    pub message: String,
}
