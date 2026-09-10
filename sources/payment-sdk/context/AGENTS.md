# Repository guide for coding agents

These rules define Payment-SDK's durable product, architecture, design, coding,
and delivery standards. Apply them to new code and improve nearby code when
safe. Preserve behavior during refactors and prove changes with observable
tests. Prefer fewer, stronger abstractions and less code.

## Source of truth

Read this file before changing code. Use project sources in this order:

1. `docs/SYSTEM_REQUIREMENTS.md` for canonical scope and acceptance criteria.
2. `ARCHITECTURE.md` for ownership and dependency direction.
3. `docs/CONTRACTS.md` for reusable Rust boundaries; source is authoritative
   for exact signatures.
4. `docs/FEATURE_VALIDATION.md` for implementation evidence and honest status.
5. `docs/INDEXING.md` for indexing and persistence semantics.
6. Current source and tests for implemented behavior.

Follow the higher-authority source when documents disagree and update the
stale document. The project is pre-release: replace unpublished internal types
and wire formats directly. Do not add compatibility aliases, legacy runtime
readers, or project-defined `V1`/`V2` DTOs. The shared PostgreSQL database is
durable multi-chain state, not a disposable internal format: its canonical
deployment-owned creation and ordered migration scripts live physically under
`sdk/indexing/postgres/migrations/` and evolve preservation-first. A migration
or exact-scope rescan must preserve unrelated scopes and application-owned
tables unless their owner separately approves a change. External Bitcoin,
Ethereum, Solana, JSON-RPC, and HTTP standards remain compatibility contracts.

The current predeployment PostgreSQL history contains one fresh-schema
initializer. Apply it only to an empty schema. Freeze its exact bytes and
checksum at the first persistent deployment; every later schema change is a new
ordered migration rather than an edit or replay of the initializer.

`old/` and `reference/` are excluded research, not production dependencies or
architecture templates. Do not edit them unless explicitly asked.

If `.codegraph/` exists, use `codegraph explore "<symbols or question>"` before
grep/find or broad reading when locating or understanding code. If it does not
exist, skip CodeGraph; repository indexing is the user's decision.

## Product boundary

Payment-SDK's accepted target is one API process that initializes Bitcoin,
Ethereum, and Solana RPC clients and embedded synchronizers; generates or
imports wallets without returning secrets; returns canonical addresses, exact
balances, and complete history; submits one transfer or a non-empty ordered
batch; indexes caller-selected addresses; and survives restarts and retained
reorgs. Current source initializes Bitcoin and Ethereum only; Solana remains an
implementation gap.

Business and HTTP code must not import a concrete chain. Concrete chains are
selected once at composition.

The accepted architecture includes Solana, but the Solana crate and `apps/api`
runtime composition remain unimplemented and source-step gated. Do not add a
nonexistent crate to the current ownership table or claim runtime support before
implementation evidence exists.

There is currently no deposit accounting, ledger, payment state machine,
collection/sweep job, reservation system, hardware-wallet workflow, remote
custody, separate wallet/indexer service, raw-block archive, or public indexing
command surface. Do not introduce these concepts without an approved product
change or claim development in-process custody is production custody.

## Architecture and ownership

`apps/api` is the only executable and composition root. No crate depends on it.

| Path | Owner |
|---|---|
| `apps/api` | Config, public HTTP/OpenAPI, concrete composition, task supervision, shutdown |
| `sdk/chains/base` | Small approved chain-neutral values, signing requests, transaction snapshots, broadcast capabilities |
| `sdk/chains/bitcoin` | Bitcoin addresses, RPC, blocks, UTXOs, scripts, fees, transactions, wallets, indexing translation |
| `sdk/chains/ethereum` | Ethereum addresses, RPC, blocks, gas/nonces, typed transactions, receipts/logs, wallets, indexing translation |
| `sdk/wallets` | Chain-neutral capabilities, providers, wallet registry, history presentation, batch orchestration |
| `sdk/indexing` | Storage-independent synchronization, address filters, canonical history, checkpoints, reorg handling, repository contracts |
| `sdk/indexing/postgres` | Physical home for the deployment-owned shared-schema history; production indexing repository implementation |
| `sdk/indexing/redb` | Embedded/test indexing repository implementation and all redb record/key encoding |
| `packages/*` | Generic crypto, HTTP, JSON-RPC, and storage mechanics usable outside blockchain projects |
| `packages/design-lint` | Architecture and Rust API checks configured by `lint.toml` |

Dependency rules:

- Packages do not import SDK or application crates.
- `sdk/chains/base` does not import concrete chains, RPC, indexing, or wallets.
  It is not a universal transaction or RPC abstraction.
- `sdk/indexing` knows no native block, backend record, wallet registry, Axum
  route, or business label.
- Concrete chains retain chain-native semantics while implementing wallet and
  indexing contracts. Removing one chain leaves the other generic crates and
  other chain usable.
- `sdk/indexing/postgres` and `sdk/indexing/redb` implement persistence only;
  neither owns a synchronizer runtime or public handle.
- `apps/api` constructs one process-wide PostgreSQL pool, one scope-bound
  repository handle per `(chain, network)`, and all long-lived RPC clients,
  synchronizers, and providers once. An asset never gets its own database,
  schema, pool, or repository. Handlers receive abstractions and do not open
  storage or construct dependencies per request.

Concrete chain crates keep this design-lint-enforced skeleton:

```text
src/address.rs
src/batch.rs
src/error.rs
src/lib.rs
src/indexer/mod.rs
src/indexer/source/mod.rs
src/rpc/mod.rs
src/transaction/mod.rs
src/transaction/operations/mod.rs
src/wallet/mod.rs
```

Protocol-specific children may differ. Do not flatten cohesive directory
modules into oversized files.

## Domain models and collections

Models express payment and chain language, valid state, invariants, and
behavior. Put behavior on the value it primarily uses. Start with the smallest
meaningful entity and compose entities into collections and capabilities.

An entity owns one thing's identity and invariants. A plural type owns
operations over a meaningful collection. Prefer `Wallet`/`Wallets` and
`ValueMovement`/`Movements` to `WalletData`, `WalletManager`, helpers, or raw
vectors passed between free functions.

```rust,ignore
pub struct Wallets<K: Ord> {
    values: BTreeMap<K, Arc<dyn Wallet>>,
}

impl<K: Ord> Wallets<K> {
    pub fn insert(&mut self, key: K, wallet: Arc<dyn Wallet>) -> Result<(), Error> {
        // Enforce duplicate and identity rules here.
    }

    pub fn get(&self, key: &K) -> Result<Arc<dyn Wallet>, Error> {
        // Keep lookup policy with the collection.
    }
}
```

A domain collection must own cohesive construction, selection, validation,
aggregation, or transformation; it must not merely forward `Vec` methods.
Receipt-success rules, for example, belong to a collection constructor:

```rust,ignore
struct Movements(Vec<ValueMovement>);

impl Movements {
    fn successful(
        transaction: &ParsedTransaction,
        receipt: &ParsedReceipt,
        scope: &IndexScope,
    ) -> Result<Self, IndexError> {
        // Build native and token movements under one success policy.
    }

    fn into_vec(self) -> Vec<ValueMovement> {
        self.0
    }
}
```

The collection owns the result's meaning; an orphan
`successful_movements(...) -> Vec<_>` does not. Keep chain-specific collections
private to their chain unless a stable cross-chain contract needs them.

Use associated constructors for validation and policy:

```rust,ignore
let address = bitcoin::Address::parse_for_network(input, network)?;
let indexer = bitcoin::Indexer::new(source, interpreter, blocks, config);
let indexers = indexing::Composer::new(vec![Arc::new(indexer)])?;
```

Use `From`, `TryFrom`, and `FromStr` for complete policy-free conversions. Use
named constructors for network, chain, wallet, fee, or storage policy.

Before preserving a primitive as state, inspect all assignments and
comparisons. Model closed vocabularies such as synchronization phase and
transaction status as enums; keep wire/storage encoding at the boundary.
Preserve unknown extensible protocol values when required. Identifiers,
messages, and user text are not closed enums.

When types duplicate the same identity and facts, compose one shared entity
into specialized nouns. Put shared behavior on the base and stage-specific
behavior on its wrapper. Expose deliberate access/conversion; do not use
`Deref`, field forwarding, or a trait to imitate inheritance. Similar field
names alone do not prove a shared entity.

Do not create a wrapper merely to turn one helper into a method. Decide whether
behavior belongs on an existing receiver/collection, a standard conversion, an
associated constructor, intentional inline code, or a genuine low-level or
multi-entity algorithm. Several cohesive rules and invariants establish a new
concept; one shared primitive argument does not.

## Capabilities and public APIs

Give each trait one capability and at most three tightly coupled methods.
Prefer standard traits and concrete code to ceremonial abstractions.

```rust,ignore
pub trait Addresser {
    fn address(&self) -> Address;
}
```

Code needing an address accepts `&dyn Addresser`; code needing history accepts
`&dyn HistoryReader`. Use `dyn Wallet` only where the composed capability is
needed. `Provider` constructs wallets; it is not a post-construction wallet
capability. Crate roots re-export small stable surfaces and keep implementation
modules private.

Keep logic and types close to their owner. Reusable wallet, indexing,
transaction, address, and storage models remain beside the domain behavior
that defines them. Do not move them into application `dto`, `models`, or
`common` namespaces.

### HTTP endpoints

Handlers are thin public adapters grouped by resource (`wallet`,
`transaction`, `health`, `contract`) and contribute to one Utoipa contract.
Define each endpoint's simple JSON input and output structs immediately above
the endpoint function so its contract is visible in one place. Do not scatter
endpoint-specific types through catch-all `request.rs`, `response.rs`, or
`dto.rs` files. Extract a shared wire type only when the exact same type is
genuinely used by multiple endpoints. Reusable domain models remain in their
owning SDK/domain module.

```rust,ignore
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct CreateWalletRequest {
    pub chain: Chain,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct CreateWalletResponse {
    pub id: WalletId,
    pub address: String,
}

pub async fn create(
    State(wallets): State<Arc<Wallets<WalletId, Chain>>>,
    Json(input): Json<CreateWalletRequest>,
) -> Result<Json<CreateWalletResponse>, HttpError> {
    wallets.generate(input.chain).await.map(Json).map_err(Into::into)
}
```

HTTP owns extraction, authentication, limits, status mapping, and encoding.
Domain models own validation; a precise domain collection owns orchestration.
Axum handlers may remain free functions because the framework
owns their extractor signature, but their bodies stay to extraction, one
application call, error translation, and encoding. SDK crates know no Axum,
route, bearer token, or response shape. Secrets never derive serialization,
schema, or ordinary `Debug`.

## Transaction and wallet invariants

```text
chain-native request
  -> unsigned chain-native transaction
  -> chain computes payload/digest
  -> injected Signer signs it
  -> chain validates and inserts signature
  -> exact signed bytes are broadcast
  -> indexing observes inclusion, confirmation, failure, or reorg
```

- Never create a universal Bitcoin/Ethereum/Solana RPC or transaction model.
- Bitcoin owns network checks, outpoints, scripts, UTXOs, dust, checked
  satoshi fees/weight, sighashes, witnesses, and consensus encoding. History
  preserves every input/output movement. A batch may fund one transaction from
  several wallets, with deterministic fees and per-source change/signatures.
- Ethereum owns chain ID, signer recovery, nonce, gas, checked EIP-1559 fees,
  typed envelopes, token calldata, receipts, and logs. Native and token assets
  remain distinct and exact `U256` values are preserved. Batch transactions use
  consecutive per-source nonces, broadcast in input order, and report accepted
  prefix plus first failure.
- Solana owns Base58 addresses, Ed25519 seeds, slots, account/context floors,
  legacy System-transfer-plus-Memo messages, blockhash lifetime, fees,
  simulation, exact signatures, ordered broadcast, and source-keyed ambiguity
  coordination. Its singular endpoint has no transparent retry, and indexing
  remains the confirmation authority.
- A batch is one non-empty ordered list on one chain. Validate destinations,
  amounts, fee bounds, wallet compatibility, and chain invariants before the
  first external effect.
- Preserve exact signed bytes across ambiguous retryable broadcasts. RPC
  acceptance means submitted, not confirmed; RPC failure remains unknown and
  does not prove a drop. Confirmation comes only from indexing.
- Money uses checked atomic integers and exact `Decimal`, never floating point.

## Indexing invariants

- `Indexer` is the complete chain-neutral indexing surface used by callers.
  Bitcoin, Ethereum, Solana, and the multi-chain `Composer` implement the same
  trait; callers do not change APIs when moving from one chain to several.
- A checkpoint contains native position, produced height, hash, and one atomic
  optional parent pairing parent position with parent hash. Native position
  drives traversal, canonical lookup, restart, readiness, and birthdays;
  produced height drives confirmation, ordering, journal keys, and retention.
- `Wallets` supplies a caller-owned `FilterSource`; each read yields one complete
  `Vec<AddressFilter>`. Wallet/application runtime owns address and birthday
  lifecycle; indexing persists no watch, watch ID, or address registry.
- Transactions preserve all stable movements. Never flatten a
  multi-input/multi-output UTXO transaction into a fictional transfer.
- Confirmation derives from inclusion produced height and checkpoint produced
  height; it is not stored as a transition. Checkpoint-bound pagination
  restarts if the checkpoint changes.
- One commit atomically writes address-primary canonical history, live output
  changes, one bounded rollback-journal entry, and checkpoint movement.
- A retained reorg atomically removes orphan canonical history, restores live
  projection state, and moves the checkpoint back. `ReorgTooDeep` requires the
  caller to recreate and rescan only that exact scope's indexing-owned rows;
  unrelated scopes and application-owned tables remain untouched.
- Persistence uses obvious domain collections. `Blocks` owns canonical
  checkpoint lookup, retained block lookup, atomic add, and atomic removal;
  `Transactions` lists canonical address history; `Outputs` lists live UTXOs.
  Do not split load/plan/apply phases into ceremonial public traits or expose
  rollback command/context bags.
- `Blocks::add` atomically writes history, live outputs, rollback journal, and
  checkpoint. `Blocks::remove` derives its inverse from the stored journal; it
  never accepts caller-authored undo state. Chain interpreters emit facts;
  storage keys, codecs, records, and compare-and-swap mechanics remain private.

Store only canonical checkpoint, bounded journal, address-primary canonical
history, and current live outputs. Do not add watches, synchronizer status,
observation revisions, pending confirmations, spent markers, secondary address
indexes, event feeds, or raw-block archives.

The production composition uses one PostgreSQL database, one schema, and one
process-wide pool. `Repository::new(pool.clone(), scope)` binds a handle to one
exact `(chain, network)` and rejects another scope. The canonical ordered schema
history is deployment-owned and lives physically under
`sdk/indexing/postgres/migrations/`. Physical script or table colocation does
not grant indexing runtime ownership of application data: the runtime adapter
must not query, rewrite, truncate, delete, or issue application-table DDL for
`payment_wallets`. An
application-owned restart read path must be proven before the current indexing
registry query surface is removed.

## Coding, errors, security, and concurrency

- Rust 2024, resolver 3, MSRV 1.91, locked dependencies. Workspace
  `unsafe_code = "forbid"` must not be weakened.
- Return typed errors from libraries and add actionable context at application
  and transport boundaries. Preserve retryability and ambiguous outcomes.
- Use `?`, checked arithmetic, and boundary validation. Avoid production
  `unwrap`, `expect`, and panic unless a local invariant makes failure
  impossible and evident.
- Parameterize external values; never interpolate untrusted data into commands,
  paths, keys, or URLs without canonical boundary encoding.
- Private keys never appear in logs, JSON, responses, snapshots, clones, or
  `Debug`; use zeroizing secret memory. Concrete chains accept injected
  `Signer`s and do not select custody policy.
- Never use funded keys or public broadcast endpoints without explicit user
  authorization for that exact external action.
- Borrow for observation; transfer ownership for storage/transformation. Clone
  only when ownership requires it. Use `Arc`, locks, channels, and atomics for
  their semantics, not by habit.
- Introduce concurrency only for real parallelism/latency. Do not block async
  executors or hold locks across `.await`; move blocking redb/CPU work off
  executors and bound spawned work.
- The `apps/api` composition root owns cancellation, fatal-task handling,
  readiness, and graceful shutdown. Do not hide unresolved chain composition
  inside a generic `Application` wrapper. The public listener starts only after
  each configured index reaches `SyncPhase::Ready` with a persisted checkpoint.

## Delivery and style

- Reject unsupported meaningful input; never accept ignored configuration.
- Prefer mature protocol libraries over local standard reimplementations.
- Write a failing behavioral test for a defect or testable behavior. Test exact
  outcomes, edges, errors, and invariants.
- Unit tests stay beside their owner. Repository contract tests cover both redb
  and PostgreSQL. System tests compose the public facade, RPC doubles,
  synchronizer, and one owned disposable PostgreSQL database/schema through one
  process-wide pool. Tests are deterministic, own their resources, preserve
  sentinel application rows, and never contact public networks.
- Indexing coverage includes birthdays, catch-up, checkpoint/hash restart,
  duplicate commit, retained reorgs, `ReorgTooDeep`, orphan removal, Bitcoin
  output restoration, native-position restart, produced-height confirmation,
  sparse Solana slots, Ethereum native/token movements, shared-schema scope
  isolation, and RPC outage without false terminal state.
- Use precise nouns and plural collections. Avoid `Manager`, `Helper`, `Util`,
  `Impl`, vague abbreviations, repeated chain prefixes, and catch-all `core`,
  `common`, `shared`, `util`, or `misc` modules.
- A type is already a namespace: prefer `Wallets::insert`, `Index::history`,
  and `Address::parse_for_network` over repeated receiver names.
- Prefer receiver methods, `Display`, `FromStr`, `From`, `TryFrom`, and iterator
  traits when they express the contract. Newtypes must enforce an invariant,
  prevent meaningful mixing, own cohesive behavior, or stabilize a boundary.
- Keep the happy path shallow and matches exhaustive. Derive traits only when
  semantics are correct; inspectable non-secret domain values implement
  `Debug`.
- Comments explain contracts, security, compatibility, or non-obvious reasons;
  names explain mechanics. Keep public APIs and rustdoc focused.
- Production Rust is limited to 500 counted physical lines. Standalone test
  files and lines inside test-only Rust items/modules do not count. Production
  and tests must still be organized cohesively; numbered fragments and
  `include!` do not create boundaries.
- Refactor incrementally, search callers before renaming, and delete obsolete
  code rather than maintaining parallel designs.

Every addition must answer: which domain/folder owns it, is it close to that
owner, and does the public API remain simple for callers?

## Design-lint workflow

`packages/design-lint` enforces `lint.toml`: dependency direction, owned chain
vocabulary, small traits, meaningful structs, concise names, associated
constructors, the production-only line limit, forbidden paths, non-empty
directories, and concrete-chain layout.

```bash
cargo run --locked -p design-lint -- --policy lint.toml check .
cargo run --locked -p design-lint -- --policy lint.toml --markdown .
cargo run --locked -p design-lint -- --policy lint.toml --cases lint .
```

Diagnostic mode fails on findings; Markdown emits a review; cases refreshes
`lint/errors/` while retaining `.gitkeep` and currently clears generated files
from both `lint/errors/` and `lint/check/`. A generated case is evidence, not
source of truth.

Resolving `lint/errors/` starts with subagents. Assign each a small disjoint
batch from one crate/domain and never overlap source files. Before editing,
each must read `AGENTS.md`, `lint/examples/positive.md`, and
`lint/examples/negative.md` completely, then report exactly:

```text
Read positive.md in full: yes
Read negative.md in full: yes
```

Reject work missing either confirmation. Each agent then verifies current
source, uses, callers, sibling behavior, models, manifest, and tests; identifies
meaning, invariants, effects, and owner; searches for cohesive behavior and
closed primitive state; and prefers an existing receiver/collection, standard
trait/conversion, constructor, inlining, or genuine algorithm. Report every
case as `refactored`, `exception proposed`, or `blocked`, with owner, API,
reasoning, files, and focused tests.

Subagents implement; they do not approve suppression. If no safe final design
is justified, leave source unsuppressed and report evidence, owners considered,
and what is missing. The manager reviews every diff and applies only an exact,
narrow exception when warranted:

```rust,ignore
// design-lint: allow self-constructor-static -- conventional consuming conversion
fn from_parts(self, parts: Parts) -> Self {
    // ...
}
```

A rule name and concrete reason are mandatory. Never use nonexistent
`hl_design` macros, mass-allow findings, weaken `lint.toml`, or add speculative
rules to empty the queue. A new rule needs an explicit invariant, a prevented
failure, and focused positive/negative tests. Update the example documents only
after explicit user approval of the pattern or rejection; silence and “next”
are not approval.

After each batch, inspect the diff and focused tests. After integration, run:

```bash
cargo test --locked -p design-lint
cargo run --locked -p design-lint -- --policy lint.toml --cases lint .
cargo run --locked -p design-lint -- --policy lint.toml check .
```

Resolved cases must disappear, remaining violations stay flat in
`lint/errors/`, `.gitkeep` remains, and no unrelated case or source change is
lost.

## Change workflow and validation

Inspect `git status --short`, preserve unrelated changes, read the owning
manifest/API/callers/tests, identify ownership, make one coherent change,
update public-contract docs, add focused tests, and report pre-existing failures
separately. Never weaken a gate.

Use locked dependencies. Workspace completion requires:

```bash
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo doc --locked --workspace --no-deps
cargo run --locked -p design-lint -- check .
git diff --check
```

For Markdown-only changes, `git diff --check` plus path and terminology review
is enough. Public API, Cargo, transaction, signing, persistence, or
synchronization changes require focused crate tests followed by workspace
gates.

## Running a fleet

Keep roughly six agents busy when independent work exists. Give each a bounded,
non-overlapping crate or source area and continue manager work while they run.
Do not create overlapping edits merely to meet a number, but do not wait on one
agent while five useful independent tasks are available.
