# rust/redundant-namespace

```toml
[[rules."rust/redundant-namespace"]]
target = "**/src/**/mod.rs"
exclude = "generated/**"
scope = "production"
```

Reports a private module file containing exactly one external child declaration and only transparent crate-visible re-exports of that child. The parent declaration and sole child implementation must both be discovered. No other source in the owning package may use or import the module namespace.

```rust
// shell/mod.rs, declared by a private `mod shell;`
mod process;
pub(crate) use process::{Child, Status};
// Candidate: process.rs can replace this transparent module wrapper.
```

Public/restricted parent modules, public/private re-exports instead of pub(crate), unrelated re-exports, additional declarations, multiple children, inline children, missing/ambiguous parents, and qualified namespace uses preserve the wrapper. Constructor structs, marker traits, and value wrappers are not namespace candidates.

Attributes and documented/conditional/platform/FFI/generated child boundaries are preserved conservatively. Macro-generated code is not analyzed as ordinary items. Qualified and imported namespace references are checked syntactically throughout the package; unresolved same-name references conservatively preserve the namespace rather than guessing they are irrelevant. Literal path overrides are outside this check's conventional module layout and remain untouched.

Required `target` and optional `exclude` accept root-relative globs or nonempty lists. Scope defaults to production and supports tests/all. Shared test classification and project exclusions apply. Findings identify the child declaration and include parent/child locations. Use a narrow exclusion or common reasoned directive for an explicit compatibility boundary. Unknown configuration fails; no blocks means unconfigured.

Migration preserves the namespace subcheck from Husklet `rule/rust/ceremony/namespace.rs` and corresponding Prop/Payment-SDK ceremony modules. Warnings become errors. The transferred tests cover transparent wrappers and public, used, configured, privacy, FFI, generation, and documentation boundaries. Marker and forwarding-wrapper checks remain separate rules; the source ceremony dispatcher and combined tests cannot be deleted until those checks migrate. Analysis reuses shared Tree-sitter syntax.
