# rust/path-module-flattening

Reports explicit `#[path]` module declarations injecting more than the configured
number of child directories into one namespace. Multiple files from one child
directory count as one domain. Separate inline namespaces are checked separately.

```toml
[[rules."rust/path-module-flattening"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
max_child_domains = 1
```

`target` is required; `exclude` optional. Selectors accept one root-relative pattern
or nonempty list. `scope` accepts `production` (default), `tests`, or `all`.
`max_child_domains` defaults to one and must be positive. Missing blocks are
unconfigured. Platform-conditioned modules retain the source rule's exemption;
unknown feature flags do not count as platform conditions.

```rust
#[path = "registry/state.rs"] mod state;
#[path = "registry/snapshot.rs"] mod snapshot; // Still one child domain.
#[path = "signal/plan.rs"] mod signal_plan;   // Second domain: error.
```

Each namespace finding lists domains and evidence for all counted declarations.
Attach an intentional exception directive to the first counted module:

```rust
// linter:disable rust/path-module-flattening -- Generated facade preserves a fixed external contract.
#[path = "registry/state.rs"] mod state;
#[path = "signal/plan.rs"] mod signal_plan;
```

Literal paths resolve lexically against the source directory and inline-module
context, following [Rust's module path rules](https://doc.rust-lang.org/stable/reference/items/modules.html#the-path-attribute).
`.` and `..` normalize before domain comparison. Parent escapes, absolute paths
and same-directory files do not introduce child domains. Inline explicit paths
replace the inline directory as Rust specifies. File existence and symlink
canonicalization are not part of this rule; it analyzes declarations without
following targets. Macro-expanded and `cfg_attr`-generated paths are not resolved.

Migration: replaces Husklet `rule/rust/boundary/mod.rs` and its `test.rs`.
Preserves its multiple-domain, same-domain, production-test and platform cases.
Extends configurable limits, per-inline-namespace checks, normalized path evidence
and current directives. The original top-level check used the first raw path
component; this rule avoids treating `.` or `..` as child domain names.
