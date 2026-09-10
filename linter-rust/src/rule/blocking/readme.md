# rust/async-blocking-operation

Reject configured synchronous operations in async functions, blocks, and closures. Also reject a known synchronous lock guard retained across an `await`, with evidence pointing to its acquisition. Blocking work belongs in an explicit worker callback; release synchronous guards before suspension.

The rule uses the shared Rust AST. Its API vocabulary is entirely configuration: an empty rule list disables it, and an assertion with neither functions nor methods is invalid.

```toml
[[rules."rust/async-blocking-operation"]]
target = ["apps/**/*.rs", "linter*/src/**/*.rs"]
exclude = "**/fixtures/**"
scope = "production"
blocking_functions = [
    "std::thread::sleep",
    "std::fs::read", "std::fs::read_to_string", "std::fs::write",
    "std::fs::File::open", "std::fs::File::create",
]
blocking_contexts = ["tokio::task::spawn_blocking", "tokio::task::block_in_place"]
guard_adapters = ["unwrap", "expect"]
blocking_methods = [
    { receiver = "std::process::Command", methods = ["spawn", "status", "output"], constructors = ["std::process::Command::new"], fluent_methods = ["arg", "args"] },
    { receiver = "std::fs::OpenOptions", methods = ["open"], constructors = ["std::fs::OpenOptions::new"], fluent_methods = ["read", "write", "create"] },
    { receiver = "std::sync::Mutex", methods = ["lock"], constructors = ["std::sync::Mutex::new"], returns_guard = true },
    { receiver = "std::sync::RwLock", methods = ["read", "write"], constructors = ["std::sync::RwLock::new"], returns_guard = true },
    { receiver = "parking_lot::Mutex", methods = ["lock"], constructors = ["parking_lot::Mutex::new"], returns_guard = true },
    { receiver = "tokio::sync::Mutex", methods = ["blocking_lock"], constructors = ["tokio::sync::Mutex::new"] },
    { receiver = "tokio::sync::mpsc::Receiver", methods = ["blocking_recv"] },
]
```

This explicit starter policy is exercised by the rule's Registry tests. Extend its exact function paths for other filesystem operations or blocking libraries; there is no implicit API catalogue. `target`, `exclude`, and optional `allowed_targets` accept one glob or a list. Allowed targets designate complete files permitted to contain blocking operations. `scope` is `production` (default), `tests`, or `all`. Unknown settings and malformed API identifiers fail configuration.

`receiver` is an exact imported or qualified type. `constructors` establish that receiver type for an inferred local; `fluent_methods` preserve it through known builder calls. All names in `methods` are blocking operations. Set `returns_guard` only when those methods acquire a synchronous guard, including a result wrapping one. `guard_adapters` identifies methods that preserve this proven acquisition through result handling; it never makes an unrelated `.unwrap()` into a lock acquisition. Parentheses and `?` preserve acquisition provenance.

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

Migrates the blocking rules from [archived source](https://github.com/propdevil/linter/tree/586162b6ca69190f6138a03ab1a7e7df1bab26dd/sources/husklet/src/packages/hl-design-lint/src/rule/rust/blocking/), [archived source](https://github.com/propdevil/linter/tree/586162b6ca69190f6138a03ab1a7e7df1bab26dd/sources/prop/packages/design-lint/src/rule/blocking/), and [archived source](https://github.com/propdevil/linter/tree/586162b6ca69190f6138a03ab1a7e7df1bab26dd/sources/payment-sdk/packages/design-lint/src/rule/adopted/blocking/). Their async API, worker boundary, storage adapter, and test-only regressions are retained. Guard checks additionally cover lexical lifetime without later use, alias moves, and shadowing; API lists and worker boundaries are now explicit configuration.
