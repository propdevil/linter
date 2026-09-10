# rust/redundant-marker

Finds private empty traits with no consumers and either no implementations or only unconstrained blanket implementations. Such traits establish no selective contract. Structs, tuple wrappers, and zero-sized typestate values are outside this rule.

```toml
[[rules."rust/redundant-marker"]]
target = "**/*.rs"
exclude = "generated/**"
scope = "production"
```

```rust
trait Forgotten {} // Error: no implementation or consumer.
trait Anything {} // Error: every T qualifies and nothing uses the trait.
impl<T> Anything for T {}

trait Selected {} // Allowed: selectively tags one nominal type.
struct Ready;
impl Selected for Ready {}

trait Required {} // Allowed: a consumer requires this capability.
fn require<T: Required>() {}
```

Preserves traits with visibility, attributes, safety/auto contracts, generic parameters, supertraits, or associated items. Concrete implementations, constrained blanket implementations (including where clauses), bounds, trait objects, and other resolved type references preserve the trait. Trait identities include the package and module; aliases resolve without conflating unrelated same-named traits or zero-sized nominal types. A macro mentioning the trait name in the same package conservatively preserves it because expansion is unavailable.

Target and optional exclude accept root-relative globs or nonempty lists. Scope supports production (default), tests, and all. Selection controls reported declarations; all discovered Rust sources supply consumer evidence, including excluded files and tests, so narrowing a target does not erase a real contract. Findings span the trait declaration and include blanket implementation evidence. Common reasoned disable directives apply.

Migrates `ceremony/marker.rs` from Husklet and both design-lint copies, retaining their unused/blanket positives and meaningful-contract negatives. Tree-sitter syntax and the shared declaration index replace the donor's name-only database. This static rule does not expand macros or prove runtime invariants; unknown macro uses and selective implementations are retained conservatively. Constrained blanket implementations are preserved even when their constraint appears in a where clause, correcting the donor's unconstrained classification.
