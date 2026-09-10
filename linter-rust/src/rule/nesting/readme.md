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

An `if` whose then branch exits, and whose else branch is absent or also exits, is a guard clause and adds no level. Recognized exits include return, break, continue, `panic!`, `unreachable!`, `todo!`, `unimplemented!`, and branches whose paths all exit. Its body is still inspected: a nested match or loop can exceed the budget even when the outer guard does not count.

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
