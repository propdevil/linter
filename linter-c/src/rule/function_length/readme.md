# c/function-length

Limits each C function definition to effective physical code lines. Signatures
and braces count; blank and comment-only lines do not. Inline comments do not
remove their surrounding code. Strings containing comment syntax remain code.
A function with exactly the configured limit passes. Prototypes are skipped.

```toml
[[rules."c/function-length"]]
target = ["native/**/*.c", "native/**/*.h"]
exclude = "native/generated/**"
max_lines = 200
```

`target` is required; `exclude` is optional. Both accept a string or a nonempty
list of root-relative patterns. `max_lines` defaults to 200 and must be positive.
Each oversized function produces an error containing its starting line, actual
count and limit. Multiple matching blocks apply independently. Shared C analysis
parses each discovered source once; this rule never reparses source.

Migration: preserves the function-length subdiagnostic of Husklet
`rule/c/structure.rs` and its budget, comment/string/prototype regression cases.
File length and nesting remain separate rules. Historical `hl-lint` suppression
comments are not interpreted here; common directive handling is owned by the
linter integration layer.
