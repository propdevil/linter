# Collected rule IDs

IDs extracted from Rust `id()` implementations. Same IDs may have different behavior; the complete implementations and tests are preserved separately.

| Rule | Payment-SDK | Prop | Husklet |
|---|---|---|---|
| async-blocking-operation | — | [source](prop/packages/design-lint/src/rule/blocking/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/blocking/mod.rs) |
| boolean-state-cluster | — | [source](prop/packages/design-lint/src/rule/boolean/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/boolean/mod.rs) |
| broad-trait-responsibilities | — | [source](prop/packages/design-lint/src/rule/contract/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/contract/mod.rs) |
| catch-all-module-name | — | [source](prop/packages/design-lint/src/rule/catchall/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/catchall.rs) |
| catch-all-source-path | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/catchall/mod.rs) |
| chain-layout | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | — | — |
| dependency-direction | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | [source](prop/packages/design-lint/src/rule/dependency/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/repository/dependency/mod.rs) |
| detached-constructor | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/rust/constructor/mod.rs) |
| duplicate-entity-base | — | [source](prop/packages/design-lint/src/rule/duplicate/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/duplicate.rs) |
| empty-directory | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | [source](prop/packages/design-lint/src/rule/empty/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/repository/empty/mod.rs) |
| empty-struct | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | — | — |
| environment-variable-access | — | [source](prop/packages/design-lint/src/rule/environment/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/environment/mod.rs) |
| file-length | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | [source](prop/packages/design-lint/src/rule/length/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/repository/length/mod.rs) |
| file-name-density | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/shape/mod.rs) |
| flat-prefix-density | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/shape/mod.rs) |
| flat-role-density | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/rust/role/mod.rs) |
| folder-noun | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/shape/mod.rs) |
| forbidden-path | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | — | — |
| god-object-growth | — | [source](prop/packages/design-lint/src/rule/object/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/object/mod.rs) |
| gui-toolkit-type-leakage | — | [source](prop/packages/design-lint/src/rule/toolkit/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/toolkit/mod.rs) |
| ignored-fallible-result | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/rust/result/mod.rs) |
| integration-test-candidate | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/rust/placement/mod.rs) |
| manual-cli-dispatch | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/rust/arguments.rs) |
| owned-vocabulary | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | — | — |
| path-module-flattening | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/rust/boundary/mod.rs) |
| platform-command-boundary | — | [source](prop/packages/design-lint/src/rule/command/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/command/mod.rs) |
| provisional-diagnostic | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/provisional.rs) |
| receiver-name-repetition | — | [source](prop/packages/design-lint/src/rule/receiver/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/receiver.rs) |
| redundant-accessor | — | [source](prop/packages/design-lint/src/rule/accessor/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/accessor/mod.rs) |
| redundant-module-prefix | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/shape/mod.rs) |
| redundant-parent-name | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/shape/mod.rs) |
| repository-escape | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/escape/mod.rs) |
| runtime-tool-ownership | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/ownership/mod.rs) |
| self-constructor-static | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | — | — |
| sibling-test-dependency | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/suite/mod.rs) |
| single-file-directory | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | [source](prop/packages/design-lint/src/rule/folder/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/repository/folder/mod.rs) |
| single-use-free-function | — | [source](prop/packages/design-lint/src/rule/single/mod.rs) | — |
| singular-test-file | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/shape/mod.rs) |
| string-backed-finite-state | — | [source](prop/packages/design-lint/src/rule/state/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/state/mod.rs) |
| struct-word-count | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | — | — |
| test-only-source-directory | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/suite/mod.rs) |
| test-suite-kebab-path | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/repository/suite_path.rs) |
| trait-method-count | [source](payment-sdk/packages/design-lint/src/rule/mod.rs) | — | — |
| unclassified-free-function | — | [source](prop/packages/design-lint/src/rule/function/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/function/mod.rs) |
| unsafe-boundary | — | — | [source](husklet/src/packages/hl-design-lint/src/rule/rust/safety/mod.rs) |
| wire-domain-model-duplication | — | [source](prop/packages/design-lint/src/rule/model/mod.rs) | [source](husklet/src/packages/hl-design-lint/src/rule/rust/model/mod.rs) |
