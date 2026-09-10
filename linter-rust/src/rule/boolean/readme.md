# rust/boolean-state-cluster

Reports boolean fields coordinated as mutually exclusive state. Having several boolean fields alone is insufficient. Findings identify the model, implicated fields, and the constructions, transition blocks, or exclusion predicate supplying evidence.

```toml
[[rules."rust/boolean-state-cluster"]]
target = "**/src/**/*.rs"
exclude = "generated/**/*.rs"
min_fields = 3
scope = "production"
```

`min_fields` defaults to three, preserving the source policy. Two is an explicit opt-in; values below two are rejected. Target and optional exclude accept a glob or nonempty list. Scope accepts `production` (default), `tests`, and `all`. Targets select model definitions; other discovered source files can supply evidence within the same scope. Unknown fields fail configuration validation.

The rule recognizes three evidence forms:

- At least two complete constructions of the same resolved model select different single active flags. Every discovered construction must give all boolean fields literal values and exactly one true flag; an unknown, incomplete, or multi-active construction prevents this inference.
- A contiguous assignment sequence in one control-flow block sets at least the configured number of `self` boolean fields to exactly one true flag. Independent branches, closures, and nested functions are not merged into a fictional transition. Additional independent fields may remain outside the implicated cluster.
- Explicit negated conjunctions reject simultaneous activation across at least the configured number of fields and at least `min_fields - 1` distinct pairs. Bare positive conjunctions and double negations are not rejection evidence.

```rust
struct Session { idle: bool, opening: bool, active: bool }
impl Session {
    fn activate(&mut self) {
        self.idle = false;
        self.opening = false;
        self.active = true;
    }
}
```

This fails. An enum can represent the single session state without admitting contradictory combinations.

```rust
struct Permissions { readable: bool, writable: bool, executable: bool }
fn owner() -> Permissions {
    Permissions { readable: true, writable: true, executable: true }
}
```

This passes. All-false resets, individual feature toggles, bit-derived protocol flags, and one isolated construction also pass.

Resolution uses shared Rust declaration identities, preserving package/module/local ownership and explicit aliases. `Self` constructors and definitions appearing after their implementations are supported. Same-spelled models from different namespaces are not combined. Test-only construction and transition evidence is excluded in production scope. Reasoned comment directives apply to the reported struct.

Migration preserves all eleven distinct Husklet/Prop scenarios and both additional Payment-SDK cases. Findings are errors instead of source warnings. The former method-wide assignment aggregation is narrowed to actual contiguous blocks, and positive conjunctions no longer masquerade as invalid-state rejection. The existing source AST is reused without a second full-file parser.
