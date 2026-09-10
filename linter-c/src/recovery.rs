use tree_sitter::Parser;
pub(super) fn first_unrecoverable_error<'tree>(
    node: tree_sitter::Node<'tree>,
    source: &str,
) -> Option<tree_sitter::Node<'tree>> {
    if node.is_error() || node.is_missing() {
        if macro_continuation(node, source)
            || terminal_macro_before_declaration(node, source)
            || balanced_source_closing_brace(node, source)
            || line_macro_invocation(node, source)
            || conditional_statement_directive(node, source)
            || annotation_prefix(node, source)
            || builtin_offsetof_type_argument(node, source)
            || va_arg_type_argument(node, source)
            || declared_identifier_macro(node, source)
            || enclosing_macro_invocation(node, source)
        {
            return None;
        }
        return Some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find_map(|child| first_unrecoverable_error(child, source))
}

pub(super) fn balanced_source_closing_brace(node: tree_sitter::Node<'_>, source: &str) -> bool {
    node.kind() == "}"
        && node.is_missing()
        && source.bytes().fold(0isize, |depth, byte| match byte {
            b'{' => depth + 1,
            b'}' => depth - 1,
            _ => depth,
        }) == 0
}

pub(super) fn declared_identifier_macro(node: tree_sitter::Node<'_>, source: &str) -> bool {
    if node.kind() != "ERROR" {
        return false;
    }
    let Ok(name) = node.utf8_text(source.as_bytes()) else {
        return false;
    };
    identifier(name)
        && source.lines().any(|line| {
            line.trim_start()
                .strip_prefix("#define")
                .and_then(|definition| definition.split_whitespace().next())
                == Some(name)
        })
        && node.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                "asm_statement" | "gnu_asm_expression" | "argument_list"
            )
        })
}

pub(super) fn conditional_statement_directive(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let row = node.start_position().row;
    let lines = source.lines().collect::<Vec<_>>();
    if conditional_else(node, &lines, row) {
        return true;
    }
    let window = &lines[row.saturating_sub(16)..row.min(lines.len())];
    if window
        .iter()
        .any(|line| line.trim_start().starts_with("#endif"))
        && window
            .iter()
            .any(|line| line.trim_start().starts_with("#else"))
        && window.iter().any(|line| {
            matches!(line.trim_start(), line if line.starts_with("#if ") || line.starts_with("\
                #ifdef ") || line.starts_with("\
                #ifndef "))
        })
        && window
            .iter()
            .any(|line| line.trim_start().starts_with("if ("))
    {
        return true;
    }
    if node.is_error() && row > 0 && lines[row].trim_start().starts_with("else ") {
        return lines[..row]
            .iter()
            .rev()
            .take_while(|line| !line.trim_start().starts_with("if ("))
            .any(|line| line.trim_start().starts_with("#endif"));
    }
    if node.kind() != ";" || !node.is_missing() {
        return false;
    }
    if row == 0 || row > lines.len() {
        return false;
    }
    let directive_row = if row < lines.len() && lines[row].trim_start().starts_with('#') {
        row
    } else if row + 1 < lines.len() && lines[row + 1].trim_start().starts_with('#') {
        row + 1
    } else {
        return false;
    };
    let previous = lines[directive_row - 1].trim_end();
    if !previous.ends_with(')') || !previous.trim_start().starts_with("if (") {
        return false;
    }
    conditional_body(&lines[directive_row..])
}

fn conditional_else(node: tree_sitter::Node<'_>, lines: &[&str], row: usize) -> bool {
    if node.kind() == ";"
        && node.is_missing()
        && lines
            .get(row)
            .is_some_and(|line| line.trim_start().starts_with("else"))
        && row > 0
        && lines[row - 1].trim_start().starts_with("#endif")
    {
        let branch = lines[..row - 1]
            .iter()
            .rev()
            .take_while(|line| {
                let line = line.trim_start();
                !conditional_start(line)
            })
            .any(|line| line.trim_start().starts_with("else if ("));
        let directive = lines[..row - 1].iter().rev().find(|line| {
            let line = line.trim_start();
            line.starts_with("#if ") || line.starts_with("#ifdef ") || line.starts_with("#ifndef ")
        });
        if branch && directive.is_some() {
            return true;
        }
    }
    false
}
fn conditional_body(lines: &[&str]) -> bool {
    let mut depth = 0usize;
    let mut branch_statement = false;
    for line in lines {
        let line = line.trim();
        if line.starts_with("#if ") || line.starts_with("#ifdef ") || line.starts_with("#ifndef ") {
            depth += 1;
        } else if line.starts_with("#endif") {
            let Some(next) = depth.checked_sub(1) else {
                return false;
            };
            depth = next;
            if depth == 0 {
                return branch_statement;
            }
        } else if depth == 1 && !line.is_empty() && !line.starts_with('#') {
            branch_statement = line.ends_with(';') || line.starts_with('{');
        }
    }
    false
}
pub(super) fn builtin_offsetof_type_argument(
    mut node: tree_sitter::Node<'_>,
    source: &str,
) -> bool {
    loop {
        if node.kind() == "call_expression"
            && node
                .child_by_field_name("function")
                .and_then(|function| function.utf8_text(source.as_bytes()).ok())
                == Some("__builtin_offsetof")
        {
            return true;
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        node = parent;
    }
}

pub(super) fn va_arg_type_argument(mut node: tree_sitter::Node<'_>, source: &str) -> bool {
    loop {
        if node.kind() == "call_expression"
            && node
                .child_by_field_name("function")
                .and_then(|function| function.utf8_text(source.as_bytes()).ok())
                == Some("va_arg")
            && node
                .utf8_text(source.as_bytes())
                .ok()
                .is_some_and(balanced_parentheses)
        {
            return true;
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        node = parent;
    }
}

pub(super) fn line_macro_invocation(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let lines = source.lines().collect::<Vec<_>>();
    let row = node.start_position().row;
    (row.saturating_sub(8)..=row).any(|start| {
        let invocation = lines[start..=row.min(lines.len().saturating_sub(1))].join(" ");
        defined_macro_invocation(invocation.trim(), source)
            || declared_macro_within(&invocation, source)
    })
}

pub(super) fn declared_macro_within(text: &str, source: &str) -> bool {
    source
        .lines()
        .filter_map(|line| {
            let definition = line.trim_start().strip_prefix("#define")?.trim_start();
            let open = definition.find('(')?;
            identifier(definition[..open].trim()).then_some(definition[..open].trim())
        })
        .any(|name| {
            text.find(&format!("{name}("))
                .is_some_and(|start| has_balanced_parenthesized_prefix(&text[start + name.len()..]))
        })
}

pub(super) fn has_balanced_parenthesized_prefix(text: &str) -> bool {
    let mut depth = 0usize;
    for byte in text.bytes() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
                if depth == 0 {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub(super) fn annotation_prefix(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let point = node.start_position();
    let Some(line) = source.lines().nth(point.row) else {
        return false;
    };
    let Some(before) = line.get(..point.column).map(str::trim) else {
        return false;
    };
    let embedded = node.utf8_text(source.as_bytes()).ok().filter(|token| {
        let after = line.get(node.end_position().column..).unwrap_or_default();
        !before.is_empty()
            && after
                .trim_start()
                .split_once('(')
                .is_some_and(|(name, _)| identifier(name.trim()))
            && macro_identifier(token)
    });
    let leading = before
        .split_whitespace()
        .next_back()
        .filter(|token| macro_identifier(token))
        .filter(|_| {
            line.get(point.column..)
                .is_some_and(|tail| tail.contains('('))
        });
    (embedded.is_some() || leading.is_some()) && declaration_ancestor(node)
}

pub(super) fn declaration_ancestor(mut node: tree_sitter::Node<'_>) -> bool {
    while let Some(parent) = node.parent() {
        if matches!(
            parent.kind(),
            "function_definition" | "declaration" | "type_definition"
        ) {
            return true;
        }
        node = parent;
    }
    false
}

pub(super) fn macro_identifier(token: &str) -> bool {
    token.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && token
            .bytes()
            .all(|byte| byte == b'_' || byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

pub(super) fn identifier(token: &str) -> bool {
    token
        .as_bytes()
        .first()
        .is_some_and(|byte| *byte == b'_' || byte.is_ascii_alphabetic())
        && token
            .bytes()
            .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

pub(super) fn macro_continuation(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let lines = source.lines().collect::<Vec<_>>();
    // A token the grammar reports as missing sits at the position it should have
    // occupied, which for an unterminated construct is one row past the last line.
    let Some(mut row) = lines
        .len()
        .checked_sub(1)
        .map(|last| node.start_position().row.min(last))
    else {
        return false;
    };
    let mut first = true;
    loop {
        let line = lines[row].trim_end();
        if first && line.trim().is_empty() {
            if row == 0 {
                return false;
            }
            row -= 1;
            continue;
        }
        if line.trim_start().starts_with("#define ") {
            return true;
        }
        if row == 0 {
            return false;
        }
        if !first && !line.ends_with('\\') {
            return false;
        }
        first = false;
        row -= 1;
    }
}

pub(super) fn terminal_macro_before_declaration(node: tree_sitter::Node<'_>, source: &str) -> bool {
    if node.kind() != ";" || !node.is_missing() {
        return false;
    }
    let lines = source.lines().collect::<Vec<_>>();
    let row = node.start_position().row;
    if !lines.get(row).is_some_and(|line| {
        let line = line.trim_start();
        (line.contains('(') && line.contains('{'))
            || line.starts_with("/*")
            || line.starts_with("//")
    }) {
        return false;
    }
    let Some(mut row) = lines[..row]
        .iter()
        .rposition(|line| !line.trim().is_empty())
    else {
        return false;
    };
    if lines[row].trim_end().ends_with('\\') {
        return false;
    }
    while row > 0 {
        row -= 1;
        if lines[row].trim_start().starts_with("#define ") {
            return true;
        }
        if !lines[row].trim_end().ends_with('\\') {
            return false;
        }
    }
    false
}

pub(super) fn enclosing_macro_invocation(mut node: tree_sitter::Node<'_>, source: &str) -> bool {
    loop {
        if node.kind().starts_with("preproc_") {
            return true;
        }
        if recoverable_macro_invocation(node, source) {
            return true;
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        node = parent;
    }
}

pub(super) fn recoverable_macro_invocation(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let context = parent.kind();
    if context != "translation_unit"
        && !(context == "compound_statement" && node.kind() == "expression_statement")
    {
        return false;
    }
    let Ok(text) = node.utf8_text(source.as_bytes()) else {
        return false;
    };
    let mut offset = 0;
    let mut invocation = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            offset += line.len();
            continue;
        }
        if defined_macro_invocation(trimmed, source) {
            invocation = true;
            offset += line.len();
            continue;
        }
        break;
    }
    invocation && (offset == text.len() || parses_without_recovery(&text[offset..]))
}

pub(super) fn parses_without_recovery(source: &str) -> bool {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_c::LANGUAGE.into()).is_ok()
        && parser
            .parse(source, None)
            .is_some_and(|tree| !tree.root_node().has_error())
}

pub(super) fn defined_macro_invocation(text: &str, source: &str) -> bool {
    let text = text.trim().trim_end_matches(';').trim_end();
    let Some(open) = text.find('(') else {
        return false;
    };
    let name = text[..open].trim();
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
        || name.as_bytes()[0].is_ascii_digit()
        || !balanced_parentheses(&text[open..])
    {
        return false;
    }
    source.lines().any(|line| {
        let line = line.trim_start();
        line.strip_prefix("#define")
            .is_some_and(|definition| definition.trim_start().starts_with(&format!("{name}(")))
    })
}

pub(super) fn balanced_parentheses(text: &str) -> bool {
    let mut depth = 0usize;
    let length = text.len();
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            _ => {}
        }
        if depth == 0 && index + 1 != length {
            // A matching close before the end means trailing syntax was recovered too.
            return false;
        }
    }
    depth == 0
}

fn conditional_start(line: &str) -> bool {
    ["#if ", "#ifdef ", "#ifndef "]
        .iter()
        .any(|prefix| line.starts_with(prefix))
}
