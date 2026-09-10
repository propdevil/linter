# forbidden-words

```toml
[[rules."forbidden-words"]]
target = "**/src/**"
words = ["common", "core", "helper", "helpers", "misc", "shared", "util", "utils", "loading", "running"]
```

Checks the entire repository-relative path of every selected regular file or directory. Both directory components and the final filename are scanned; the final file extension is ignored. File contents are not inspected.

`src/helper.rs`, `src/paymentHelper.rs`, and `src/helpers/payment.rs` match the listed vocabulary. `src/score.rs` does not match `core`. Selecting only `**/*.rs` still checks the ancestor directory components of each selected file. One finding per selected path lists its matching words; selecting the directory and its children can report each affected path.

Matching is case-insensitive by normalized whole words. Vocabulary must contain unique single alphanumeric words with at least one letter. Empty, multiword, glob, and duplicate values fail validation. `words = []` disables the vocabulary check for that block. All vocabulary lives in TOML; the library embeds no forbidden-word list.

All blocks apply independently. They use discovered entries, honor `[files].exclude`, do not follow symlinks, and do not require a target to match any entries. Without configured blocks the rule reports unconfigured. Disable a rule with its table envelope and `enabled = false`; invalid retained configuration still fails.
