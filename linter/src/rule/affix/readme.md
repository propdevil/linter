# shared-affix

```toml
[[rules."shared-affix"]]
target = "**/src/**/*.rs"
max_prefix = 2
max_suffix = 2
```

Groups selected regular files by parent directory and checks repeated first/last whole words in their stems. The final extension is ignored. A maximum of two flags the third matching sibling. Configure at least one positive `max_prefix` or `max_suffix`; an omitted side is unchecked.

`func_a.rs`, `func_b.rs`, and `func_c.rs` suggest `func/{a,b,c}.rs`. `a_func.rs`, `b_func.rs`, and `c_func.rs` suggest the same organization. Words are normalized across snake_case, kebab-case, and camelCase. `function_a.rs` does not share the prefix word `func`. Single-word names are not grouped. Files in different parent directories never contribute to one group. Suggestions require ownership and collision review; no files are moved.

All blocks apply independently. They use discovered entries, honor `[files].exclude`, do not follow symlinks, and do not require a target to match any entries. Without configured blocks the rule reports unconfigured. Disable a rule with its table envelope and `enabled = false`; invalid retained configuration still fails.

To restrict suffix checks to implementation roles, supply a vocabulary:

```toml
[[rules."shared-affix"]]
target = "**/src/**/*.rs"
max_suffix = 2
suffix_words = ["adapter", "handler", "service", "registry"]
```

Three `*_handler.rs` siblings fail; three `*_wallet.rs` siblings pass this block.
Omitting `suffix_words` checks every suffix. An explicit list must be nonempty,
contain unique lowercase words, and accompany `max_suffix`. Prefix checks are
unaffected by the suffix vocabulary.
