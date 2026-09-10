# rust/string-backed-finite-state

```toml
[[rules."rust/string-backed-finite-state"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
min_variants = 3
state_words = ["state", "status", "phase", "mode", "kind", "stage", "condition", "lifecycle", "action"]
ignored_words = ["message", "name", "path", "id", "input", "raw"]
```

Reports a closed state vocabulary encoded as strings. `state_words` is required; vocabulary is never hidden in implementation constants. A name qualifies when its final underscore-separated word matches this list case-insensitively. `ignored_words` wins over state words and defaults to empty. Entries must be unique nonempty ASCII alphabetic words. Empty vocabulary matches nothing.

The default minimum is three distinct decoded string literals; `min_variants` must be at least two. Repeating the same literal does not increase the variant count. Beyond that threshold, the rule requires one of:

- A resolved string field with a comparison/match decision, or at least the configured number of literal assignments/constructions.
- A string local binding with both assignment and comparison/match evidence.
- Repeated literal calls to a `set_<state-word>` method on the same lexically resolved, nominally typed receiver binding.

```rust
struct Upload { status: String }
impl Upload {
    fn finished(&self) -> bool {
        match self.status.as_str() {
            "preparing" | "pushing" => false,
            "pushed" => true,
            _ => false,
        }
    }
}
// Error: status has three distinct state values used in a decision.
```

Fields use shared nominal declaration identity; String aliases resolve but unrelated newtypes do not become strings. Local bindings retain declaration identity through lexical scopes, and different receivers never share setter evidence. Field checks follow `self` and resolved struct initializers. Arbitrary foreign object fields and unresolved receiver types are not guessed.

Recognizes equality/inequality, match literals, string literal ownership conversions, and string constructors. Arbitrary calls containing a string argument do not establish string-state evidence. Strings in comments and opaque macros do not count. State-like names used only to inspect incoming values, without local evolution or persistent-field evidence, pass.

An unguarded fallback that directly returns the unknown value or carries it in Unknown/Unrecognized/Other/Raw/Custom representation preserves an open vocabulary and exempts the concept. Merely logging the unknown value before returning a fixed fallback does not preserve it. A protocol directory alone is not an exemption.

`target` is required and optional `exclude` accepts the same root-relative glob or nonempty-list syntax. Scope defaults to production; tests and all are available. Shared test classification recognizes test-only items and integration files. Project exclusions apply. Each error identifies its first literal and includes assignment/decision evidence locations and distinct values. Common reasoned directives apply; no blocks means unconfigured.

Migration preserves the ten source regressions from Husklet `rule/rust/state/{mod.rs,syntax.rs,test.rs}` and Payment-SDK `rule/adopted/state/{mod.rs,syntax.rs,tests.rs}`. Warnings become errors, vocabulary and thresholds become configuration, declaration identity replaces name-only grouping, and unknown-value evidence is tightened to actual preservation. The implementation reuses Tree-sitter syntax and the shared Rust declaration index.
