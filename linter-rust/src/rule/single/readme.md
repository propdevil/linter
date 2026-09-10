# Single-use free functions

Reports private free functions with exactly one resolved reference outside their
own body. Function values count as references. Multiple uses and recursion alone
pass. Public/restricted functions, `main`, extern functions, attributed framework
callbacks, and proven local struct constructors are excluded.

```toml
[[rules."rust/single-use-free-function"]]
target = "**/*.rs"
scope = "production"
```

`exclude` accepts a path selector. `scope` can be `production`, `tests`, or `all`;
production ignores test-only declarations and references, including integration
sources. Targets select declarations; reference counting scans the whole analysis.
Unknown options and invalid selectors fail configuration validation.

```rust
fn normalize() {}
fn run() { normalize(); } // One use: consider inlining normalize.
```

```rust
fn normalize() {}
fn run() { normalize(); normalize(); } // Two uses: allowed.
```

Resolution uses package, module, and lexical function identity. Parameters and
preceding local bindings shadow free functions. Sibling modules do not share an
unqualified namespace. A function passed to an ordinary callback API still counts;
a custom attribute is evidence of an externally owned callback contract.

This is conservative source analysis, not macro expansion or compiler name
resolution. Matching import aliases, wildcard imports, opaque macro/attribute
mentions, and matching names inside complex binding scopes prevent an exact-one
claim. Generic function references likewise defer rather than assume a count.
Recursive references are excluded. Legacy visual-section comments are not
exceptions; use a reasoned `// linter:disable rust/single-use-free-function -- ...`
directive directly before a deliberate semantic boundary.
