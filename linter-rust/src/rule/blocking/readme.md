# rust/async-blocking-operation

Reject known synchronous operations in async functions, blocks, and closures. Also reject a known synchronous lock guard retained across an `await`, with evidence pointing to its acquisition. Blocking work belongs in an explicit worker callback; release synchronous guards before suspension.

The rule uses the shared Rust AST. Supported API behavior is built into the analyzer. A target-only assertion enables the check; without assertions it reports unconfigured.

```toml
[[rules."rust/async-blocking-operation"]]
target = ["apps/**/*.rs", "linter*/src/**/*.rs"]
exclude = "**/fixtures/**"
scope = "production"
```

`target`, `exclude`, and optional `allowed_targets` accept one glob or a list.
Allowed targets permit blocking operations in whole files. `scope` is
`production` (default), `tests`, or `all`. Unknown settings fail configuration,
including the removed `blocking_functions`, `blocking_methods`,
`blocking_contexts`, and `guard_adapters` options.

Built-in operations are `std::thread::sleep`, `std::fs::{read, read_to_string,
write}`, `std::fs::File::{open, create}`, `Command::{spawn, status, output}`,
`OpenOptions::open`, standard Mutex/RwLock acquisition, `parking_lot::Mutex::lock`,
Tokio `Mutex::blocking_lock`, and Tokio mpsc `Receiver::blocking_recv`.
Constructors identify known receiver types. Command `arg`/`args` and OpenOptions
`read`/`write`/`create` preserve builder identity. Other APIs and builder methods
are not claimed as covered.

Tokio `spawn_blocking` and `block_in_place` callbacks are worker boundaries.
`unwrap` and `expect` preserve a proven standard lock acquisition through result
handling; an unrelated `.unwrap()` never establishes a guard. Parentheses and
`?` preserve acquisition provenance. Standard Mutex/RwLock and parking_lot Mutex
guards are tracked across await. Custom API lists are not configurable.

```rust
use std::sync::Mutex;
async fn bad(lock: &Mutex<u8>) {
    let guard = lock.lock().unwrap(); // Blocking acquisition.
    ready().await; // Guard remains alive, even when not used afterward.
    consume(guard);
}

async fn good() {
    tokio::task::spawn_blocking(|| std::fs::read("input")).await;
}
```

Worker boundaries exempt only callback bodies. For `spawn_blocking({ read_input(); || work() })`, `read_input()` still executes in the caller and remains checked. A nested async body is checked independently; an ordinary nested function is not assumed to execute on its enclosing async caller.

Imports, aliases, grouped imports, typed parameters, constructor-created locals, and simple local shadowing are tracked. Custom methods named `lock`, `read`, or `output` are not enough to produce a finding. A guard stops belonging to the caller after explicit drop, passing its value to a direct function, replacement, or lexical scope exit. Simple aliases transfer ownership. Shadowing does not destroy an old guard. Both conditional branches must release a guard to prove release; loops retain possible live guards. A captured guard is checked inside an async body; an unrelated guard outside that body is not automatically considered captured.

This is bounded syntax analysis, not Rust type checking or whole-program execution analysis. It does not resolve macro expansions, cross-file reexports, arbitrary field receiver types, destructuring, container-held guards, or general interprocedural effects. Unknown receiver identities remain unclassified. Directives use the common engine and require a concrete reason:

```rust
// linter:disable rust/async-blocking-operation -- Bounded compatibility call during isolated startup.
std::fs::read("startup");
```

Migrates the blocking rules from [archived source](https://github.com/propdevil/linter/tree/586162b6ca69190f6138a03ab1a7e7df1bab26dd/sources/husklet/src/packages/hl-design-lint/src/rule/rust/blocking/), [archived source](https://github.com/propdevil/linter/tree/586162b6ca69190f6138a03ab1a7e7df1bab26dd/sources/prop/packages/design-lint/src/rule/blocking/), and [archived source](https://github.com/propdevil/linter/tree/586162b6ca69190f6138a03ab1a7e7df1bab26dd/sources/payment-sdk/packages/design-lint/src/rule/adopted/blocking/). Their async API, worker boundary, storage adapter, and test-only regressions are retained. Guard checks additionally cover lexical lifetime without later use, alias moves, and shadowing; known API behavior and worker boundaries are now built-in; repository scope remains configurable.
