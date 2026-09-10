# redundant-parent-name

Reports selected regular files whose stem repeats complete semantic words from
its immediate parent directory. Case and separators normalize consistently:
`net_work/socketWork.rs` repeats `work`, but `net_work/network.rs` passes.
`HTTPServer/server_http.rs` repeats both `http` and `server` in one finding.
Only the final extension is removed; ancestors beyond the immediate parent do
not contribute words. Root files have no in-project parent name to compare.

```toml
[[rules."redundant-parent-name"]]
target = ["**/src/**/*.rs", "native/**/*.{c,h}"]
exclude = "native/generated/**"
ignored_names = ["lib", "main", "mod", "build", "index"]
```

`target` is required. `exclude` optionally accepts one pattern or a nonempty
list. `ignored_names` defaults to empty and compares exact, case-sensitive file
stems. Project exclusions apply first; directories and symlinks are not checked.
Each block reports at most one finding per file, listing every repeated word.
Rename `memory/memory_map.h` to `memory/map.h` when it preserves ownership and
avoids collisions.

Migration: preserves Husklet `ParentName` in
`sources/husklet/src/packages/hl-design-lint/src/rule/repository/shape/mod.rs`
and its `filename_does_not_repeat_parent_semantic_word` regression cases.
Extends it to explicit language-neutral targets and all repeated words.
Replaces implicit conventional-entry exemptions with `ignored_names` in config.
