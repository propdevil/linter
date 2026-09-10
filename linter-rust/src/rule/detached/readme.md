# rust/detached-constructor

Reports free factories whose final expression constructs and returns one resolved
local struct or enum. Such factories belong as associated constructors on that
type. Findings link the factory, owner declaration and returned construction.

```toml
[[rules."rust/detached-constructor"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
```

`target` is required; `exclude` is optional. Both accept one root-relative pattern
or a nonempty list. `scope` accepts `production` (default), `tests`, or `all`.
Missing blocks are unconfigured. Shared syntax and the declaration index resolve
local identities and aliases; matching the last type-name word alone is insufficient.

```rust
struct Lease { value: usize }
fn open() -> Result<Lease, Error> { Ok(Lease { value: 1 }) } // Error: Lease::open.
impl Lease {
    fn open() -> Result<Self, Error> { Ok(Self { value: 1 }) } // Already owned.
}
```

Direct return types and nested standard `Option`/`Result` wrappers are supported.
Direct construction, parenthesized/block expressions, explicit returns and
same-owner factory calls provide evidence. Standard conversion methods such as
`from`/`try_from` are excluded. Prior construction of another local entity prevents
a single-owner claim. Parameters carrying a local entity also preserve potential
transformations rather than treating them as primitive-input factories.

Generic functions, dynamic/opaque/borrowed returns, external or ambiguous types,
forwarding another owner's factory, nested function bodies and trait/impl methods
are outside this check. Rust `main`, foreign-ABI functions and functions carrying
unknown framework attributes retain their required entrypoint shape. Explicit
value bindings cannot impersonate type constructors. Macro expansion, arbitrary
alias/function-pointer flow and complete compiler type inference are not claimed.

```rust
// linter:disable rust/detached-constructor -- Framework requires this free factory signature.
fn open() -> Lease { Lease { value: 1 } }
```

Migration: transfers Husklet `rule/rust/constructor/mod.rs` and its four regression
cases. Preserves concrete/wrapped construction, associated ownership, uncertain
ownership, orchestration and conversion exceptions. Extends resolved declaration
identity, target/scope controls, entrypoint/transform safeguards, source evidence
and current reasoned directives. Unknown ownership is skipped conservatively.
