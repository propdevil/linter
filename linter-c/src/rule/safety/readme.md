# c/safety-rationale

Requires an immediately attached, nonempty `SAFETY:` comment for configured
C calls. The rationale names a pointer, bounds, lifetime, ownership or concurrency
invariant. Each configured call lacking a rationale produces an error.

```toml
[[rules."c/safety-rationale"]]
target = ["native/**/*.c", "native/**/*.h"]
exclude = "native/generated/**"
operations = ["copy_bytes", "map_unchecked"]
```

`target` is required; `exclude` optional. Both accept a root-relative pattern or
nonempty list. `operations` is a required nonempty list of exact C identifiers.
There is no embedded operation vocabulary. Missing blocks are unconfigured.

```c
// SAFETY: The source and destination each cover eight valid bytes.
copy_bytes(destination, source, 8);
int result = /* SAFETY: Mapping bounds were validated. */ map_unchecked();
```

A contiguous comment block immediately before the call or its containing
statement/declaration can supply the rationale. Line and block comments, multiline
explanations, and same-line comments before the call are accepted. Blank lines or
intervening code break attachment. An empty marker or closing `*/` alone is not
an explanation. Strings and other non-comment syntax never supply rationale.
Only direct identifier calls are matched; function aliases/macros are not resolved.
All syntax and comments come from the existing shared C parse.

Migration: preserves Husklet `rule/c/safety.rs` and `safety_test.rs` configured-call,
attached rationale, detached/empty rationale and suppression cases. Replaces
line-prefix comment guesses with real comment nodes; extends same-line and
multiline rationales. Current reasoned directives are supported, including unused
directive errors. Old `hl-lint` spelling is not recognized.
