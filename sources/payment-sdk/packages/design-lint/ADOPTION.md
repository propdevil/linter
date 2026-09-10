# Adoption evidence

Implementation date: 2026-09-03. Source adoption is complete. Default activation
has been reviewed separately: all eleven original rules remain active; thirteen
adopted rules are enabled; four adopted error rules retain an existing source
backlog and are available through `Registry::all()`.

## Invariants and detector evidence

Each adopted folder contains positive/negative donor tests and focused SDK
fixtures. These checks implement the following accepted `AGENTS.md` rules;
heuristic warnings identify candidates for human review.

| Rule | Existing SDK invariant / failure detected | Focused fixture folder |
|---|---|---|
| `catch-all-module-name` | Logic stays near a precise owner; catch-all modules obscure ownership | `catchall/tests.rs` |
| `environment-variable-access` | Configuration belongs at composition; ambient reads elsewhere hide dependencies | `environment/tests.rs` |
| `platform-command-boundary` | Effects have an explicit owner; dynamic shell input and hidden process construction are detected | `command/tests.rs` |
| `ignored-fallible-result` | Typed failures must not disappear; supported Result-producing expressions cannot be silently discarded | `result/tests.rs` |
| `async-blocking-operation` | Async work must not block executors or hold recognized blocking guards across await | `blocking/tests.rs` |
| `single-use-free-function` | Behavior stays with its meaningful owner; isolated single-caller helpers are review candidates | `single/tests.rs` |
| `deep-control-flow` | Happy paths remain shallow; excessive nested control flow merits review | `nesting/tests.rs` |
| `boolean-state-cluster` | Closed states express invariants; coordinated flags can permit invalid combinations | `boolean/tests.rs` |
| `string-backed-finite-state` | Closed vocabularies are enums; assignment/comparison evidence distinguishes them from open text | `state/tests.rs` |
| `god-object-growth` | Capabilities are cohesive; accumulated unrelated workflows obscure ownership | `object/tests.rs` |
| `redundant-accessor` | Abstractions own meaning; local redundant accessors add avoidable surfaces while encapsulation stays valid | `accessor/tests.rs` |
| `wire-domain-model-duplication` | Shared identity and facts are composed; similar fields alone do not justify merging transport/domain models | `model/tests.rs` |
| `ceremonial-structure` | Namespaces, wrappers and markers must earn their boundary; required chain structure is retained | `ceremony/tests.rs` |
| `receiver-name-repetition` | A type is already a namespace; redundant receiver words hide the useful method name | `receiver/tests.rs` |
| `struct-noun-naming` | Types use precise domain nouns; adjective-only names obscure the represented value | `naming/tests.rs` |
| `unclassified-free-function` | Behavior belongs to a receiver, collection, conversion, constructor or justified algorithm | `function/tests.rs` |
| `duplicate-entity-base` | Repeated identity/facts should be considered for composition, with semantic review before changes | `duplicate/tests.rs` |

Fixture paths are relative to `src/rule/adopted/`. Broad-trait fixtures in
`contract/tests.rs` also prove that the original max-three error is retained and
that decorated traits can receive capability/implementor evidence.

## Repository activation review

The scan uses the unchanged SDK business source at baseline
`3f54bbc6c43d5792737e603d8a754c06a7553af6` plus this linter integration.
No production application findings were fixed or suppressed in this adoption.

| Selection | Error findings | Warning findings | Decision |
|---|---:|---:|---|
| Original eleven SDK checks | 0 | 0 | Always enabled |
| Five newly enabled error rules | 0 | 0 | Enabled without severity changes |
| `single-use-free-function` | 0 | 33 | Enabled as donor warning |
| `deep-control-flow` | 0 | 83 | Enabled as donor warning |
| Six other adopted warning rules | 0 | 0 | Enabled as donor warnings |
| `receiver-name-repetition` | 1 | 0 | Explicit review; `Key::native_key` |
| `struct-noun-naming` | 1 | 0 | Explicit review; indexing-redb `Stored<T>` |
| `unclassified-free-function` | 282 | 0 | Explicit review; includes framework handlers and algorithms needing ownership decisions |
| `duplicate-entity-base` | 3 | 0 | Explicit review; cursor/block shape and RPC/synchronization configuration pairs |

Default policy therefore runs 24 rules with zero errors and 116 warnings.
The all-rule review runs 28 rules with 287 errors and 116 warnings. Warning cases
are generated under `lint/errors/` with explicit severity. The four pending
rules are implemented and tested, but their source candidates have not been
accepted as defects or exceptions. Their default activation remains pending a
separate source review/refactor batch under the existing design-lint workflow.

Exact configured environment boundaries are `apps/api/src/config.rs`,
`apps/api/src/config/postgres.rs`, and the PostgreSQL/redb benchmark files.
The process boundary list is empty. No framework-wide allowance was added.

## Validation

| Check | Result |
|---|---|
| `cargo test --locked -p design-lint` | Pass: 191 library tests, 4 CLI argument tests, 2 CLI integration tests, 1 public-registry doctest; 198 total |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked --workspace --all-targets` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo doc --locked --workspace --no-deps` | Pass |
| `cargo +1.91.0 check --locked -p design-lint --all-targets` | Pass with the locked naming dependencies |
| Explicit-policy and bare `check .`; case regeneration | Pass: zero errors, 116 warnings/cases |
| Full explicit-registry review | Completed: 287 error candidates, 116 warnings |
| `git diff --check` and new-file whitespace/path review | Pass |
| `cargo test --locked --workspace` | Pass: 682 tests, zero failures/ignored, including PostgreSQL migration/repository and API acceptance fixtures |

The first sandboxed workspace run could not bind three Solana loopback sockets;
rerunning with local test permissions passed all 117 Solana unit tests and the
191 linter tests. It then stopped at the existing PostgreSQL migration fixture
because its exact pinned image was absent. Fetching that exact public image
resolved the dependency, and the complete workspace suite then passed with zero
failures. Docker's stalled credential helper was avoided with a temporary
anonymous client configuration; the user's Docker configuration was unchanged.
No test, version, image digest or schema requirement was relaxed. No owned
PostgreSQL test containers remained running after completion.

Shared regression tests additionally cover Cargo aliases, external/local name
collisions, workspace inheritance, target-specific dependency kinds, valid
cycle semantics, focused scans, test-only file/module ownership, Unicode spans
and production masking, policy errors/precedence, warning/error exit behavior,
registry uniqueness, reasoned comments versus string literals, and staged case
replacement/failure recovery.

Named-rule registration also verifies execution order and rejection of invalid
registries before parsing or rule execution. The default diagnostic output and
complete review match the previous implementation, including rule ordering,
severities, source locations, and review evidence.

`PROVENANCE.md` identifies every copied source family, license, and intentional
semantic adaptation. SDK design examples were not changed.

## Scan measurements

Three runs at initial adoption, before the named-rule registration follow-up,
using Rust/Cargo 1.97.1 on Apple Silicon,
unoptimized development profile, with build and report rendering time excluded:

| Library operation | Median |
|---|---:|
| Original source loading | 1.381 s |
| Original eleven-rule run including loading | 1.965 s |
| Current source loading | 2.340 s |
| Current retained eleven-rule run | 2.747 s |
| Current enabled twenty-four-rule run | 4.440 s |
| Current all-twenty-eight-rule review | 5.770 s |

The original loader parsed 323 Rust files before filtering, retaining 219.
The new loader skips the linter package before parsing and keeps explicit source
views: 251 parsed files, 217 production and 34 test files. These are different
amounts of analysis, not an equal-work microbenchmark. Counts and findings were
identical in all three rounds; no concurrency was introduced.

Baseline Rust source was extracted byte-for-byte from the pinned SDK revision
and built in an isolated temporary target. Only inherited manifest fields were
made standalone. Pruning the copied workspace lock required an offline build;
all 38 retained dependency package/version pairs existed in the original lock.
A temporary harness measured `Workspace::load` and `Linter::run` with a sink
reporter; the current harness used `Registry::standard`, `Registry::all`, and
the retained eleven-rule selection. These timings describe this local run,
not a performance guarantee.
