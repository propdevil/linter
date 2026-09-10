# rust/wire-domain-model-duplication

```toml
[[rules."rust/wire-domain-model-duplication"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
min_shared_fields = 3
min_overlap_percent = 75
```

Reports a serialized model duplicating a related owned model. Each finding is an error with both declaration locations, every matching field pair, and any resolved field-copy conversion. Reuse or compose the owner unless a concrete boundary contract requires separate representation.

A wire candidate has serialization attributes and at least three public named fields. Its owner has at least three private named fields and inherent behavior. Two wire models can also qualify across packages joined by a direct local Cargo dependency. Sharing a directory or arbitrary primitive fields does not establish package ownership. Targets select the candidate to report; owner evidence may come from another discovered file. Both models must satisfy the configured scope, which defaults to production and also accepts tests or all.

Models must share a concept or a resolved standard From/TryFrom field-copy conversion. Concept comparison splits complete identifier words and removes Wire, Api, Dto, Model, and Data tokens; it never removes arbitrary substrings. Image and WireImage relate, but Image and WireImagery do not. A conversion containing calls, arithmetic, validation, or other statements is conservatively treated as meaningful transformation and exempts the pair. Shadowed custom From traits are not conversion evidence.

At least three matching named fields and 75% overlap relative to the smaller model are required by default. `min_shared_fields` cannot be below three; `min_overlap_percent` must be 1–100. Threshold equality passes comparison and produces a finding. Field identity uses serialized field renames and resolved written types. Local aliases resolve; nominal wrappers retain distinct identities. Unresolved or ambiguous types do not contribute matching evidence.

```rust
struct Email(String);
struct WalletId(String);
// These are separate nominal values, not duplicated models.
```

Tuple structs and models with fewer than three named fields never participate. ABI representations, platform-gated representations, and Request/Response/View/Summary/Snapshot/Event/Command projections are excluded. These exclusions are conservative: this rule does not infer equivalent compiler configurations or semantic transformations from arbitrary Rust code.

```rust
#[derive(serde::Serialize)]
struct WireImage { pub id: u64, pub name: String, pub path: String }
struct Image { id: u64, name: String, path: String }
impl Image { fn validate(&self) {} }
// Error: same concept, wire/owner evidence, three identical field types.
```

`target` is required and optional `exclude` uses the same root-relative glob or nonempty-list syntax. Unknown settings, invalid selectors, and invalid thresholds fail configuration. Project exclusions and common reasoned directives apply. No blocks means unconfigured. Shared Tree-sitter syntax and the Rust nominal declaration index supply analysis; the rule does not parse whole files again.

Migration preserves model comparison scenarios from Husklet `rule/rust/model/{mod.rs,test.rs}` and Payment-SDK `rule/adopted/model/{mod.rs,tests.rs}`. Warning severity is replaced with errors. Matching is tightened to nominal identity, semantic word boundaries, resolved field-copy conversions, and real package dependency edges; the old shared-directory domain heuristic is removed. Cargo analysis replaces the donor's duplicate manifest reader in `model/dependencies.rs`. Old implicit macro exceptions are replaced by common reasoned comment directives.
