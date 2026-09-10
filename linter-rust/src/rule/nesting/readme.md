# rust/nesting

Limits control-flow nesting inside Rust functions and methods using the shared AST. Each finding identifies the function and deepest construct. Exactly the configured depth passes; exceeding it is an error.

```toml
[[rules."rust/nesting"]]
target = "**/src/**/*.rs"
exclude = "generated/**/*.rs"
max_depth = 2
ignore_guard_clauses = true
scope = "production"
```

`target` is required and accepts a glob or nonempty list. `exclude` accepts the same syntax. `max_depth` defaults to two and must be positive. Scope accepts `production` (default), `tests`, or `all`, sharing the file-length test-only and integration-source classification. Unknown fields fail configuration validation.

With `ignore_guard_clauses = true`, statement-position `if` and `match`, loops, and async blocks count. Branches computing a bound value do not add a level, but statements inside their blocks still count. An `else if` chain stays on one level. Braced closures carry an additional level into nested control flow, but straight-line closure bodies do not create findings themselves. Expression-only predicate closures add no level.

Depth starts at zero inside each function body. Enclosing `impl`, trait, and
`mod` blocks never increase a function's depth. Physical indentation is measured
by a separate rule.

An `if` whose then branch exits, and whose else branch is absent or also exits, is a guard clause and adds no level. Recognized exits include return, break, continue, `panic!`, `unreachable!`, `todo!`, `unimplemented!`, and branches whose paths all exit. A break must leave the guard branch: breaking an inner labeled block does not
qualify. Returns inside deferred closures/async blocks and breaks inside local
loops do not prove that their enclosing branch exits. Qualified exit macros must
use `std` or `core`; visible macro/import shadowing prevents an assumed exit.
Its body is still inspected: a nested match or loop can exceed the budget even when the outer guard does not count.

```rust
fn walk(rows: &[Row]) {
    for row in rows {
        if row.invalid() {
            continue;
        }
        row.process();
    }
}
```

This reaches depth one with guard handling enabled, and depth two in strict mode.

Set `ignore_guard_clauses = false` for strict structural counting: value branches and every closure also count, guard clauses add a level, and condition/scrutinee expressions are visited inside their construct's level. Else-if chains still share a level. Nested function declarations have independent budgets rather than inheriting the surrounding function's depth.

Migration preserves Husklet's `maximum-nesting` guard-aware behavior and its twenty regression cases. Strict mode preserves Prop/Payment-SDK `deep-control-flow` structural checks; `scope = "all"` reproduces their test-code inclusion. Findings are errors instead of the old Prop warning. Trait default bodies are additionally checked. Test-only nested items are excluded consistently in production scope. No full-file secondary parser is used.

Diagnostics include the exact deepest construct as related evidence. An
early-return suggestion is only offered for a final, non-pattern `if` without an
`else` in a unit-returning function, when the excess nesting occurs inside it.
There are no later statements for the new return to skip. For example:

```rust
fn process() {
    if ready() {
        if valid() {
            if enabled() {
                work();
            }
        }
    }
}
```

A guard rewrite can keep this path shallow:

```rust
fn process() {
    if !ready() { return; }
    if !valid() { return; }
    if enabled() { work(); }
}
```

No rewrite is applied automatically. Other findings recommend reviewing control
flow without claiming that a particular return is safe. The analysis does not
perform macro expansion or prove imported macros' behavior across files.
