# c/nesting

Limits the deepest control-flow nesting in each C function definition. Counts
`if`, `switch`, `for`, `while`, and `do` constructs. An `else if` chain remains
one level; an `if` nested inside an `else` block adds a level. Braces, declarations,
preprocessor conditionals, comments and strings do not add levels.

```toml
[[rules."c/nesting"]]
target = ["native/**/*.c", "native/**/*.h"]
exclude = "native/generated/**"
max_depth = 6
```

`target` is required; `exclude` is optional. Both accept a pattern or nonempty
list of root-relative patterns. `max_depth` defaults to 6 and must be positive.
Exactly six levels pass; seven produce one error per function and matching block,
including the function's starting line and its maximum measured depth. Sibling
branches do not accumulate depth. Parsing is shared across all C rules.

Migration: preserves `maximum_nesting` from Husklet `rule/c/structure.rs`, including
its control-flow and else-if regression cases. File/function length are separate
rules. Early exits can reduce nesting but this C rule does not exempt guards from
structural counting. Historical `hl-lint` comments are not interpreted here;
common directive handling belongs to the linter integration layer.
