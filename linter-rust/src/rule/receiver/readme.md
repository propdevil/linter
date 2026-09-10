# rust/receiver-name-repetition

```toml
[[rules."rust/receiver-name-repetition"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
ignored_names = []
```

Flags instance methods repeating the complete receiver namespace as a prefix or suffix. `Directory::create_directory` suggests `Directory::create`; `Directory::directory_remove` suggests `Directory::remove`. A middle occurrence or substring does not trigger a finding. Associated functions without a receiver are excluded.

Preserves the donor tokenizer's underscore, case, acronym, and numeric boundaries. HTTPServerV2 becomes `http`, `server`, `v`, `2`; `restart_http_server_v2` repeats its namespace, while `restart_http_server` does not. Names consisting entirely of short tokens below three characters or digits remain too ambiguous to diagnose (for example Id). A method equal to its namespace is not shortened to an empty name.

Inherent methods resolve their owning nominal type, including qualified, generic, and aliased receivers. Unknown owners are not guessed. Repository-owned trait declarations are inspected using the trait namespace; all trait implementations retain their contract names and are skipped. Explicit typed self receivers are supported.

Conversion names beginning with as/from/into/to or try_from/try_into are exempt only when their destination tokens match an explicit resolved return type, optionally through a reference or Result/Option. Self returns count as the current receiver type. `into_directory(self) -> Directory` passes; `to_directory(&self)` without a return type and `as_directory(&self) -> Other` do not gain exemptions merely from their names.

Required `target` and optional `exclude` accept root-relative globs or nonempty lists. Scope defaults to production and accepts tests/all. Optional `ignored_names` contains unique exact Rust method identifiers and defaults empty. Shared test classification handles test-only declarations and integration files. Findings contain the method span and suggested shortened name; common reasoned directives apply.

Migration preserves receiver tokenization, trait contracts, short-namespace exclusions, and regression scenarios from Husklet `rule/rust/receiver.rs`, Prop `rule/receiver/mod.rs`, and Payment-SDK `rule/adopted/receiver/{mod.rs,tests.rs}`. Name-only conversion exemptions are tightened with return-type evidence. Shared Tree-sitter syntax and nominal declarations replace the source visitors; no full-file reparsing occurs.
