---
name: software-design
description: Opinionated, entity-driven software design principles for structuring code, naming, picking abstractions, and building software in general. READ always.
---

# Software Design

## Use the linter to make the design executable

This skill and the linter are one workflow: decide ownership and invariants,
encode the enforceable parts in `linter.toml`, implement, and validate the saved
files. The linter provides evidence; it does not choose the product architecture
or prove correctness. Preserve the user's requirements and existing contracts.
Do not add abstractions, services, storage, or directories merely to satisfy a
rule or an example below.

### Start with the project

1. Inspect existing requirements, manifests, code, callers, and tests. Identify
   composition roots, reusable packages, domain entities, and dependency direction.
2. If `linter.toml` already exists, read and preserve it. Otherwise obtain the
   matching bundled preset, then tailor its targets to the actual project.
3. Encode ownership with category globs such as `apps/*` and `packages/*`, not
   an inventory of today's directories. Separate structural requirements from
   naming and semantic checks.
4. Run the baseline check. Inspect each finding's rule, path, evidence, and repair
   instruction before changing code. A status of `unconfigured` is not a pass.
5. Make a cohesive change, run focused behavioral tests, format, and lint again.
   Finish by running the project's full required checks and reporting remaining
   failures honestly. Never raise limits, broaden exclusions, add meaningless
   files, or rename a concept dishonestly just to clear findings.

### MCP workflow

The bundled server exposes the `check` tool. Call it with an absolute project root:

```json
{"root": "/absolute/path/to/project"}
```

Read a preset through MCP resources: `linter://configs/rust.toml`,
`linter://configs/c.toml`, or `linter://configs/default.toml`. Save the selected
content as the project's `linter.toml` only when no configuration exists, then
adjust its rules to the project's agreed design. Resources are read-only;
use the agent's ordinary file-editing tools to create or update configuration.
The server checks saved files and does not edit them. Retry a busy check after
its current worker finishes. Configuration/analysis failures are tool errors;
a successful tool call can still contain lint findings, so inspect `findings`
and every rule's `status`.

This skill is also available as resource `linter://skills/software-design` and
prompt `software-design`. A plugin installation makes the skill discoverable
alongside the server; MCP resources alone do not install a client's local skill.

### CLI workflow

When the CLI is installed:

```sh
linter config rust                 # Inspect the embedded Rust preset.
linter init rust /path/to/project   # Create linter.toml; never overwrite one.
linter check /path/to/project --json
```

Choose `default`, `rust`, or `c`; `init` requires an explicit preset. From a source
checkout, use `cargo run --locked -p linter-cli --` before the same arguments.
Exit codes: `0` means no findings, `1` means findings, `2` means a configuration,
analysis, or output failure. A missing configuration currently yields
unconfigured rules: verify that `linter.toml` exists and the intended rules ran.
For Rust changes, run `cargo fmt --all` periodically; the linter is not rustfmt.

### Configuration contracts

Use `[[rules.<id>]]` assertion blocks with `target`, not `glob` or a nested
`config.layouts` table. Quote IDs containing punctuation. A target accepts a
root-relative glob or a nonempty list; `*` matches one path segment and `**`
crosses directories. File-oriented rules target files, layout structure targets
directories, and Rust layers target directories containing Cargo packages.

Start from generated presets instead of inventing a complete TOML file by hand.
The following examples are policy fragments to adapt or replace existing blocks,
not fragments to append blindly. Most matching assertions all apply independently.
Only layout permission blocks use last-match ordering. Unknown rules and invalid
settings fail even when a rule is disabled. Vocabulary belongs in configuration.
`[files].exclude` controls discovery; exclusions such as Git and build output come
from the preset, not hidden library policy.

### Encode structure and documentation

```toml
[[rules.layout]]
target = "packages/*"
mode = "restrictive"
files.required = ["Cargo.toml", "src/lib.rs"]
directories.allowed = [{ target = "tests", description = "Crate integration tests." }]

[[rules.layout]]
target = ["**/*.md", "**/*.markdown"]
allow = false
case_sensitive = false

[[rules.layout]]
target = "docs/*.md"
allow = true
description = "Project requirements and architectural decisions."

[[rules.layout]]
target = "skills/*/SKILL.md"
allow = true
description = "Installable agent workflows and their configuration guidance."
```

`permissive` permits extra entries; `restrictive` permits only required paths,
their parents, and explicitly allowed immediate entries. Required paths are
literal and relative to each matched directory. Keep repeated skeletons under
one category glob. Every permission allowance has a purpose description, and
later matching permissions win. An allowance never bypasses another rule.

Use `files.allow_empty`, `directories.allow_empty`, and
`directories.allow_single_file` only where those shapes express a real invariant.
For example, target source-tree parents when discouraging directories containing
one file; preserve intentional crate, fixture, and integration-test boundaries.
Keep unit tests in the source file owning the behavior. Use root `tests/` or a
crate-level `tests/` for integration tests. Adjust the preset's integration-test
allowance to actual crate categories; do not allow every `**/tests/**` path.

### Encode names independently

```toml
[[rules.filename]]
target = "**/src/**/*.rs"
kind = "file"
case = "snake_case"
max_words = 2
reject_numbered_fragments = true

[[rules.filename]]
target = "**/src/**"
kind = "directory"
max_words = 1

[[rules."forbidden-words"]]
target = "**/src/**"
words = ["common", "core", "helper", "helpers", "misc", "shared", "util", "utils"]

[[rules."shared-affix"]]
target = "**/src/**/*.rs"
max_prefix = 2
max_suffix = 2
```

`filename` checks spelling and word counts, not English noun semantics. Review
whether each name expresses ownership. `forbidden-words` checks all components
of each selected path, including ancestor directories. `shared-affix` flags the
third sibling sharing a first or last whole word: `func_a.rs`, `func_b.rs`, and
`func_c.rs` suggest `func/{a,b,c}.rs`. Confirm those files form a cohesive domain
before moving them. Use `redundant-parent-name` to catch repeated parent words.

### Encode dependency direction

```toml
[[rules."rust/layers"]]
name = "apps"
target = "apps/*"
dependencies = ["usecase", "packages"]

[[rules."rust/layers"]]
name = "usecase"
target = "usecase/*"
dependencies = ["usecase", "packages"]

[[rules."rust/layers"]]
name = "packages"
target = "packages/*"
dependencies = ["packages"]
```

Each analyzed Cargo package must match exactly one layer. A layer may depend on
itself only when its name appears in `dependencies`. Here apps cannot depend on
other apps, while packages may depend on peer packages. Five packages under
`packages/*` need one block; adding a sixth requires no policy change. Use
`packages/**` instead for nested crates, without retaining an overlapping layer.
These are Cargo dependency edges, including development, build, optional, and
target-specific local declarations; they are not inferred from `use` statements.
Add `rust/dependency-cycles` to reject cycles and `rust/dependency-budget` to
constrain an agreed package dependency budget. Do not confuse a budget with
ownership or remove necessary dependencies merely to meet an arbitrary number.

### Keep Rust logic shallow and bounded

```toml
[[rules."rust/file-length"]]
target = "**/*.rs"
max_lines = 500

[[rules."rust/function-length"]]
target = "**/*.rs"
max_lines = 50

[[rules."rust/method-length"]]
target = "**/*.rs"
max_lines = 50

[[rules."rust/max-indent"]]
target = "**/*.rs"
max_columns = 20

[[rules."rust/nesting"]]
target = "**/*.rs"
max_depth = 2
ignore_guard_clauses = true

[[rules."line-width"]]
target = "**/*.rs"
max_columns = 100
```

The free-function length rule exempts file-level `main` functions, including async
entrypoints: composition may be large. Helpers inside `main` retain their own
limits. This does not exempt the containing file from its production-line limit
or disable nesting and indentation checks.

The length rules exclude recognized test-only code under their default production
scope. A file with 500 production lines plus 500 test lines passes a 600-line
production limit. Rust indentation is relative to each function declaration;
enclosing `impl` and `mod` indentation does not consume its budget. Its 20-column
limit also checks function bodies in tests. Closures inside functions still
consume indentation. Generic `max-indent` measures physical file indentation;
use `rust/max-indent` for Rust. `line-width` still checks test code and comments.

Prefer guards and early returns when they preserve behavior and clarify the
happy path. Extract cohesive operations onto their actual owner or collection.
Do not split into numbered fragments or manufacture wrappers for one helper.
Nested callbacks still deserve review; a passing numerical budget is not proof
of a good design.

### Choose rules by the problem you are solving

Start with the matching preset and the repository's existing policy. Use this
map to identify useful checks; it is not an instruction to enable every check
or impose every convention on every project. Configure selectors, limits, API
lists, and ownership boundaries for the actual invariant. A registered rule
reported as `unconfigured` has not validated that invariant.

For a new repository, start with layout, names, and language-specific size limits.
For a growing Rust workspace, add Cargo layers and cycles, then model and behavior
ownership checks. For async services or native code, configure the relevant
runtime and safety boundaries. During refactoring, inspect related evidence
before deciding whether a finding justifies moving or combining code.

| When this is useful | Rules to consider | What to configure or inspect |
| --- | --- | --- |
| Establish directory skeletons, permitted documents, and test locations | `layout` | Required paths, restrictive directory categories, and ordered bans/allowances with purpose descriptions. Checks paths, not test dependencies or behavior. |
| Keep file and directory names concise | `filename` | Case, word/character limits, numbered fragments, and separate file/directory targets. Does not prove English noun meaning. |
| Remove vague path vocabulary | `forbidden-words` | Explicit banned words; selected paths include ancestor components. Does not scan Rust source vocabulary. |
| Recognize a missing containing module | `shared-affix`, `redundant-parent-name` | Repeated sibling prefix/suffix thresholds and repetition of parent words. Group only genuinely cohesive files. |
| Keep text readable | `line-width`, `max-indent` | Physical column limits. Prefer `rust/max-indent` for Rust so enclosing modules and impls do not count. |
| Protect Cargo package ownership | `rust/layers` | Package-directory selectors and allowed destination layers. Does not enforce dependencies between modules inside one crate. |
| Control dependency growth | `rust/dependency-cycles`, `rust/dependency-budget` | Cycles and agreed dependency budgets; keep necessary dependencies rather than hiding them to meet a count. |
| Bound production Rust size | `rust/file-length`, `rust/function-length`, `rust/method-length` | Separate file, free-function, and method limits. Recognized test-only code is excluded by default. |
| Flatten complicated Rust control flow | `rust/nesting`, `rust/max-indent` | Nesting depth, guard handling, and function-relative indentation. Consider early returns without changing behavior. |
| Keep traits focused | `rust/trait-method-count`, `rust/broad-trait-responsibilities` | Use the method cap by default. Broad-trait clustering is absent from presets and runs only with explicit project vocabulary; its lexical evidence is a heuristic. |
| Investigate a growing orchestration object | `rust/god-object-growth` | Looks for many methods, independent field capabilities, and a workflow crossing their ownership. Standard wrappers are resolved automatically; custom newtypes retain identity. A high method count alone is insufficient. |
| Improve Rust declaration names | `rust/struct-noun-naming`, `rust/struct-word-count`, `rust/module-name` | Word limits and configured naming vocabulary. Review domain meaning before shortening a name. |
| Remove repeated namespace words | `rust/redundant-module-prefix`, `rust/receiver-name-repetition` | Declarations repeating their module noun or methods repeating their receiver's name. Preserve necessary disambiguation. |
| Stop path attributes flattening several child domains | `rust/path-module-flattening` | Maximum child domains injected into one namespace. This is not a repository-escape check. |
| Replace closed primitive state with a model | `rust/string-backed-finite-state`, `rust/boolean-state-cluster` | Literal state comparisons and related boolean state. Preserve extensible protocol values, identifiers, and ordinary user text. |
| Investigate duplicated entity facts | `rust/duplicate-entity-base`, `rust/wire-domain-model-duplication` | Inspect identity, field evidence, and conversions. Different newtypes wrapping the same primitive remain different concepts. |
| Put behavior on its owner | `rust/free-function`, `rust/detached-constructor`, `rust/self-constructor-static` | Receiver-shaped operations, free factories, and constructors. Prefer an existing entity, collection, or standard conversion; do not invent a wrapper for one helper. |
| Review a helper with one caller | `rust/single-use-free-function` | Inline only when it improves the caller; preserve a meaningful algorithm or deliberate boundary with a justified exception. |
| Remove forwarding that adds no contract | `rust/redundant-accessor`, `rust/redundant-wrapper` | Equivalent access/forwarding contracts. Preserve validation, visibility, nominal identity, and external API guarantees. |
| Remove empty structural indirection | `rust/redundant-namespace`, `rust/redundant-marker`, `rust/empty-struct` | Transparent modules, unused empty traits, and fieldless structs are different checks. Empty structs can be valid typestate; never add dummy fields to silence a finding. |
| Avoid blocking async executors | `rust/async-blocking-operation` | Known blocking APIs, receiver tracking, Tokio worker callbacks, and guard adapters are built-in; configure targets, scope, and permitted files. An unrelated method with the same name is not proof. |
| Capture environment at composition | `rust/environment-variable-access` | Ambient APIs, permitted targets/modules, and optional lazy-global policy. Pass validated configuration to consumers. |
| Keep Rust unsafe operations at an explicit boundary | `rust/unsafe-boundary` | Allowed targets/modules and attached safety rationales. Does not replace compiler unsafe checks. |
| Remove provisional instrumentation | `rust/provisional-diagnostic`, `c/provisional-diagnostic` | Explicit comment terms such as temporary diagnostics; strings are not comments. |
| Bound C implementation size and depth | `c/file-length`, `c/function-length`, `c/nesting` | File/function limits and nested control-flow depth for selected C sources. |
| Keep C headers focused | `c/interface-breadth` | Maximum externally visible function declarations in selected headers. |
| Catch unchecked C operations | `c/unchecked-allocation`, `c/ignored-result` | Allocation and result handling; inspect the configured API assumptions and actual success/failure branches. |
| Enforce native API contracts | `c/forbidden-call`, `c/safety-rationale` | Explicit prohibited calls and rationale requirements for selected operations. |
| Find C tests that exercise unreachable production state | `c/test-only-state` | Test-hook macro names; reports production predicates whose known state writers are test-only. |
| Integrate native formatting and analyzers | `c/format`, `c/tidy`, `c/cppcheck` | Executable paths, formatting/check policy, and compilation databases for analyzers. Missing tools are errors; these checks do not edit source. |
| Validate example-document structure | `markdown/examples` | Title, case headings, and closed fences. Document permissions remain in `layout`. |

Use `linter config rust` or the MCP preset resource for actual starter settings;
use `c` or `default` for the other presets. Preserve an existing configuration
instead of overwriting it. The release plugin does not currently expose individual
rule READMEs through MCP; when a needed option is absent from these examples and
the preset, consult that rule's README in the matching repository version rather
than guessing its schema. Unknown settings are errors.

These are implemented rules, not promises of compiler-level analysis. The linter
does not expand Rust macros or replace compilation, tests, rustfmt, Clippy, or
language-specific tools. Process-execution boundaries, general public API type
leakage, ignored Rust results, and semantic test-dependency checks are not
implemented; do not claim that existing path or Cargo rules enforce them.

For string states, this is a finding even without a `state_words` name filter:

```rust
struct Upload { status: String }
impl Upload {
    fn finished(&self) -> bool {
        match self.status.as_str() {
            "preparing" | "pushing" => false,
            "pushed" => true,
            _ => false,
        }
    }
}
```

Model the closed vocabulary as an enum. A boundary parser returning a resolved
enum or `Option<Enum>` from incoming text is accepted. A string field remains
stored state even if one method converts it to an enum. Similarly,
`UserId(String)` and `OrderId(String)` are not the same entity merely because
their representation matches. Never merge distinct identities to satisfy a
model-duplication finding; investigate and fix an unsupported inference.

A justified exception uses a real rule ID and a concrete reason on the next item:

```rust
// linter:disable rust/free-function -- Uniform registration hook across language packages.
pub fn register(registry: Registry) -> Result<Registry, Error> {
    registry.register::<Layout>()
}
```

Use a directive only after inspecting ownership, callers, and the relevant
contract, and after any review required by the repository. Unknown, malformed,
and unused directives are errors. Do not use blanket suppressions or claim an
exception proves the design correct.


Entity-driven, OOP-leaning, composition-first design. Goal: rocksolid, maintainable, minimal code.
Every line is debt.
Folder structure documents the system.

- **Great architecture = add/remove without modify.**
But this has a price because this requires infinite abstraction. It should be paid when it is justified.

**It is extremly important to define folder structure that can be enforced by linter, and if job is done right, that linter will not have to change much in future.**
You have to figure out structure that will be extensible no matter the requirements.
When project is given or propmted by user, your goal is to estimate abstraction and that is even for future requests, find balance and design proper folder structure so we can always fit any logic into that regid structure without need to modify linter later on.

## How design software

This skill comes with the linter, your goal is to properly setup the linter first so that we enforce concrete structures.
Your goal first and formost is to always think about abstraction, desining proper amount of abstraction is necessity and should be always well defined upfront.

This involves actually following questions:

1) What layers and how generic they should be?
2) What is the lifecycle of the software?
3) How are error states handled?
4) What domains we need and how should be structured?
5) Is this going to be multiple services and how many binaries we need?
6) What parts of it should be monorepo, what make sense to split?
7) What external libraries we could use?
8) Is there existing software that is popular and we can exploit?
9) Do we know if microservice-like architecture should be in place?

Points below outline how to answer these.
---

## 1. Entity-driven, OOP-first.

- Model the smallest meaningful unit as an entity.
- Start with ENUMs wrapping strings, intgers, primitives it it make sense.
- So for example User mith have `kind` that might be enum. Each should be modeled.¨

## 2. Naming

- Short, elegant names. Demand it. Long type names are a smell.
- Multi-word type name = bad namespace or bad name. Fix the namespace first.
  - Bad: `ExchangePair` → Good: `exchange.Pair`
  - Bad: `book.BookShelf` → Good: `book.Shelf`
- Prefix only when the namespace is too dense to disambiguate. Otherwise prefer the package as the prefix.
- Entity + collection pairing is fine. `book.Book` + `book.Books`, `Service` + `Services`.
- Create a namespace only when multiple entities live together. A single type doesn't justify a package.

## 3. Decide generic nature

In general software is about desining the abstraction. Key is to classify things and make order to them.
Every line, module, package, needs to be classified and must have clearly defined boundaries of abstraction.

First key distinction is wether project is standalone library or something that should actually does something.
We will focus in this section on latter which actually might involve including writing libraries.

Onion style architecture:

Level 5, business logic only, main block:
- Everything starts in main block. Often you might have multiple main blocks, multiple binaries, multiple entry points.
- Entrypoint is subject of understanding env vars + flags.
- Entrypoint might be for small application everything, but has to be split when logic grows.
- Important is that everything should be in mainblock composed as lego.
- Goal of entrypoint is to configure and run whole app, but you dont separate from entrypoint until its clear and obvious it would grow too large.
- Point of entrypoint is usually to setup http servers or configure services that handle transport, or similar.
- If it is gui might involve seting up window or compose components.
- It might involve configuring database adapter and composing concrete implementations that might be passed trough abstractions.
- Usually it make sense to setup something like apps/ cmd/ or bin/ or something of that nature.

Level 4, public surface, transport and apis:
- Depending on application, we might need to have public surface or api access
- In case of multiple services each service might have public api access or trasnport.
- Think of http, grpc, jsonrpc and similar as API access its no different than including something as dependency into project and calling its functions that are public.
- Transport such as http really is just way to define communication that can be serialized.
- Imporatant not is that if you have microservices, it just might be the case where service has direct API access and you use that in other services by including them as dependency.
- Public API surface should encapsulate service needs and should expose things that are needed for communication.
- In any case it is very important to always generate client. Usually OpenAPI, or protobufs are the answer.
- API surface should be thin, most of business logic should be centered around domain, usecases or other structures.
- This is mostly applicable for servers and backends.
- Layer 4 is usually significant to have its own dedicated api folder.

- **Routes: plural nouns.** `/events`, `/markets`, `/orders`.
- **Avoid verbs in routes.** Prefer `POST /calculation` over `/calculate`. CRUD covers the majority.
- **Verb exceptions exist** but are rare: `/auth/{in,out,register}` is acceptable.
- **RPC isn't always wrong**, but if used, lift it to the protocol layer (e.g., JSON-RPC) rather than scattering verb routes.

Level 3, services and usecases:
- Usually you ned to encapsulate multiple models and domains together to create somethign useful, this is where to do it.
- Service MUST contain work with domain.
- If you build generic math package that is not service nor usecase.
- Goal of usecases is to design abstraction and do composition around specific domain, that often involves saving models, working with multiple models and composing them together.
- Dedicated folder such as usecase/ service/ in cocnrete domain or package is usually good idea.
- Usecases are concrete and go after concrete implementation or specific goal.

Level 2, domains:
- Domain entities are single biggest thing that is hard to design and must be done really well and granullar.
- Domain entities are something that you will have to design around business rquirements.
- Examples of such a thing might be: User, Token, Company, Book etc.
- These singular nouns are often represented by database.
- It is often the case database might be source of truth and helps with generation of these entities.
- These domain entities are often exposed to API directly and it is often the case that Request/Response from transport is involving these.
- It't very important to however understand what storage and entitiy is, they are not same.

Domain entitiy is core entity tha tis internally used and might be expose publicly.
This entitiy might be same as database model and represent both, but database is storage.
Any storage has goal of representing concrete entitiy in database and its goal is to take and produce domain entitiy.
Entitiy -> Database Model -> Storage -> Database Model -> Entity OR Entity -> Storage -> Entity if storage layer allows that.

If managable often database can be source of truth for entities. This by degree speeds up development a lot.

Level 1, packages:
- Packages, libs, std lib etc are most generic and hence layer 1. These involves extensions to standart library
- However these can also involve http clients or specific functionality that is business realted
- Think of it this way if you can take package and transfer it to another project and could be used in 90% of projects that do something around that domain is valid
- Examples might be for example http client for stripe, or library doing jwt, or logging library, or http client that implements retry
- Think of standart libraries + little of the business but just so little it can be trasnfered still anywhere and be useful.
- These packages do not ever depend on the app, only on each other, and that is rarely.
-

It is important at very beggining to setup this folder structure and ensure that linter forces depndency flow:

Level 5: cannot depend on other entrypoints but might really include anything.
Level 4: can depend on entry except entrypoints.
Level 3: cannot depend on api or main blocks, only can import packages and entities and storage.
Level 2: can import packages but nothing else.
Level 1: only can import other pacakges.

Example of webapp:

Packages/
    jwt
    httpio
        server
            middlewares
        client
            client <- defines simple http client interface
            retriable client <- defines wrapper around
App/
    cmd/ <- handle binaries
        webapp <- implements specific web interface
    api/
        http/
            users <- handlers and usecase access
        direct/
            ....
    models/
        User <- entitiy + might have storage too
        Org
    usecase/
        users <- composes org and user together


- **Follow language conventions.** Rust: `src/bin/`. Go: `cmd/`. Don't fight idioms.
- **Group by domain**, where a domain is multiple tightly coupled entities (e.g., `identity`, `payments`, `markets`).
- **`lib/` may contain things domain uses.** What's forbidden is `lib/` depending on `domain/`.
- Dependency direction must stay clean: `cmd → usecase → domain → lib`. Never the other way.
- **No `helpers/` or `utils/` dumping grounds.** That's "logic without a classification" — all-catch-garbage.
  - Worst: `utils/utils.sin()`
  - Better: `utils/math.sin()`
  - Best: `lib/math.sin()`


### Domain design

Goal of the domain design is to figure out how large single service should be.
For example if you have authentiaction together mixed with notifications that might be fine as long as it is just this alone.
However if more functionality starts accumulating it make sense to split on:

identity/ -> service
    bin/
    ...
notifications/ -> service
    bin/
    api/...

It is very important to manange dependencies between these services. Say in this case we need to define that identity is center and
rest is depending on identity, circular dependency is what we want to avoid.

Domain should group related things, so for example identity, might deal with authentication, authorization, 2fa, password recovery.


### Database design

By default we should assume that each service eventually will have its own database.
Its a good practice therefore prefix each table with its domain.

```text
DB tables:                 Code:
  identity_user        →    identity/user/{entity, collection, storage}
  identity_picture     →    identity/picture/...
  identity_token       →    identity/token/...
  identity_twofa       →    identity/twofa/...
```

You can see service identitiy with models User, Picture, Token, TFA etc.
Singular names in database as always.

If you need to combine multiple models together its often good practice instead of joins to build view in database.
View itself becomes new model.

Prefered is <domain>_<singular noun>.

- **Each entity gets:** the entity itself, a collection/list, a storage layer.
- **Compose views from joins.** `identity.Profile` = `user` + `picture`. DB is source of truth; generate models from schema where possible.
- **Use code generators** for json → struct, schema → types, OpenAPI → client. Don't hand-roll what tooling does better.

### Function constructs

Don't create a function for one caller. A one-use function is usually deletable inline. Extract only when **at least one** holds:

1. Two or more call sites exist.
2. It makes sense as a static constructor (e.g., `Server.from_config_path` → `Server.from_config` → `Server{...}`).
3. It's an optimization or isolated piece of logic worth testing on its own.

- **Sweet spot: ~5–50 LOC.**
- **Less than ~5:** usually better inlined or duplicated.
- **More than ~50:** usually a domain modeling problem — the function is doing too many things, or the domain split is wrong. (`main`/composition roots are the exception.)

### Static constructors

Layer them. Each level adds one concern:

```
Server.from_config_path(path)   → reads file, calls
Server.from_config(cfg)          → validates, calls
Server { ... }                   → raw struct literal
```

## Composition patterns

- **Find shared traits across levels.** Often `Services` as list of `Service`, and `Process` as child of `Service` all need `log_to_file`. Define one tiny trait/interface so `concat(Services)`, `concat([Service, Process])`, etc. all work. This kind of abstraction is hard to spot and usually 1–2 function signatures wide.
- **Smallest-possible interfaces win.** Go's `io.Reader` / `io.Writer` are the standard to aim for. One method, universal applicability.
- **Lean on well-known interfaces** (`fmt`, `io`, iteration protocols) before inventing new ones.
- **Collections encapsulate behavior.** A `Books` type can sort, filter, paginate, and persist. Don't scatter that logic into free functions.

## Frontend / UI design

Same problem, different surface. Distinguish:

- **Generic components** (`Button`, `Input`, `Modal`) — reusable, no domain knowledge.
- **Domain/page components** (`HomepageHero`, `OrderConfirmation`) — composed of generics, carry domain meaning.

Draw the line early. Generic components stay in something like `components/ui/`; domain components live with the feature/page they belong to.

Often here wins following:
```
/src
  /pages
    /home
      _components
      page.tsx
    /another_page
      page.tsx
  _components/
    generic
    modal
    button
/eslint
```

Its good practice to force singular names, and underscore. page, layout, button, chart, card.
Usually any more complicated name means folder for namespacing.

## Comments and documentation

- **Comments should be short and only mark what's important.** Why, not what.
- **Folder structure + filenames carry most of the documentation.** Good layout reduces the comment burden.


## PG often is enough

- Job queues over listen notify
- pgvector, timescale, pgstats all sort of good extensins
- Don't be afraid to use functions
- Testing againt pg can be done using transactions. Open tx, run what is needed and rollback in every test.

Following pattern is often useful:
```
func begin(conn, fn) {
  if conn == tx {retrun fn(conn)} else {
    begin tx
    try: fn(tx); commit;
    except: rollback
  }
}

func c(x) {
  begin(x, () => ...)
}

func a(x) {
  begin(x, c)
}

call c or a - both will run just one tx
```

## Usually better to avoid state

Keeping thing stateless is usually good practice. Avoid in memory/file state. Better to use db.
This has serval asterisks, but generally applies.

## Think of go context.Context

Very useful in practice to clean things up to carry tracing ids etc. If serverside is being build good to have and use
if there are alternatives to go context use it.

## Cloud vs Local

This is biggest distinction. Cloud is often centered around databases and http transport.
While local apps are often centered around GUI and UIs.
That means local apps can take many of these concepts under their wings.
In any case your goal is to judge user request, design propriet architecture, consult that with user and setup linter properly for that usecase.

## Abstraction

You have to judge if things make sense to abstract. Sometimes it make sense sometime doesnt.
You have to make judgemnt call, let me give you example: storage.

Should I create repositories so I can store entity or should i store directly?
And it boils down to this, do we know if second storage is necessary?
Usually just one storage is needed. In case of external usage (aka building lib) this might be concern.
Can I test functionality without replacing underlaying implementation? In case of pg, I can create transaction and safely test.

Both testing and need for single DB are good enough arguments to not implement traits repositories and use pg to store things directly.

However if you decided to do so:

user/
    entity.rs
    storage.rs <- would implement interface
    storage_pg.rs <- would implement that storage interface or directory storage_pg/...

## Force markdown format

If you decide you need markdown you should absolutly question if you are trully in need of it and if it is something human will attempt to read.
Humans are lazy and dont read. You have to assume its so important that it would be huge issue not to have docs.

Another thing is means of documentation that might be source of truth for html later on.
But keep in mind every line is burder and you have to eliminate as much as possible.

It might be good practice to for example for modules, services etc force specific format of markdown and require that all details are in single document per service.

## Tooling

Its not an issue to take detour and build tooling you might need for future. For example codegeneration, directory scaffolding can be very
powerful for everyone.

## Errors

Its been expirience that errors needs to be handled properly and can easily grow into problem.
You should define and categorize errors, keep reason, + kind. Kind Should be for example GPRC class errors.
Like IO, data, conflict, not found etc.

Depending on usecase in majority this works, such a error should have mapping on http codes and server should be adjusted to handle that.
