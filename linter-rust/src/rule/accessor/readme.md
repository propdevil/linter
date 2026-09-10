# rust/redundant-accessor

```toml
[[rules."rust/redundant-accessor"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
```

Reports direct accessors over fields already available to the same or a broader audience, and duplicate accessors with equivalent contracts on one resolved nominal owner. A public `value()` returning an already public value adds no access boundary. Two private-field getters with identical operation, return type, receiver ownership, visibility, and const contract duplicate one another.

Recognized bodies contain one direct operation: return `self.field`, borrow it, mutably borrow it, clone it, or assign the supplied setter argument. Explicit return statements and parentheses are supported. Return/argument types must resolve and exactly match the operation's field type. A borrowed slice view of Vec or str view of String is preserved as a distinct conversion contract.

Private fields with a single deliberate accessor pass. Multiple statements, validation, arithmetic, method conversions, asynchronous/unsafe/extern methods, method generics/where clauses, trait implementations, serialization/ABI markers, deprecation, and platform-dependent contracts are excluded conservatively. Receiver mutability and consumed versus borrowed self remain distinct. Tuple-newtype storage is never compared across wrappers.

```rust
struct Wallet { address: Address }
impl Wallet {
    fn address(&self) -> &Address { &self.address }
}
// Pass: the field remains private and the accessor owns its boundary.
```

The declaration index preserves type identity across inline modules, aliases, and split inherent implementations. Unresolved or ambiguous owners and field types do not establish redundancy. Duplicate findings point at the later accessor and include the original contract; exposed-field findings include the field location. Three identical accessors produce findings for the second and third rather than every pair.

Required `target` and optional `exclude` accept root-relative globs or nonempty lists. They select methods to report; other discovered methods can supply equivalence evidence. Scope defaults to production and supports tests/all through shared test classification. Common reasoned directives apply. Unknown settings and invalid selectors fail configuration; no blocks means unconfigured.

Migration preserves the access/ownership regression corpus from Husklet `rule/rust/accessor/{mod.rs,test.rs}`, Prop `rule/accessor/{mod.rs,tests.rs}`, and Payment-SDK `rule/adopted/accessor/{mod.rs,tests.rs}`. Warnings become errors. Canonical owner identity replaces package/name-only matching; signature and field-type matching preserve coercion and ownership boundaries more conservatively than the source. The wallet regression declares its referenced nominal types explicitly because unresolved names no longer provide type evidence. Analysis reuses shared Tree-sitter syntax.
