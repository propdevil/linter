# filename

```toml
[[rules.filename]]
target = "**/src/**/*.rs"
kind = "file"
case = "snake_case"
max_words = 2

[[rules.filename]]
target = "**/src/**/*"
kind = "directory"
max_words = 1
```

Checks selected file stems and directory names. `target` is required and accepts a root-relative glob or a nonempty list. `kind` is `file`, `directory`, or `any` (default). Omitted limits allow two words for files and one for directories. `case` is optional: snake_case, camel_case, pascal_case, or kebab_case. Limits must be positive.

Words are split across underscores, hyphens, and camel-case boundaries. The final file extension is ignored. Names need at least one letter. This is a deterministic concise-name check, not an English part-of-speech classifier; noun semantics still require review. Put banned action words and catch-all vocabulary in `forbidden-words`, not here.

Examples: `payment.rs` and `payment_id.rs` pass a two-word file limit; `payment_account_id.rs` fails. A directory `payments` passes a one-word limit, while `payment_accounts` fails. Files and directories can use different blocks of this one rule. The root `.` has no repository-relative name and is not checked.

All blocks apply independently. They use discovered entries, honor `[files].exclude`, do not follow symlinks, and do not require a target to match any entries. Without configured blocks the rule reports unconfigured. Disable a rule with its table envelope and `enabled = false`; invalid retained configuration still fails.
