# rust/provisional-diagnostic

Rejects actual source comments containing every configured term. Retains the
source rule's ASCII-insensitive substring matching: `diagnostic` also matches
`diagnostics`. Words in strings and ordinary code do not count.

## Configuration

```toml
[[rules."rust/provisional-diagnostic"]]
target = "**/*.rs"
terms = ["temporary", "diagnostic"]
```

Terms are explicit, nonempty and unique after ASCII case folding. Optional
`exclude` narrows the selection. All comments, including test-code comments, are
checked. A block comment can supply its terms across several lines. The shared
language AST is reused. Assembly is not parsed or claimed as covered.

## Examples

`// Temporary child diagnostics.` fails. A comment about a temporary directory,
a separate comment about bounded diagnostics, and a string containing both terms
pass. A reasoned directive before the comment and its attached code item can
record a narrow exception; unused directives still fail.
