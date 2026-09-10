# layout

Assert required paths, allowed directory structure, and directory shape. Name checks belong to `filename`, repeated filename affixes to `shared-affix`, and prohibited vocabulary to `forbidden-words`.

## Directory structure

```toml
[[rules.layout]]
target = "configs"
mode = "restrictive"
files.allowed = [{ target = "*.toml", description = "Embedded project presets." }]

[[rules.layout]]
target = "{linter,linter-*}/src/rule/*"
mode = "restrictive"
files.required = ["mod.rs", "config.rs", "readme.md"]
files.allow_empty = false
directories.allowed = [{ target = "fixtures", description = "Rule test inputs." }]
```

`target` selects root-relative directories; it accepts one glob or a nonempty list. `.` selects the root. `*` stays within a segment; `**` crosses directories. Required paths are literal, relative to each selected directory. `permissive` (default) permits extras; `restrictive` rejects unlisted immediate entries. Required paths and their parents are implicitly permitted. Optional allowed entries need purpose descriptions and apply only in restrictive mode. All structural assertions apply independently; unmatched directory targets produce findings. Symlinks cannot satisfy requirements.

## Ordered file permissions

```toml
[[rules.layout]]
target = ["**/*.md", "**/*.markdown"]
allow = false
case_sensitive = false

[[rules.layout]]
target = "docs/*.md"
allow = true
description = "Project goal and design."

[[rules.layout]]
target = "{linter,linter-*}/src/rule/*/readme.md"
allow = true
description = "Rule documentation."
```

The last matching file permission wins. A later ban can narrow an earlier allowance. Every allowance needs a nonempty description. Permissions do not bypass structure or another rule's checks. Targets are case-sensitive unless their block sets `case_sensitive = false`. Permissions use discovered entries and honor exclusions; symlinks cannot satisfy allowances.

## Directory shape

```toml
[[rules.layout]]
target = "**/src{,/**}"
files.allow_empty = false
directories.allow_empty = false
directories.allow_single_file = false
```

Shape checks inspect immediate physical children, including excluded children inside selected directories. Empty files contain zero bytes; empty directories have no physical entries. Single-file directories contain exactly one regular file and no other entries. All three settings default to true. Selecting parents inside source trees preserves the crate's `src` boundary. Do not add meaningless files to satisfy these constraints.

## Test locations

```toml
[[rules.layout]]
target = ["**/{test,tests}.rs", "**/*{_,.,-}{test,tests}.rs", "**/{test,tests}{_,.,-}*.rs", "**/{test,tests}/**/*.rs"]
allow = false
case_sensitive = false

[[rules.layout]]
target = "**/*{Test,Tests}.rs"
allow = false

[[rules.layout]]
target = ["tests/**/*.rs", "{apps,packages,usecase}/*/tests/**/*.rs"]
allow = true
description = "Crate integration tests and repository-wide cases."
```

Unit tests stay inline beside implementation. Adjust crate-category selectors for integration tests; this repository uses `{linter,linter-*,apps/*}/tests/**/*.rs`. Avoid a broad `**/tests/**` allowance, which also admits `src/tests/`. Layout checks paths, not Rust attributes or test behavior.

Old `glob`, permission `files` selectors, and layout naming/word-limit/affix/vocabulary settings are rejected. Generate current presets with `linter init rust`. Missing configuration reports unconfigured; invalid settings fail even for disabled rules.

Permission blocks accept `kind = "file"` (default), `"directory"`, or `"any"`.
A banned directory is reported once; its descendants do not duplicate that finding.
Later matching allowances still decide whether the directory itself is allowed.

```toml
[[rules.layout]]
target = "old{,/**}"
kind = "any"
allow = false

[[rules.layout]]
target = "**/src"
directories.allow_empty = false
directories.allow_single_file = false
directories.content_ignored = [".gitkeep"]
```

Content exclusions are relative to each inspected child directory. They affect
emptiness and single-file counts, not required paths or structural allowances.
Project-excluded entries do not count as substantive discovered content.
This transfers configurable forbidden paths and placeholder-aware directory checks.
