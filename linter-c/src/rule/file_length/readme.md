# c/file-length

Limits selected C files and headers by effective physical code lines. Blank and
comment-only lines do not count. Signatures, braces, preprocessor directives,
prototypes and string literals count; inline comments leave their code counted.
The rule reports one error per oversized file and matching configuration block.

```toml
[[rules."c/file-length"]]
target = ["native/**/*.c", "native/**/*.h"]
exclude = "native/generated/**"
max_lines = 1500
```

`target` is required and `exclude` is optional; each accepts one root-relative
pattern or a nonempty list. `max_lines` defaults to 1500 and must be positive.
Exactly 1500 effective lines pass; 1501 fail. Project exclusions apply before
analysis. CRLF and LF have the same counting semantics. Comment recognition
uses the shared C syntax tree, so comment markers inside strings remain code.

Migration: preserves Husklet `rule/c/structure.rs`'s file-length diagnostic and
default threshold. Function length and control-flow nesting are separate rules.
Reuses the same comment masking as `c/function-length`, without reparsing C.
Historical `hl-lint` comments are not interpreted by this rule; common directive
handling belongs to the linter integration layer.
