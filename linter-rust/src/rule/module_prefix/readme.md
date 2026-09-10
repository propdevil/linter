# rust/redundant-module-prefix

```toml
[[rules."rust/redundant-module-prefix"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
ignored_names = []
```

Flags structs, enums, traits, free functions, and trait declarations' functions that repeat a complete module name as their semantic prefix. In `launcher/plan.rs`, `LauncherPlan` repeats ancestor launcher and `PlanSpec` repeats current module plan. `PlanetSpec` does not repeat plan. Inline modules add their own namespace. Acronyms and case boundaries are normalized; multiword module names must match completely.

Implementation methods are excluded because their receiver/trait owns that namespace; `rust/receiver-name-repetition` handles receiver names. Import aliases do not rename declaration ownership. A declaration exactly equal to its module name is not shortened to an empty identifier.

Declared external modules and literal path overrides connect discovered source files to their namespace. For example `#[path="other.rs"] mod launcher;` makes declarations in other.rs part of launcher, not other. Ambiguous multiple ownership or cyclic module edges are skipped conservatively. Unconnected source files use conventional module paths; lib.rs/main.rs and Cargo target roots do not invent module names. This is static repository analysis: macro-generated and compiler-conditional module selection is not resolved.

Required `target` and optional `exclude` accept root-relative globs or nonempty lists. Scope defaults to production and supports tests/all. Shared test classification handles test-only items and integration sources. Optional `ignored_names` contains unique exact Rust declaration identifiers and defaults empty. Every finding spans its declaration and supports common reasoned directives.

Migration preserves Husklet `ModulePrefix` checks in `rule/repository/shape/mod.rs` and its directory/trait/test regression cases in `shape/test.rs`. Resolved current modules and complete ancestor names extend the donor's immediate-directory-first-word heuristic. Legacy standalone `_test.rs` exclusions are replaced by current shared test classification; layout enforces valid test locations. Source shape files contain other rules and must not be deleted wholesale until all their behavior migrates. The implementation reuses shared Tree-sitter syntax.
