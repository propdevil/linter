# rust/function-length

Limits each free Rust function, including nested functions, using the shared Rust AST. Methods, associated functions, trait default bodies, and declarations without bodies are outside this rule.

```toml
[[rules."rust/function-length"]]
target = "**/src/**/*.rs"
exclude = "generated/**/*.rs"
max_lines = 50
scope = "production"
```

`target` is required and accepts a pattern or nonempty list. `exclude` accepts the same syntax. `max_lines` defaults to 50 and must be positive. Unknown options fail configuration validation.

The count includes the declaration, multiline signature, closing brace, comments, blank lines, and closures. Attributes preceding the declaration are outside its span. Exactly the configured limit passes. Each finding identifies the function, starting line, measured count, and configuration block.

`scope` accepts `production` (default), `tests`, or `all`. Production excludes test-only functions, nested test-only items, and crate/global integration sources. Test-only classification reuses the file-length rule's conservative `cfg(test)` handling. Tests scope checks entire functions identified as test-only; all scope counts every physical line. Nested functions are checked independently and also contribute to their enclosing function.

```rust
fn payment() {
    authorize();
    submit();
}
```

This function passes `max_lines = 4` and fails `max_lines = 3`.

This is a new Rust rule; the existing C function-length implementation remains a separate migration. Comment suppression is supplied by the common directive layer when registered; this rule does not interpret its own suppression syntax.
