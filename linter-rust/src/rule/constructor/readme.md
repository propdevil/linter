# rust/self-constructor-static

Reports receiver-independent inherent factories that return `Self` but take
`self`, `&self`, `&mut self`, or a typed receiver. Candidate names are `new`,
`parse`, `from`, `try_from`, and their underscore-separated extensions. The return
must be `Self` or a generic type with `Self` as a direct argument, such as
`Result<Self, Error>`, `Option<Self>` or `Box<Self>`.

```toml
[[rules."rust/self-constructor-static"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
```

`target` is required; `exclude` optional. Both accept a pattern or nonempty list.
`scope` accepts `production` (default), `tests`, or `all`. Shared Rust syntax and
test classification are reused. Missing blocks are unconfigured.

```rust
impl Value {
    fn new(&self) -> Self { Self(0) } // Error: receiver contributes nothing.
    fn new() -> Self { Self(0) }      // Associated factory passes.
    fn from_parts(self) -> Self { Self(self.0) } // Receiver-dependent conversion passes.
    fn parse(self) -> Result<Self, Error> { Ok(self) } // Consuming operation passes.
}
```

Trait implementations/default contracts, associated functions without receivers,
non-constructor names, borrowed `&Self` results, and nested wrapper returns are
outside this check. Actual receiver references preserve conversions and builders,
including field access, mutation, aliasing and receiver tokens inside macros.
Comments, strings, similarly named locals and `self::module` paths do not count
as receiver references. Macro expansion/type aliases are not resolved.

Findings identify the method and unused receiver. Intentional compatibility
signatures may use a narrowly reasoned current directive; stale directives fail.

Migration: transfers Payment-SDK `rule/rust.rs`'s constructor detector and
`SelfConstructorStatic` wrapper. Preserves name/return shapes and test exclusions.
Narrows its receiver ban to proven receiver-independent factories: the donor
flagged identity conversions such as `parse(self) { self }`, which now pass
because the receiver participates. Suppression regression cases use actual unused
receivers so removing the defect correctly makes suppression stale.
