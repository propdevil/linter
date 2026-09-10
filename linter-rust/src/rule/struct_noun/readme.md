# rust/struct-noun-naming

```toml
[[rules."rust/struct-noun-naming"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
accepted_words = ["telemetry"]
```

Every selected Rust struct name needs at least one recognized noun. Identifiers are split using the donor's ASCII naming algorithm, removing numeric characters from words before classification. `VkImageCopy2` contains `vk`, `image`, and `copy`; it passes. `Selected` and `Updated` fail. Enums, aliases, methods, and filename spelling are outside this rule.

A word qualifies when WordNet noun morphology recognizes it or the English POS tagger returns an `NN` tag. `accepted_words` extends this vocabulary case-insensitively; entries must be unique nonempty ASCII alphabetic words. An empty list adds nothing. This classifier is linguistic evidence, not proof of domain ownership.

`target` is required; optional `exclude` accepts the same root-relative glob or nonempty-list syntax. Scope defaults to `production`; `tests` and `all` are supported. Test attributes, test-only modules, and integration sources use shared Rust classification. Each failure identifies its struct and source line. Analysis reuses the shared Tree-sitter AST.

Migration preserves Husklet `rule/rust/naming.rs` and Payment-SDK `rule/adopted/naming` and Prop `rule/naming/mod.rs` classifier behavior and regression cases. Donor macros and old suppression comments do not suppress this implementation; new directive processing belongs to the common linter mechanism. Test classification now consistently handles test-only structs and nested test functions.

Dependencies preserve the original `english-pos-tagger` 0.1 and `wordnet-lemmatizer` 0.1 classifiers. The latter bundles Princeton WordNet data and its morphology algorithm derives from Apache-2.0 NLTK; no runtime dictionary installation is needed. Attribution and distribution notices are retained in `license.txt` and dependency distributions. See https://docs.rs/wordnet-lemmatizer/0.1.0/wordnet_lemmatizer/ and https://docs.rs/english-pos-tagger/0.1.0/english_pos_tagger/.
