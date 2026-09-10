# rust/module-name

```toml
[[rules."rust/module-name"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
forbidden_words = ["util", "utils", "core", "common", "shared", "helper", "helpers", "misc"]
```

Rejects configured semantic words in actual Rust module declarations. Both inline `mod common {}` and external `mod common;` are checked, including nested declarations. `#[path = "entities.rs"] mod common;` reports the declared `common` name. Filesystem names are independently checked by `forbidden-words`.

Required `forbidden_words` is nonempty and contains unique nonempty ASCII alphabetic words, compared case-insensitively. Raw identifier prefixes are removed; underscores, hyphens, and case boundaries split words. `helper_domain` and `CoreData` fail for helper/core; `utility` and `score` pass. No forbidden vocabulary is embedded in implementation code.

Required `target` and optional `exclude` accept root-relative globs or nonempty lists. Scope defaults to production; tests/all are supported. Test-only declarations and integration files use shared Rust classification. Every finding spans its module declaration and supports common reasoned directives. Functions, structs, import paths, comments, string contents, and undeclared file names do not trigger this rule. Loaded external files do not hide their module declaration finding.

Migration transfers `catch-all-module-name` from Husklet `rule/rust/catchall.rs`, Prop `rule/catchall/mod.rs`, and Payment-SDK `rule/adopted/catchall/{mod.rs,tests.rs}`. Filesystem scanning moves to the existing language-neutral forbidden-words rule. Whole-name comparison is extended to whole semantic words, and test inclusion becomes explicit through scope. Shared Tree-sitter analysis replaces source-rule syntax visitors without reparsing files.
