# rust/struct-word-count

```toml
[[rules."rust/struct-word-count"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
max_words = 2
```

Limits semantic words in named-field, tuple, and unit struct identifiers. Exactly `max_words` passes; the positive limit defaults to two. `ThreeWordRecord` fails, while `Record` and `RecordV2` pass. Enums, traits, aliases, methods, and filenames are not checked by this rule.

Preserves Payment-SDK's original tokenizer: split at lowercase-to-uppercase transitions, acronym-to-word boundaries, and transitions into digit sequences. A trailing V/v followed by digits is removed before counting. `HTTPServerV2` has two words, `BitcoinRPCClientV1` has three, and `SHA256Hash` has three. Underscores are retained within tokens as in the source implementation; naming case is a separate concern.

`target` is required; optional `exclude` accepts the same root-relative glob or nonempty-list syntax. Scope defaults to production and supports tests/all. Shared Rust test classification handles test-only items and integration sources. Findings attach to the complete struct declaration and support common reasoned directives. Invalid settings fail configuration; no blocks means unconfigured.

Migration preserves the StructNames branch and `name_words` algorithm in Payment-SDK `rule/rust.rs`, the `StructWordCount` registration in `rule/mod.rs`, and version-suffix cases in `rule/test.rs`. These source files contain other rules and must not be deleted as a whole until those checks migrate. Noun classification remains independently configurable in `rust/struct-noun-naming`. Analysis uses the shared Tree-sitter AST.
