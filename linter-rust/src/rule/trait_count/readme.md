# rust/trait-method-count

Limits directly declared Rust trait functions, including default implementations and associated functions without a receiver. Associated types, constants, inherited supertrait methods, and implementation methods are not counted.

```toml
[[rules."rust/trait-method-count"]]
target = "**/src/**/*.rs"
exclude = "generated/**/*.rs"
max_methods = 3
scope = "production"
```

Target is required and accepts a glob or nonempty list; exclude accepts the same syntax. `max_methods` defaults to three and must be positive. Exactly the limit passes. Scope accepts `production` (default), `tests`, or `all`. Production excludes test-only traits and methods using the shared Rust scope analysis. Tests scope selects test-only traits; all scope counts both production and test methods. Unknown fields fail configuration validation.

```rust
trait Reader {
    type Value;
    fn read(&self) -> Self::Value;
    fn ready(&self) -> bool;
}
```

This trait has two methods and passes the default limit. A trait declaring four methods fails even when all four have default bodies. Generated methods inside unexpanded macros are not invented by the analysis.

Each error includes the trait span and evidence spans for every counted method. A reasoned directive attaches to the trait. The implementation reuses the globally parsed Rust AST.

Migration preserves Payment-SDK's strict `trait-method-count` gate and its decorated-trait regression. Method evidence is consistently attached rather than only when the separate broad-responsibility detector also fires. The broad trait responsibility rules in Husklet, Prop, and Payment-SDK remain independent migrations; this rule does not claim their capability clustering behavior.
