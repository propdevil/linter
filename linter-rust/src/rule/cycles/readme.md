# Cargo dependency cycles

Rejects cycles among discovered local Cargo packages. Each strongly connected
component produces one finding with a closed cycle and evidence for each declared
edge. Manifest paths identify packages, so duplicate names in unrelated projects
cannot create false edges.

```toml
[[rules."rust/dependency-cycles"]]
target = ["apps/*", "packages/*", "linter", "linter-rust"]
kinds = ["normal", "build"]
```

`target` selects crate directories. A cycle is reported if it contains a selected
crate, even when another member is outside the selector. `exclude` removes crate
directories and their edges from the graph. `kinds` is a nonempty, unique list of
`normal`, `build`, and `development`; normal/build are the default. Cargo permits
dev-only cycles, so development edges require explicit opt-in.

```toml
[[rules."rust/dependency-cycles"]]
target = "packages/*"
exclude = "packages/fixtures"
kinds = ["normal", "build", "development"]
```

The language-owned `CargoGraph` analysis reads Cargo manifests without dependency
resolution, network access, or Rust parsing. Local path dependencies, package
renames, target-specific tables, and `workspace = true` inheritance are resolved.
Inherited paths are relative to the workspace manifest; member paths are relative
to the member manifest. An explicit `package.workspace` selects its discovered
workspace; otherwise the nearest discovered ancestor workspace is used.

Registry/git dependencies remain external even if their names match local crates.
Path dependencies outside discovered files do not contribute edges. Missing path
manifests, missing inherited entries, invalid meaningful dependency fields, and
package-name mismatches fail analysis instead of silently hiding edges.

This checks the declared graph: optional dependencies and all target conditions
participate, without attempting feature or platform satisfiability. Evidence names
the target condition. A declared cycle can therefore span mutually exclusive
platforms; configure the intended graph scope explicitly.
