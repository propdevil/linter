# rust/file-length

Limit physical production lines in Rust files while excluding test code. Enabled by default with a maximum of 500 lines.

```toml
[[rules."rust/file-length"]]
target = "**/*.rs"
target = "**/*.rs"
max_lines = 600
```

A file with 500 production lines and 500 inline test lines passes this limit, despite being 1,000 lines long. Exactly 600 production lines also passes; 601 produces a finding. The limit must be a positive integer.

Excluded code:

- Items and modules guarded by `#[cfg(test)]`, including their attributes and nested contents.
- Direct functions marked `#[test]`.
- Scopes with an inner `#![cfg(test)]` attribute.
- Root `tests/`, Cargo packages' top-level `tests/`, and explicitly declared Cargo test entry files.

```rust
pub fn calculate() -> u64 {
    42
}

#[cfg(test)]
mod tests {
    #[test]
    fn calculates() {
        assert_eq!(super::calculate(), 42);
    }
}
```

Only the production function and the blank separator count here. Production comments, blank lines, structs, enums, constants, and macros still count. A line shared by production and test code counts as production. Blank lines inside an excluded test item do not count. LF and CRLF line endings work the same way.

The rule uses shared Tree-sitter syntax trees for item boundaries. Syn parses only attribute metadata, not source files. Conditional predicates are handled conservatively: `cfg(all(test, feature = "extra"))` is test-only, while `cfg(any(test, feature = "extra"))` can contain production code and is counted. `cfg(not(test))` is production. Attribute-looking strings and comments do not suppress code.

Macro expansion and cross-file test-module reachability are not resolved. Custom test attributes and `cfg_attr` remain counted unless enclosed in a recognized test-only scope. Integration sources are still parsed by shared Rust analysis, so syntax errors remain errors.

Findings report production lines, total physical lines, and the configured maximum. Split excess production code by responsibility; do not use `include!` or numbered fragments to evade the limit. Test code is already excluded.

Each block requires `target` and accepts optional `exclude`. The default maximum is
500 production lines. Multiple blocks apply independently. Without blocks, the
rule is unconfigured. Test code is always excluded from this production budget.
