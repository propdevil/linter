# c/forbidden-call

Rejects explicitly configured direct C calls. The engine bans no APIs by default;
projects own the function vocabulary, rationale and corrective instruction.
Each matching configuration block produces a finding identified by its index.

```toml
[[rules."c/forbidden-call"]]
target = "native/**/*.c"
exclude = "native/compatibility/**"
functions = ["system", "popen"]
description = "Shell execution bypasses the explicit process boundary."
instruction = "Launch the process with an explicit executable and argv vector."
```

`target`, nonempty `functions`, `description`, and `instruction` are required.
`exclude` is optional; selectors accept a pattern or nonempty list. Functions must
be exact C identifiers. Description and instruction cannot be whitespace-only.
Missing blocks are unconfigured. No directory exceptions are embedded in code.

Only identifier-designated call expressions match. `system(...)` matches the
example; `subsystem(...)`, `object.system(...)`, `(*system)(...)` and macro
replacement text do not. Parenthesized function designators and pointer aliases
are not resolved. Strings/comments do not manufacture calls or directives.
Nested direct calls are checked independently using shared C syntax.

```c
// linter:disable c/forbidden-call -- The compatibility launcher exposes no argv API.
system(command);
```

Migration: replaces Husklet `rule/c/policy.rs` and `policy_test.rs`. Preserves
configurable API policy, exact call positions, syntax exclusions and opt-in
ambient-environment bans. Replaces dynamic subrule IDs with `c/forbidden-call`
and configuration indices. Current directives apply to their attached syntax
subtree; old single-finding suppression restrictions and `hl-lint` spelling are
replaced by the common directive contract. Unknown, malformed and unused
directives remain errors.
