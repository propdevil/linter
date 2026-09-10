# rust/method-length

Limits Rust functions declared directly inside inherent implementations, trait implementations, and traits. This includes constructors and associated functions without a receiver. Trait declarations without a body are skipped. Nested free functions belong to `rust/function-length`.

```toml
[[rules."rust/method-length"]]
target = "**/src/**/*.rs"
exclude = "generated/**/*.rs"
max_lines = 50
scope = "production"
```

`target` is required and accepts a glob or nonempty list; `exclude` accepts the same syntax. `max_lines` defaults to 50 and must be positive. Unknown configuration fields fail validation.

Count physical lines from the declaration through its closing brace, including multiline signatures, comments, blank lines, closures, and nested functions. Preceding attributes are outside the declaration span. Exactly the configured limit passes.

`scope` accepts `production` (default), `tests`, or `all`. Production excludes test-only methods, nested test-only items, and crate/global integration sources using the shared Rust scope analysis. Tests scope checks entire methods identified as test-only. All scope includes every physical line. Each finding names the method and its starting line, count, and configured limit.

```rust
impl Payment {
    fn submit(&self) {
        self.authorize();
        self.broadcast();
    }
}
```

The method passes `max_lines = 4` and fails `max_lines = 3`.

This is a new Rust rule; no source rule is retired. Comment directives belong to the shared suppression layer, not a separate method-length parser.
