# max-indent

This is an absolute text constraint. For Rust function-relative indentation, use
`rust/max-indent`; Rust control-flow depth and guard clauses use `rust/nesting`.

```toml
[[rules."max-indent"]]
target = "**/*.{rs,c,h}"
exclude = "generated/**"
max_columns = 16
tab_width = 4
```

Checks leading whitespace on every nonblank physical line in selected UTF-8 regular files. Required `target` and optional `exclude` accept a root-relative glob or nonempty list. Positive limits default to 16 indentation columns and four-column tab stops. Exactly the maximum passes.

Each leading Unicode whitespace scalar counts one column, except tabs, which advance to the next tab stop. With four-column stops, `<SPACE><TAB>x` has four indentation columns and `<TAB><SPACE>x` has five. Counting stops at the first non-whitespace character. Blank and whitespace-only lines are ignored. LF and CRLF terminators do not count.

Continuation lines, comments, strings, and test code are checked like other physical lines. This is a text indentation constraint; AST control-flow nesting is handled by a separate language rule. No code is rewritten.

Each excessive indentation produces an error containing its one-based line number and measured indentation. Project exclusions apply, symlinks are skipped, and unmatched targets are allowed. Selected unreadable or invalid UTF-8 files cause execution errors. Blocks apply independently; no blocks means unconfigured.

This is a new language-neutral rule and does not replace the migrated early-return-aware nesting checks.
