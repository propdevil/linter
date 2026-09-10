use super::recovery::{defined_macro_invocation, identifier};
pub(super) fn normalize_directives_inside_parentheses(source: &str) -> String {
    let mut normalized = source.as_bytes().to_vec();
    let mut offset = 0;
    let mut scan = ParenthesisScan::default();
    for line in source.split_inclusive('\n') {
        let text = line.trim_start();
        if text.starts_with('#') && !scan.block_comment {
            if scan.depth > 0 {
                let start = offset + line.len() - text.len();
                let end = offset + line.trim_end_matches(['\r', '\n']).len();
                normalized[start..end].fill(b' ');
            }
        } else {
            scan.advance(line);
        }
        offset += line.len();
    }
    String::from_utf8(normalized).expect("normalization preserves UTF-8")
}

/// Running lexical state of the parenthesis scan.
#[derive(Default)]
struct ParenthesisScan {
    depth: usize,
    block_comment: bool,
}

impl ParenthesisScan {
    /// Advances the scan across one line, ignoring comments, strings, and character literals.
    fn advance(&mut self, line: &str) {
        let mut characters = line.chars().peekable();
        while let Some(character) = characters.next() {
            if self.block_comment {
                if character == '*' && characters.peek() == Some(&'/') {
                    characters.next();
                    self.block_comment = false;
                }
                continue;
            }
            match character {
                '/' if characters.peek() == Some(&'/') => return,
                '/' if characters.peek() == Some(&'*') => {
                    characters.next();
                    self.block_comment = true;
                }
                '"' | '\'' => {
                    let quote = character;
                    while let Some(inner) = characters.next() {
                        match inner {
                            '\\' => {
                                characters.next();
                            }
                            _ if inner == quote => break,
                            _ => {}
                        }
                    }
                }
                '(' => self.depth += 1,
                ')' => self.depth = self.depth.saturating_sub(1),
                _ => {}
            }
        }
    }
}

/// Erases comments inside a continued `#define` so its body stays one preprocessor token.
///
/// The preprocessor replaces a comment with whitespace before it forms the macro body, but
/// the grammar ends the `preproc_arg` token at the comment instead. Every later line of the
/// definition then parses as ordinary top-level C, so an unfinished `do { ... } while (0)`
/// runs on and swallows whatever declaration follows the macro -- which is reported against
/// the declaration, far from the comment that caused it. Erasing the comment in place, at
/// the same offset and the same length, restores the body the grammar is meant to see.
pub(super) fn normalize_macro_body_comments(source: &str) -> String {
    let mut normalized = source.as_bytes().to_vec();
    let mut offset = 0;
    let mut body = MacroBodyScan::default();
    for line in source.split_inclusive('\n') {
        body.advance(line.trim_end_matches(['\r', '\n']), offset, &mut normalized);
        offset += line.len();
    }
    String::from_utf8(normalized).expect("normalization preserves UTF-8")
}

/// Running lexical state of the macro-body comment scan.
#[derive(Default)]
struct MacroBodyScan {
    inside: bool,
    block_comment: bool,
}

impl MacroBodyScan {
    /// Erases comment bytes on one line, entering and leaving a continued definition.
    fn advance(&mut self, line: &str, offset: usize, normalized: &mut [u8]) {
        let continued = line.trim_end().ends_with('\\');
        if !self.inside {
            self.inside = continued && line.trim_start().starts_with("#define");
            if !self.inside {
                return;
            }
        }
        self.erase(line, offset, normalized);
        if !continued {
            self.inside = false;
            self.block_comment = false;
        }
    }

    fn erase(&mut self, line: &str, offset: usize, normalized: &mut [u8]) {
        let bytes = line.as_bytes();
        let mut start = 0;
        let mut index = 0;
        while index < bytes.len() {
            if self.block_comment {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    index += 2;
                    self.block_comment = false;
                    normalized[offset + start..offset + index].fill(b' ');
                    continue;
                }
                index += 1;
                continue;
            }
            match bytes[index] {
                b'/' if bytes.get(index + 1) == Some(&b'*') => {
                    start = index;
                    self.block_comment = true;
                    index += 2;
                }
                // A line comment would swallow the continuation marker with the rest of the
                // line, so the marker is kept and the definition still reaches its last line.
                b'/' if bytes.get(index + 1) == Some(&b'/') => {
                    let stop = line.trim_end();
                    let stop = if stop.ends_with('\\') {
                        stop.len() - 1
                    } else {
                        bytes.len()
                    };
                    normalized[offset + index..offset + stop.max(index)].fill(b' ');
                    return;
                }
                b'"' | b'\'' => index = literal_end(bytes, index),
                _ => index += 1,
            }
        }
        if self.block_comment {
            normalized[offset + start..offset + bytes.len()].fill(b' ');
        }
    }
}

/// Returns the offset just past a string or character literal opened at `index`.
pub(super) fn literal_end(bytes: &[u8], index: usize) -> usize {
    let quote = bytes[index];
    let mut cursor = index + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor += 2,
            byte if byte == quote => return cursor + 1,
            _ => cursor += 1,
        }
    }
    cursor
}

pub(super) fn normalize_declared_macro_lines(original: &str, source: &str) -> String {
    let mut normalized = source.as_bytes().to_vec();
    let mut offset = 0;
    for line in original.split_inclusive('\n') {
        let text = line.trim();
        if defined_macro_invocation(text, original) {
            let start = offset + line.len() - line.trim_start().len();
            let end = offset + line.trim_end_matches(['\r', '\n']).len();
            normalized[start..end].fill(b' ');
            normalized[start] = b';';
        }
        offset += line.len();
    }
    String::from_utf8(normalized).expect("normalization preserves UTF-8")
}

pub(super) fn normalize_named_registers(source: &str) -> String {
    let mut normalized = source.as_bytes().to_vec();
    for spelling in ["__asm__(\"", "asm(\""] {
        let mut offset = 0;
        while let Some(relative) = source[offset..].find(spelling) {
            let start = offset + relative;
            let line_start = source[..start].rfind('\n').map_or(0, |line| line + 1);
            if !source[line_start..start].contains("register ") {
                offset = start + spelling.len();
                continue;
            }
            let Some(close) = source[start..].find("\")").map(|close| start + close + 2) else {
                break;
            };
            normalized[start..close].fill(b' ');
            offset = close;
        }
    }
    String::from_utf8(normalized).expect("normalization preserves UTF-8")
}

pub(super) fn normalize_va_arg_types(source: &str) -> String {
    let mut normalized = source.as_bytes().to_vec();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("va_arg(") {
        let start = offset + relative + "va_arg(".len();
        let Some(comma) = source[start..].find(',').map(|comma| start + comma) else {
            break;
        };
        let Some(close) = source[comma..].find(')').map(|close| comma + close) else {
            break;
        };
        for byte in &mut normalized[comma + 1..close] {
            if *byte == b'*' {
                *byte = b' ';
            }
        }
        offset = close + 1;
    }
    String::from_utf8(normalized).expect("normalization preserves UTF-8")
}

pub(super) fn normalize_offsetof_designators(source: &str) -> String {
    let mut normalized = source.as_bytes().to_vec();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("offsetof(") {
        let start = offset + relative + "offsetof(".len();
        let Some(comma) = source[start..].find(',').map(|comma| start + comma) else {
            break;
        };
        let Some(close) = source[comma..].find(')').map(|close| comma + close) else {
            break;
        };
        for byte in &mut normalized[comma + 1..close] {
            if *byte == b'.' {
                *byte = b'_';
            }
        }
        offset = close + 1;
    }
    String::from_utf8(normalized).expect("normalization preserves UTF-8")
}

pub(super) fn normalize_computed_goto(source: &str) -> String {
    let mut normalized = source.as_bytes().to_vec();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if let Some(relative) = line.find("goto *") {
            let start = offset + relative + "goto ".len();
            if let Some(end) = line[relative..].find(';') {
                normalized[start..offset + relative + end].fill(b' ');
                normalized[start] = b'L';
            }
        }
        offset += line.len();
    }
    String::from_utf8(normalized).expect("normalization preserves UTF-8")
}

pub(super) fn normalize_complex_macro(source: &str) -> String {
    ["float complex", "double complex", "long double complex"]
        .into_iter()
        .fold(source.to_owned(), |source, spelling| {
            source.replace(spelling, &spelling.replace("complex", "       "))
        })
}

pub(super) fn normalize_gnu_attributes(source: &str) -> String {
    let mut normalized = source.as_bytes().to_vec();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("__attribute__") {
        let start = offset + relative;
        let mut cursor = start + "__attribute__".len();
        while source
            .as_bytes()
            .get(cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            cursor += 1;
        }
        if source.as_bytes().get(cursor) != Some(&b'(') {
            offset = cursor;
            continue;
        }
        let mut depth = 0usize;
        while cursor < source.len() {
            match source.as_bytes()[cursor] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        cursor += 1;
                        normalized[start..cursor].fill(b' ');
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        if depth != 0 {
            break;
        }
        offset = cursor;
    }
    String::from_utf8(normalized).expect("normalization preserves UTF-8")
}

pub(super) fn normalize_atomic_specifiers(source: &str) -> String {
    let mut normalized = source.as_bytes().to_vec();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("_Atomic(") {
        let open = offset + relative + "_Atomic".len();
        let mut depth = 1usize;
        let mut cursor = open + 1;
        while cursor < source.len() && depth != 0 {
            match source.as_bytes()[cursor] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            cursor += 1;
        }
        if depth != 0 {
            break;
        }
        normalized[open] = b' ';
        normalized[cursor - 1] = b' ';
        offset = cursor;
    }
    String::from_utf8(normalized).expect("normalization preserves UTF-8")
}

pub(super) fn normalize_function_pointer_annotations(source: &str) -> String {
    let mut normalized = source.as_bytes().to_vec();
    let mut start = 0;
    while start < source.len() {
        if !source.as_bytes()[start].is_ascii_uppercase() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < source.len()
            && matches!(source.as_bytes()[end], b'_' | b'0'..=b'9' | b'A'..=b'Z')
        {
            end += 1;
        }
        let before = source.as_bytes()[..start]
            .iter()
            .rfind(|byte| !byte.is_ascii_whitespace());
        let tail = source[end..].trim_start();
        let pointer = tail
            .strip_prefix('*')
            .map(str::trim_start)
            .and_then(|tail| {
                let name_end = tail.find(|character: char| {
                    !character.is_ascii_alphanumeric() && character != '_'
                })?;
                identifier(&tail[..name_end]).then_some(tail[name_end..].trim_start())
            });
        if before == Some(&b'(') && pointer.is_some_and(|tail| tail.starts_with(")(")) {
            normalized[start..end].fill(b' ');
        }
        start = end;
    }
    String::from_utf8(normalized).expect("normalization preserves UTF-8")
}
