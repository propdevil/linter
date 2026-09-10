# c/ignored-result

Reports configured calls whose return value is discarded as an expression
statement. Parentheses and casts, including `(void)`, do not bypass this rule.
Assigned, returned, conditional, and argument values are consumed and pass.
Only the outer discarded call is reported; `consume(open_resource())` passes.

```toml
[[rules."c/ignored-result"]]
target = ["native/**/*.c", "native/**/*.h"]
exclude = "native/generated/**"
functions = ["open_resource", "flush_output"]
```

`target` is required and `exclude` optional; both accept a pattern or nonempty
list. `functions` is a required nonempty list of exact C identifiers. No function
names are embedded as policy. Missing blocks are unconfigured. Existing project
exclusions apply before shared C parsing.

```c
int handle = open_resource(); // Result retained.
(void)open_resource();        // Error: still discarded.
// linter:disable c/ignored-result -- Best-effort shutdown cleanup.
flush_output();
```

Direct identifier designators, parentheses and explicit `(*function)()` spelling
are recognized. Function-pointer aliases and macro expansion are not resolved;
configure the actual called spelling when appropriate. Comments and string text
never establish calls. This is a syntactic policy, not compiler type resolution.

Migration: preserves Husklet `rule/c/result.rs` and `result_test.rs` direct-call,
parenthesis, cast, consumed-value and reasoned-suppression cases. Replaces its
implicit `(void)` exemption with an error requiring an explicit reasoned directive.
Unused directives are reported by the common directive engine.
