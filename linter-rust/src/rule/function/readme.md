# rust/free-function

Requires a proven receiver-shaped free function to move its cohesive behavior onto its existing local owner, or carry an exact reasoned exception.

```toml
[[rules."rust/free-function"]]
target = "**/*.rs"
mode = "receiver"
boundary_derives = ["clap::Parser", "clap::Args", "Parser", "Args"]
```

Receiver mode is the default and requires exactly one total parameter. Removing references must expose a direct, resolved struct, enum, or union declared in the same package. Primitive values, foreign types, generic parameters, slices, and `Vec<T>`/`Option<T>`/other wrapped arguments do not establish a receiver. Even one additional primitive argument leaves this rule's default scope.

```rust
struct Wallet { id: u64 }
fn active(wallet: &Wallet) -> bool { wallet.id != 0 }
```

This fails; `Wallet::active(&self)` expresses the existing owner. `fn compare(a: &Wallet, b: &Wallet)` and `fn select(values: &[Wallet])` pass because they operate over multiple values.

`mode = "classification"` restores the broader Prop/Payment-SDK policy: every free Rust function with one or two parameters needs an ownership decision, even with primitive or framework-shaped parameters. It does not invent a receiver when none is proven. Both modes exclude explicit foreign ABIs, proc-macro entrypoints, methods, and concrete struct factories handled by the detached-constructor rule. Factories require a resolved local return owner and construction evidence; merely returning an entity is not enough.

Target and optional exclude accept a glob or nonempty list. Scope defaults to production, with tests/all alternatives. `boundary_derives` is empty unless configured and matches exact written derive paths on the argument owner. In receiver mode this permits CLI boundary values without classifying every wrapped extractor as an entity.

Exact configured exceptions can be declared within a block:

```toml
exceptions = [
    { function = "crate::http::handle", reason = "The framework owns this extractor signature." },
]
```

A bare function name applies to that name within the targeted files; a `crate::module::function` path narrows it to its lexical owner. Reasons must be nonempty. Reasoned `linter:disable rust/free-function` comments also apply. Legacy `hl_design::classify` or `adapter` attributes do not suppress errors.

Findings carry function spans and conservatively resolved call/function-value/serde-hook evidence. Shadowed local names, attribute display strings, unresolved aliases, and opaque macro bodies are not fabricated as callers. Same-spelled functions in separate modules remain separate subjects. The existing AST and nominal declaration index are reused.

Migration preserves Husklet's single-owned-argument scope and replaces its name-based owner guesses with resolved identity. Optional classification mode preserves Prop/Payment-SDK one-/two-argument checks and current reasoned exceptions. CLI boundary vocabulary moves to configuration, and concrete factories remain owned by the separate constructor rule. Caller evidence is stricter than the old same-name search.
