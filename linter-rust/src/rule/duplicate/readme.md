# rust/duplicate-entity-base

Reports related Rust models repeating at least three named fields with identical resolved types. Findings are errors and include both definitions and the exact shared fields.

```toml
[[rules."rust/duplicate-entity-base"]]
target = "**/src/**/*.rs"
exclude = "generated/**/*.rs"
min_shared_fields = 3
scope = "production"
```

`target` is required and accepts a glob or nonempty list. `exclude` accepts the same syntax. `min_shared_fields` defaults to three; smaller thresholds are rejected. `scope` accepts `production` (default), `tests`, or `all`. Targets select the model reported; other discovered models may supply evidence. Excluding a path prevents reporting that path, not consulting its declaration as evidence.

Models must belong to the same Cargo package and share complete trailing name words, such as `Wallet` and `ImportedWallet`, or have a resolved standard `From`/`TryFrom` implementation relating them. Unrelated `Image` and `Invoice` shapes do not trigger a finding. Substring coincidences such as `Art` and `Cart` do not count. Platform-gated models are conservatively excluded as alternative compilations.

```rust
struct Wallet { id: u64, address: String, network: String }
struct ImportedWallet { id: u64, address: String, network: String, birthday: u64 }
```

These repeat three fields and fail. Compose a shared `Wallet` into the imported representation when they share identity and invariants.

```rust
struct Email(String);
struct WalletId(String);
```

These pass. Tuple wrappers and models with fewer than three shared named fields are outside this rule. Types retain nominal identity: an `Email` field differs from a `WalletId` field even though both contain `String`. Simple aliases and explicit imports can establish shared identity; unresolved or ambiguous types cannot. Generic aliases and unresolved external types are left unclassified. Generic container arguments remain part of the type identity.

The shared declaration index uses the existing Rust AST, Cargo ownership, source module paths, inline modules, and lexical item scopes. It does not infer runtime invariants or expand macros. The rule does not equate fields by primitive storage or by the spelling of unresolved type identifiers across modules.

Migration preserves the three-field threshold and package ownership from Husklet, Prop, and Payment-SDK `duplicate-entity-base`. It strengthens type evidence, replaces substring suffix matching with whole words, and removes the old same-module-name shortcut that could equate unrelated entities. Donor wallet tests now declare their field types rather than treating unresolved spelling as proof. Resolved conversion relationships are an additional source of ownership evidence.
