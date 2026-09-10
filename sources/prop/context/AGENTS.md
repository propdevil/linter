You are working on a project that provides various services around trading and finance.

DB: `postgresql://prop@0.0.0.0:5433/prop` — use `psql` to access it.

---

## Toolchain — everything runs in nix

This project builds inside the flake's dev shell — locally, in CI, and in prod. **Use nix for
every command.** The `scripts/*` wrappers `exec` into `nix develop --command …`, so rustc,
node, libpq, openssl, diesel-cli, etc. all come from the flake, never the host:

```
scripts/cargo <args>         # cargo, in nix (from the repo root / workspace)
scripts/npm <args>           # npm, in nix, in YOUR cwd (run from front/ or a package)
scripts/supervisor <args>    # the `supervisor` control plane (run / logs / task / …)
scripts/compose <args>       # prod-parity docker compose
```

Equivalent for anything without a wrapper: `nix develop --command <cmd>`, or `nix develop` to
drop into the shell. The dev shell (`flake.nix` → `devShells.default`) pins rust (rust-overlay
stable), `nodejs_22`, `diesel-cli`, `pg`, `mkcert`, `just`, `jq`, `opentofu`, `sops`, `age`,
`lld`. Bare host `cargo` / `npm` are NOT supported — they drift from CI/prod.

`.claude/settings.json` registers a PreToolUse gate (`scripts/hooks/block-raw-cargo.sh`) that
rejects raw `cargo …` / `npm …` and points at the wrapper. Both the hook *logic*
(`scripts/hooks/`) and the *wiring* (`.claude/settings.json`) are tracked, so every clone gets
the same assistant setup with no per-developer step. See *Assistant gates* below for the full set.

---

## Develop & build

### Command grammar

One grammar for every command name (npm scripts + supervisor tasks): `scope:group[:action]`,
`:` is the only separator, each segment lowercase **kebab-case**. `:` separates concepts
(`db:migrate`, `gen:openapi`, `gen:scopes:check`); kebab stays *within* one concept
(`openapi`, `web-org`). In supervisor the leading segment is the service namespace
(`identity:db:migrate`, `markets:test`, `front:lint`).

### Everyday dev → supervisor tasks

The supervisor owns process lifecycle (`./supervisor.toml` includes each `service.toml`) and
runs every task inside nix. This is the day-to-day entrypoint:

```
scripts/supervisor                                 # start the "default" group (TUI)
scripts/supervisor run api                         # start a service group
scripts/supervisor run identity:api markets:api    # start specific services
scripts/supervisor task db:migrate                 # a task-group (all services, in order)
scripts/supervisor task markets:db:seed            # one namespaced task
scripts/supervisor task markets:test               # cargo test -p markets (in nix)
scripts/supervisor task front:lint                 # eslint the front-end
scripts/supervisor task openapi                    # regenerate every spec + TS (codegen loop)
scripts/supervisor logs --filter service=markets:api level=Error
scripts/supervisor restart markets:api             # restart / stop / start / status
```

Tasks are defined per service (`[tasks.*]` in its `service.toml`) and composed at the root
(`[task-groups.*]`). A task is `kind = "cargo" | "shell" | "turborepo" | "postgres"`;
`supervisor task` runs the `cargo`/`shell` ones inline (services + turborepo run under the TUI).
Logs are JSONL at `storage/logs.jsonl`; the TUI metric strip is fed by `logging::set!`.

### Build & ship → nix

The artifact that ships to prod is built by nix, never by the dev tasks above:

```
nix build .#rustBins      # every workspace binary (= .#default); .#binaries = symlink set
nix build .#web           # built front-end apps
nix build .#npmPacks      # packed @prop/* tarballs (version from front/package.json)
nix build .#dockerImage   # streamLayeredImage OCI tarball (bakes npmPacks → /srv/npm)
scripts/deploy [profile…] # refuses a dirty tree; git pull --ff-only; compose up -d --build
```

**Gotcha:** the FE nix builds pin `webNpmDepsHash` in `flake.nix`. Any FE dependency change
(`package.json` / lockfile) needs that hash updated, or `nix build .#web` / `.#npmPacks` /
`.#dockerImage` fail with a hash mismatch — copy the expected hash from the error. Local
`scripts/npm` / supervisor builds are unaffected. Deploy detail: `build/DEPLOY.md`.

### Codegen loop — Rust owns every wire shape; never hand-edit `schema.ts`

1. change the Rust type / handler;
2. `scripts/supervisor task openapi` — dumps every service's `openapi.json` (the spec build
   fails if an endpoint lacks a `#[require]`/`#[public]` annotation) **and** regenerates
   `@prop/client`'s `schema.ts` + scope maps. (Per-service equivalents:
   `scripts/supervisor task markets:openapi`, then `front:gen:openapi`.)
3. commit the regenerated `openapi.json` + `schema.ts` alongside the change.

### Migrations

Add one under `services/<svc>/migrations/` (`diesel migration generate <name>` → `up.sql` /
`down.sql`); apply with `scripts/supervisor task <svc>:db:migrate` (or the `db:migrate` group).
`embed_migrations!` bakes them into the `<svc>-db` bin; `store::run_migrations` runs them at boot.

### Version bump (`@prop/client` and every `@prop/*`)

No per-package version — `@prop/client` and all `front/packages/*` share one lockstep version
from `front/package.json` (per-package `version` fields are placeholders stamped at pack time).
Bump in the **same commit** as any `front/packages/*` change or the registry serves stale code:

```
cd front && scripts/npm run bump          # patch 0.3.0 → 0.3.1 (npm version minor|major for the rest)
```

The npm registry (`services/registry`, served at `/npm`) is immutable per version: a build
packed under an already-served version is ignored. `scripts/supervisor task registry:pack`
packs locally into `storage/npm`.

---

## What lives where

```
services/<svc>/   Rust domain service — its own DB tables, HTTP API, bins (e.g. identity, markets)
packages/<lib>/   Rust shared library — generic, reusable across services (error, store, httpio, …)
front/apps/<app>/ deployable UI (React + Vite)
front/packages/   shared FE libs — vanilla (Stencil / TS / Zustand) + the one React adapter
supervisor/       process & control plane (run, logs, tasks, TUI)
```

Cross-stack invariants (the things that must never drift):

- **Rust owns every wire shape.** The FE consumes generated types only (`@prop/client`), never
  hand-rolled TS.
- **identity is the upper domain.** Services depend only downward (markets → identity); never
  the reverse.
- **Every package/service carries its own `README.md`** (see template below). This doc teaches
  conventions; it never enumerates packages. To learn what exists, `ls` and read the READMEs.
- **Structure is machine-guarded** — `archlint` for Rust services, `eslint` for the front-end.
  If a guard rejects your change, fix the change, not the guard.

### READMEs — read them first, keep them current

Before touching a package/service, **read its README.** Before finishing, **update it in the
same change** — a stale README misleads worse than none. Keep it short and dense (low cognitive
load); follow this template exactly:

```
# <name>

- **Purpose** — why it exists, one line
- **Contains** — what's inside (entities / modules)
- **Architecture** — how it's built; the key decisions
- **Use** — how to consume it correctly
```

`archlint` enforces this on services (README present + all four sections); for other packages
it's convention. Per-symbol detail stays in the source, not the README.

---

## Design rules (both stacks)

These are stack-agnostic. Rust and Front-end sections below add only their specifics and do not
repeat these.

### Entity-driven design

- Everything is entity-driven; use OOP as much as you can. Classify the entity — the smallest
  possible unit — and compose upwards.
- `ExchangePair` is bad; package `exchange` with type `Pair {}` is better. Collections/lists of
  entities encapsulate logic well.
- Demand elegance and short names. Prefix only when the namespace is too dense; more than one
  word for a type usually means the namespace or the name is bad.
- Simple beats clever. Lots of connections is usually a sign of bad architecture.

### Naming and namespaces

- `Service` (entity) + `Services` (list) is fine; `book:Book` + `book:Books` is fine.
  `book:BookShelf` is bad — use `book:Shelf`. Only create a namespace when more entities are
  involved.
- The folder gives the namespace, so names stay short — prefer single words; a compound is
  allowed only when one word genuinely isn't enough.
- REST paths are nouns, not verbs; CRUD are the actions. Express the most important dependency
  first: `accounts/:id/portfolios`, not `portfolios/:account_id`.

### Project structure

- Structure by dependency: independent/transferable code (clients, tooling, generic libs) →
  `pkg`/`lib`-style; executables → `bin`/`cmd`; domain logic → entities + storage; usecases →
  things that combine domains + libs.
- Never make a dependency from domain → lib pointing the wrong way; keep the graph acyclic and
  pointing downward.
- **No garbage-drawer files** — `helpers`, `utils`, `misc` are catch-alls for code you failed to
  classify. Name by concept: `lib/math`→`sin`, not `utils/utils`. (eslint enforces this on the
  FE; the same discipline holds in Rust.)

### Abstraction and composition

- Start from the entrypoint and expand; extract logic into its own unit only once there's enough
  of it — not sooner. Great architecture lets you add/remove without modifying — but abstraction
  has a price, so weigh it (don't build a repository interface for a single forever-driver).
- Decompose to the smallest concern and reason upwards. If removing a piece doesn't break
  anything, it was probably a burden. Premature optimization is the root of all evil.
- In planning, outline signatures + types first to confirm direction.

### Comments and documentation

- Comments are short and only for what matters. Well-structured folders + files document
  themselves; per-package detail lives in that package's README.

---

## Rust — structure

References the shared Design rules above; this section is Rust-specific only.

### Canonical service layout (enforced at build time)

Every service `src/` contains ONLY these top-level entries — anything else fails the build:

```
<service>/src/
  lib.rs        the crate's public API (required) — re-export here to keep logical paths stable
  models/       domain entities + storage (Diesel). Push as much logic onto the entities as possible
  pkg/          self-contained, domain-specific libs. A LEAF: must NOT reference
                crate::{models,usecase,api}. Scope grammar, JWT, crypto, 3rd-party wrappers, storage init
  usecase/      business logic combining pkg/ + models/ (or several models) so api/ stays thin
  api/          the API layer (axum router + thin handlers: deserialize → usecase → serialize).
                Contents live DIRECTLY under api/ — no api/http, api/router, or api/service nesting
  db/           schema (+ pool/listen re-exported from the `store` crate)
  scopes.rs     scope catalog (the service's known service:resource:action triples)
  bin/          <service>-api, <service>-db, <service>-spec, …
  clients/      OpenAPI spec + generated rust client, for service-to-service calls
  migrations/   (sibling of src/) Diesel migrations
```

Recurring infra is a shared `packages/*` crate, never copied per service. When something generic
repeats across services, extract it into a package.

### Layering

`identity` is the foundational/upper domain — depends on no other service. Services depend only
*downward*, toward identity (`markets` → `identity`); the graph stays acyclic; a service never
depends on one above it. This is a judgment rule, deliberately not a service→rank table. **If
it's unclear where a new service sits or whether a dependency is allowed, ask — don't guess.**

### Enforcement — `packages/archlint` (FROZEN)

Each service's thin `build.rs` calls `archlint::check(CARGO_MANIFEST_DIR)`, failing the build on
a structural violation: an unexpected top-level `src/` entry, or a `pkg/` module referencing
`crate::{models,usecase,api}`. (Layering is a review rule, not validated here.) Keep `build.rs`
thin — it only calls `archlint` (and `httpio::build` where a client is generated).

> **`packages/archlint` is frozen.** Any LLM (including the assistant) is **forbidden** from
> editing `packages/archlint/**` on its own. It encodes the locked architecture contract;
> loosening a rule to make some other change compile defeats its purpose. If a task seems to
> require changing archlint (a new allowed top-level dir, relaxed pkg-purity), STOP and ask the
> user with an explicit warning that it modifies the frozen validator. Default to fixing the
> offending service instead.

### Design lint — `packages/design-lint`

An AST-level design linter, **separate from and complementary to** the frozen `archlint`. It
parses the Rust workspace and reports design smells — blocking calls in async, oversized files,
ignored `Result`s, duplicated models, and more. It is a **standalone dev/CI tool**: its own
workspace, excluded from the root `Cargo.toml` and the nix image, so its deps never reach the
container.

**The lint output is the todo list.** `scripts/lint-cases` regenerates a flat Markdown queue —
one file per finding — under:
- `lint/errors/` — violations to resolve (regenerated each run; gitignored).
- `lint/check/` — classified "review later" findings (free-function rule only).

**Rules are adopted ONE AT A TIME** via `lint/rules.enabled` (one rule id per line) — that file
is the live list, currently **17 of 23**. Rules carrying a real queue were burned down to zero
one at a time; the nine that were already at zero were switched on together as ratchets. To
adopt the next rule: add its id to `lint/rules.enabled`, run `scripts/lint-cases`, then burn
down the new `lint/errors/` **before** enabling another.

The **6 not yet adopted**, with rough cost:

| Rule | Findings | Shape of the work |
|---|---|---|
| `single-use-free-function` | 72 | warning-severity; inline or justify each |
| `deep-control-flow` | 96 | warning-severity; extract nested branches |
| `unclassified-free-function` | 204 | needs `#[hl_design::classify(...)]` per function |
| `environment-variable-access` | 12 | cascades into typed config injection across 5 crates |
| `file-length` | 13 | file splits; one source is 1433 lines |
| `platform-command-boundary` | 14 | needs a process-adapter design — spawning *is* supervisor's job |

Counts drift as code moves; re-run `scripts/design-lint services packages supervisor` for the
current summary. Note that **warning-severity rules do not fail the gates** — `design-lint`
exits non-zero only on error-severity findings, so a warning rule ratchets documentation and
`lint-cases` output, not CI.
(`scripts/design-lint services packages supervisor` prints the full rule catalog + counts.)

**Resolving `lint/errors/` (definition of done for Rust changes):**
1. Regenerate: `scripts/lint-cases`. Each case names its rule, source location, and a `Help:`
   line with the intended fix.
2. **Refactor to resolve — never suppress.** Do not weaken/remove a rule or add allow-attributes
   to empty the queue; a case disappears only when the design issue is genuinely fixed. For
   `async-blocking-operation`: use the runtime's async filesystem/IO API, or wrap the whole
   blocking operation in `tokio::task::spawn_blocking` — never just silence it.
3. Treat the case file as **evidence, possibly stale** — verify every location against current
   source before editing.
4. For large sweeps, spawn subagents on **small, disjoint batches** (one package / cohesive area
   each; never overlapping files). Each subagent must read this section and
   `lint/examples/{positive,negative}.md` in full before editing and confirm it did; a reviewer
   checks every diff against the source and never rubber-stamps.
5. Re-run `scripts/lint-cases`; confirm resolved cases disappear and no unrelated case is lost.
   Append to `lint/examples/*.md` only after explicit maintainer approval.

`#[hl_design::classify(...)]` is husklet's free-function review mechanism and is **not** in play
for the currently enabled rule — resolution here is always a real refactor.

#### Assistant gates — design lint runs automatically

Design lint is not opt-in for the assistant. `.claude/settings.json` wires four hooks, all
implemented in tracked `scripts/hooks/` and all driven off `lint/rules.enabled`, so adopting a
rule there switches on every gate at once:

| Hook | Script | Effect |
|---|---|---|
| `SessionStart` | `session-lint-contract.sh` | Injects the live rule list + "refactor, never suppress" contract into every new session. |
| `PostToolUse` (`Write`\|`Edit`) | `design-lint-file.sh` | Lints the single `.rs` file just written; blocks with the diagnostic on a violation. |
| `Stop` | `design-lint-stop.sh` | Sweeps `services packages supervisor` before the turn may end; blocks while any violation remains. |
| `PreToolUse` (`Bash`) | `block-raw-cargo.sh` | Denies bare host `cargo` / `npm`. |

The per-file gate is partial by construction — cross-file rules (`dependency-direction`,
`duplicate-entity-base`, `wire-domain-model-duplication`) need the whole parse, so the `Stop`
sweep is the authoritative check. The sweep costs ~0.1s.

The `Stop` gate honours the harness loop guard (`stop_hook_active`), so it blocks at most once
per stop chain and cannot wedge a session. Every gate fails **open**: a missing toolchain or a
linter build error exits 0 rather than trapping the turn. `scripts/lint-rules` is the shared
parser for `lint/rules.enabled` — `scripts/lint-cases` and the hooks all read through it.

#### What in `.claude/` is shared, and what is not

The assistant setup is **committed**, so a fresh clone gets the identical configuration with no
per-developer step. `.gitignore` scopes this with `.claude/*` plus negations — note the missing
trailing slash: git cannot re-include anything inside a directory excluded as `.claude/`.

| Path | Tracked? | Why |
|---|---|---|
| `.claude/settings.json` | **yes** | the four hook wirings above; portable (`${CLAUDE_PROJECT_DIR:-.}`, no absolute paths) |
| `.claude/skills/wizard/` | **yes** | the architect-mode orchestration skill, adapted to this repo's conventions |
| `.claude/agents/` | **yes** | the six-agent roster the skill dispatches |
| `.claude/settings.local.json` | **no** | per-developer permission allowlist; accumulates machine-specific paths and pasted commands that can contain live credentials |
| anything else under `.claude/` | **no** | assistant-local state |

**Never commit `.claude/settings.local.json`**, and never move an entry from it into
`settings.json` without reading the entry first — allowlist entries are verbatim command strings
and have contained secrets.

Anything committed under `.claude/` is instructions the whole team's assistant will follow.
Review a change there like you would a change to CI: check what it makes the assistant *do*, and
never let a rule that weakens a gate in through this door.

### Errors

Single classified error type — `error::Error` (crate `packages/error`) and `ApiError` (TS,
`front/packages/client/src/error.ts`). Every fallible boundary returns `error::Result<T>`; the
TS client throws `ApiError`. Nine kinds, mirrored on both sides:

```
validation | unauthenticated | unauthorized | not_found | conflict
failed_precondition | rate_limited | io | internal
```

```rust
use error::{Error, Result};
return Err(Error::not_found(format!("account {id} not found")).with("id", id));
let acct = Account::find(c, &id)?;                  // diesel/reqwest/io/… wrap via From
error::bail!(Validation, "amount must be > 0");
error::ensure!(amount > 0, Validation, "amount must be > 0");
```

- Wrap third-party errors at the boundary they enter; after that, code only sees `error::Error`.
- Pick the kind that matches the **cause**, not the HTTP status — status derives from kind.
- `.with("key", value)` attaches context (lands in the wire `details` map + the JSONL log).
- Construction is infallible (`from_response_bytes` / `ApiError.fromResponse` never throw).
- `packages/exch` + connectors return `error::Error` directly — classify at the source, no bridge
  enums. `anyhow::Result<()>` is allowed only in `bin/main()`.

### Logging

One JSONL pipeline. Every service binary calls `logging::init("name")` once in `main()`.

- `tracing::info!`/`warn!`/`debug!` — structured kv fields, not format strings:
  `info!(market_id = %m.id, "order placed")`.
- `logging::log_err!(e)` / `log_warn!(e)` — ERROR/WARN log of a classified `error::Error`; every
  `.with()` field becomes a queryable top-level field.
- `logging::tap_err!(expr)` — log-on-`Err` pass-through; pairs with `?`.
- `logging::set!("key", value)` — live gauge into the TUI metric strip.
- No `println!`/`eprintln!` in service code; init exactly once (re-init panics); never log
  secrets/credentials/raw bodies.

**Frequency discipline.** The pipeline is file-backed (`storage/logs.jsonl`, flushed per record)
— every record is a syscall. Treat log emission like a network packet: cheap once, catastrophic
in a hot loop. `info!`/`warn!`/`error!` are for lifecycle/fault events (≲ 5/s per service at
idle); `debug!` same discipline; **`set!` never in a hot loop — bump an `AtomicU64`, emit from a
1 Hz sidecar** (see `services/markets/src/usecase/book/event_reactor.rs`,
`packages/exch-polymarket/src/depth.rs`). If a service is above idle CPU, profile it
(`sample`/`perf`) and throttle at the call site — never swap in a different driver.

### REST resources, handlers, client helpers

- **Resource-first:** every route models a noun; path segments are resource names, never verbs.
  `PATCH` for partial updates, not `/credit`/`/approve`. A verb-in-path only for non-mutations
  (`POST /orders/preview`).
- **Handlers** (axum) are CRUD-named: `list` (GET collection), `show` (GET one), `create` (POST),
  `update` (PATCH), `delete`. Sub-resources scope by parent (`list_events`); special ops keep an
  English verb (`preview`, `delete_bulk`).
- **`operation_id`** = `<service>_<resource>_<action>` (`router_accounts_show`).
- **Types** carry the action prefix: `Create<Resource>Request`, `List<Resource>Query`. Wrapper
  responses only when extra metadata is needed; plain reads return the resource (`GET /accounts/:id`
  → `Account`). Every wire shape is a `ToSchema` struct — never `Object`/`JsonValue`. Modules
  disambiguate clashing names; no `RequestBody` suffix.
- **FE client helpers** deliberately diverge: `get_*` (single, throws on 404 — never null),
  `find_*` (many, may be `[]`). Keep this — it encodes the empty-vs-not-found distinction. Don't
  rename to `show_*`/`list_*`. Mutations match the backend (`create_*`, `update_*`, `delete_*`,
  `preview_*`).

---

## Frontend — structure

References the shared Design rules above; this section is FE-specific only. JS monorepo
(`front/`, turbo), same dependency-direction discipline as Rust.

### App skeleton

```
apps/<app>/
  index.html  vite.config.ts
  src/
    main.tsx      entry (ReactDOM.createRoot)
    app.tsx       router + providers
    brand.ts      per-app constants;  theme.ts  JS-side tokens that can't ride CSS classes
    styles.css    global CSS + Tailwind v4 @theme;  api.ts  @prop/client instance
    _layout/      app-shell chrome (header, navbar, footer) — wraps the page outlet
    _components/  generic UI atoms, not layout, not domain-bound (a <Pill>, <Modal>)
    lib/          pure logic, no React, transferable (challenge.ts, formatters)
    pages/<route>/index.tsx            page entry (exports Page — routing/params/top state)
    pages/<route>/_components/…        page-scoped components (entity-named: Markets/Market)
```

`_layout/` + `_components/` live at the src root; `pages/_components/` is not a thing. Don't
pre-create `_components/` — start with one `index.tsx`, split when it hurts (3+ inline components
or the file pages off-screen). `Page` stays thin; entity components own the rendering.

### Where things live

- App-shell chrome → `src/_layout/`. Generic UI atom → `src/_components/`. Page-scoped widget →
  `src/pages/<route>/_components/`. Pure logic → `src/lib/<concept>.ts` (named by concept — see
  the no-garbage-drawer rule).
- Numeric rendering → `@prop/formatters`. Runtime config (locale/currency/theme/account) →
  `@prop/context`. Anything rendering a `Market`/`Group`/orderbook → `@prop/markets`. Shared
  across apps → promote to a `front/packages/*`; apps never import from other apps.

### Conventions & defaults

- **Typed source of truth:** consume `@prop/client/<svc>` for every wire type — no hand-rolled
  shapes, no `as any`. A field/variant/endpoint change is a Rust change + the codegen loop, never
  a parallel TS edit.
- **Components are Stencil/vanilla by default.** Every `front/packages/*` is framework-free
  (Stencil for components, vanilla Zustand for stores, pure TS for functions). **Only
  `@prop/react` may import React** — it's the single adapter (generated Stencil shims + hooks); a
  future `-vue`/`-solid` adapter follows the same one-per-framework rule.
- **Apps are React.** They compose the vanilla Stencil components via `@prop/react/<concept>`; no app-local
  wrapper around a Stencil widget.

### What eslint enforces (per project — `scripts/supervisor task front:lint`)

eslint is the FE's archlint — but **per project, not one monolith**. `@prop/eslint` is a toolbox
of flat-config blocks (`recommended`, `ignores`, `naming`, `filenames("snake"|"kebab")`,
`browser`, `frameworkFree`, `noRelativeParents()`, `tailwind(entry)`, `stencilH`); each
app/package's own `eslint.config.js` composes the blocks it needs, and `turbo run lint` runs them
per-project. Tune one project by adding/dropping a block (or appending `{ rules: {…} }`) in its
config — nothing central to touch. The rules (build-failing errors unless noted):

- **Filenames:** snake_case in apps + vanilla-TS packages; kebab-case in Stencil packages
  (tag === filename); ≤ 24-char stem; no `helpers`/`utils`/`misc` basenames. Multi-word stems
  warn (prefer single words; the folder namespaces).
- **Imports:** apps use the `~/` alias — `../` parent imports banned in `src/` (sibling `./x` is
  fine). Vanilla `front/packages/*` may not import react/vue/solid (the `@prop/react` adapter is
  the sole exception). The `~/` ban is apps-only: packages keep relative imports (the alias can't
  cross package boundaries — a TSC `paths` limit, not eslint-reachable).
- **Tailwind** v4 hygiene in apps (class order, dupes, canonical) via better-tailwindcss.

---

## Quality checklist

After every change, ask:

- Is the domain well split? Can entities be decoupled further?
- Idiomatic Rust/JS? Static constructors well designed?
- A one-struct-arg function — method or free function? A one-use function — inline it?
- Does a new module carry enough logic to justify itself?
- Rock-solid beats clunky. You have a limited budget — plan so you finish well.
