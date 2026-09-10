# rust/empty-struct

Rejects Rust structs with no fields: unit structs, empty named structs and empty
tuple structs. Comments do not count as fields. A real field, including a newtype
or `PhantomData` field, passes; this rule does not infer field meaning.

```toml
[[rules."rust/empty-struct"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
```

`target` is required; `exclude` optional. Both accept a pattern or nonempty list.
`scope` accepts `production` (default), `tests`, or `all`, using shared Rust
classification of test-only items and integration sources. No marker-name,
visibility or derive-based exemptions are built in. Missing blocks are unconfigured.

```rust
struct Empty;        // Error.
struct AlsoEmpty {}  // Error.
struct Tuple();      // Error.
struct Email(String); // Has a field; passes.

// linter:disable rust/empty-struct -- Marker distinguishes authorization in the type system.
struct Authorized;
```

Findings point at complete struct items. Current reasoned directives support
intentional markers; an obsolete directive becomes an error. Shared Rust analysis
supplies syntax once per run. Macro-generated structs are not expanded.

Migration: transfers Payment-SDK `rule/rust.rs`'s `ApiRule::EmptyStructs` and its
`EmptyStruct` registration wrapper. Preserves `fields.is_empty()` behavior and
production test exclusions, with explicit target/scope configuration and current
directives. This rule has no separate implementation in the other copied donors.
