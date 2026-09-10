# line-width

```toml
[[rules."line-width"]]
target = "**/*.{rs,c,h}"
exclude = "generated/**"
max_columns = 100
tab_width = 4
```

Checks every physical line in selected UTF-8 regular files. `target` is required and accepts a root-relative glob or nonempty list. Optional `exclude` has the same syntax. Both limits must be positive; their defaults are 100 columns and four-column tab stops.

Each Unicode scalar value occupies one column, regardless of terminal display width. Tabs advance to the next tab stop: with four-column stops, `a<TAB>` occupies four columns and `a<TAB>b` occupies five. LF and CRLF line terminators do not count. Trailing whitespace, comments, strings, and test code count normally. Exactly the maximum passes.

Each oversized line produces an error with its one-based line number and measured width. Empty files pass. Invalid UTF-8 or unreadable selected files are execution errors. Project exclusions apply, symlinks are skipped, and no matching input is acceptable. Blocks apply independently; omitting all blocks leaves the rule unconfigured.

This is a new language-neutral rule. It does not replace source file-length, function-length, or control-flow nesting checks. The checker reports findings without rewriting contents.
