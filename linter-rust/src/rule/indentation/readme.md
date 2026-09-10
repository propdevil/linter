# rust/max-indent

Limits indentation inside Rust function and method bodies. Each function gets
its own baseline: the leading whitespace of its declaration. With four-space
formatting, an ordinary body statement uses four relative columns. Enclosing
`mod`, `impl`, and trait indentation does not consume the budget.

```toml
[[rules."rust/max-indent"]]
target = "**/*.rs"
max_columns = 20
tab_width = 4
```

`target` is required; optional `exclude` uses the same root-relative selectors.
Limits must be positive. Defaults are 20 columns and four-column tab stops.
Exactly the limit passes. Inline tests and trait default methods are checked.
Nested functions start independent budgets; closures and async blocks inside a
function retain its budget. This prevents hiding indentation in callbacks.

Signatures, declarations outside functions, blank lines, comments, and multiline
string contents are excluded. A string's opening line counts as an expression.
Wrapped expressions still count: use intermediate values when an
expression becomes difficult to follow. Checks use parsed Rust, not brace counts
inside fixture text. Each function reports one finding at its deepest excessive
indentation, with related source evidence. Reasoned function directives apply.

```rust
fn run(ready: bool, enabled: bool) {
    if ready {
        if enabled {
            work(); // Twelve relative columns.
        }
    }
}
```

With `max_columns = 8`, this fails. When skipping the function has the same
meaning, guard clauses make the path shallow:

```rust
fn run(ready: bool, enabled: bool) {
    if !ready { return; }
    if !enabled { return; }
    work();
}
```

`rust/nesting` separately measures control flow independent of formatting and
recognizes terminating guards. Neither rule rewrites control flow automatically.
The generic `max-indent` remains an absolute text rule for other languages;
Rust presets use this rule instead.
