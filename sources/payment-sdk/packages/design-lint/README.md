# Design lint

`design-lint` combines Payment-SDK's eleven architecture checks with seventeen
reusable checks adopted from local Husklet. One parsed workspace, rule registry,
and reporting pipeline serve both groups. Repository policy stays in `lint.toml`.
See [PROVENANCE.md](PROVENANCE.md) for copied source and intentional adaptations,
and [ADOPTION.md](ADOPTION.md) for activation and validation evidence.

```bash
cargo run --locked -p design-lint -- --policy lint.toml check .
cargo run --locked -p design-lint -- --policy lint.toml --markdown .
cargo run --locked -p design-lint -- --policy lint.toml --cases lint .
```

Diagnostic and Markdown modes fail on **errors**; warnings remain visible with a
successful exit. Cases is a review command and succeeds after writing reports,
even when findings contain errors. Parsing, configuration and output failures
always fail.

Diagnostic, Markdown, and case locations start with the project directory name,
for example `Payment-SDK/sdk/chains/ethereum/src/transaction/coordinator_state.rs:50:17`.
Related locations use the same format, including when scanning a single file or
subdirectory within the project.

An explicit `--policy` wins. Otherwise the CLI loads `lint.toml` from the current
working directory if present; without it, standalone defaults run the original
eleven checks. Unknown fields, rule IDs, layer references and invalid boundary
selectors are rejected.

Cases writes errors and warnings to `lint/errors/`, with severity in each file.
Only files carrying its generated ownership marker are replaced or removed,
including stale marked files in `lint/check/`. `.gitkeep`, unmarked legacy
reports, and user notes remain. Analysis and staging finish before replacement;
handled publication failures roll back, preserving backups if rollback fails.
This is not a crash-atomic or concurrent-writer protocol.

## Always-on SDK rules

| Rule | What it protects |
|---|---|
| `dependency-direction` | Local Cargo dependencies follow configured layers and contain no cycles. |
| `owned-vocabulary` | Chain names stay in their owning chain or composition paths. |
| `trait-method-count` | Traits contain at most three cohesive functions. |
| `empty-struct` | Structs carry meaningful state. |
| `struct-word-count` | Struct names contain at most two semantic words; trailing version suffixes are ignored. |
| `self-constructor-static` | Constructor-shaped functions returning `Self` are associated functions, not receiver methods. |
| `file-length` | Rust files stay below the configured production-line limit. Parsed `#[cfg(test)]` modules and items, exact `#[test]` items, and standalone test sources do not count. |
| `forbidden-path` | Deleted architecture does not return. |
| `empty-directory` | Repository-owned directories are not filesystem-empty; `.gitkeep` counts as content. |
| `chain-layout` | Concrete chain crates share the configured indexer, RPC, transaction, operations, source, and wallet directory topology; protocol-owned files inside those directories may differ. |
| `single-file-directory` | Every Rust module directory under `src` contains zero or at least two direct Rust files, so each namespace earns its directory. |

## Adopted rules

`[rules].enabled` adds adopted checks without disabling or changing the severity
of an original SDK rule. The checked-in policy enables thirteen adopted rules:
five errors with a clean source scan and eight advisory warnings. Four further
error rules remain available for explicit review while their source findings
await ownership review. This is activation status, not a severity downgrade.

| Rule | Severity | Default in SDK |
|---|---|---|
| `catch-all-module-name` | Error | Enabled |
| `environment-variable-access` | Error | Enabled |
| `platform-command-boundary` | Error | Enabled |
| `ignored-fallible-result` | Error | Enabled |
| `async-blocking-operation` | Error | Enabled |
| `single-use-free-function` | Warning | Enabled |
| `deep-control-flow` | Warning | Enabled |
| `boolean-state-cluster` | Warning | Enabled |
| `string-backed-finite-state` | Warning | Enabled |
| `god-object-growth` | Warning | Enabled |
| `redundant-accessor` | Warning | Enabled |
| `wire-domain-model-duplication` | Warning | Enabled |
| `ceremonial-structure` | Warning | Enabled |
| `receiver-name-repetition` | Error | Explicit review |
| `struct-noun-naming` | Error | Explicit review |
| `unclassified-free-function` | Error | Explicit review |
| `duplicate-entity-base` | Error | Explicit review |

Husklet's broad-trait analysis enriches `trait-method-count` with capability and
implementation evidence; SDK's maximum remains three methods. Its overlapping
length/directory rules do not replace SDK requirements. Its GUI-toolkit rule
is not included.

Review all implemented checks through the same runner:

```bash
cargo run --locked -p design-lint --example review-adoption -- --policy lint.toml .
cargo run --locked -p design-lint --example review-adoption -- --policy lint.toml --cases /tmp/sdk-lint-review .
```

This example is a review harness: successful report generation returns success
even when the explicitly selected registry finds errors. The normal CLI remains
the enforcement command. Library callers can compose `Linter::new(policy,
registry)`, `Registry::standard(&policy)`, `Registry::all()`, or a custom registry
of named `Rule` implementations:

```rust
use design_lint::{Linter, Policy, Registry, rule};

let registry = Registry::new()
    .register(rule::DependencyDirection)
    .register(rule::FreeFunction)
    .register(rule::DuplicateEntity)
    .register(rule::BooleanState);
let linter = Linter::new(Policy::default(), registry);
```

The complete built-in registration chain lives in `standard_rules()` in
`src/lib.rs`. Standard and all-rule selections use this one ordered catalog.
An explicit registry runs exactly the supplied rules in registration order.
Empty or duplicate IDs are rejected before source loading or rule execution;
inconsistent finding severity/IDs are rejected before reporting.

## Boundaries and exceptions

`rust.forbidden_modules` configures catch-all module names.
`boundaries.environment` and `boundaries.process` accept selectors with `package`,
`path`, or both. Paths are relative to the owning Cargo workspace root (or package
root for a standalone package), match complete components, and retain their
meaning during focused source scans. Both fields must match when both are given.
An empty selector, absolute path, or parent traversal is invalid.

The SDK selects its exact configuration and benchmark files for environment
access and grants no production process boundary. Known test scopes retain the
donor test behavior; unsafe dynamic-shell construction is still examined.

An exceptional Rust declaration may suppress a source rule on its declaration
line or either of the two preceding lines. A specific rule and reason are
mandatory. Literal strings never act as directives:

```rust
// design-lint: allow self-constructor-static -- conventional consuming conversion
fn from_parts(self, parts: Parts) -> Self { /* ... */ }
```

Do not add speculative style checks. A new rule needs an explicit architecture
invariant, a concrete failure it prevents, and focused positive/negative tests.

Adopted checks use the same exception contract. `hl_design` attributes are not
an SDK exception mechanism; recognized framework signatures and review metadata
also do not suppress findings.

The detectors use parsed syntax and conservative reference evidence, not compiler
type resolution or macro expansion. A finding can require human review rather
than imply that two models or similarly named references are semantically equal.
Standalone and proven test-only module files remain available to appropriate
rules but are excluded from production checks.
