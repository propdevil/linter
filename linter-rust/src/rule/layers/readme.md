# rust/layers

Enforce allowed dependency directions between Cargo packages. Each package directory must match exactly one configured layer. Dependencies name permitted target layers, including the source layer when peer dependencies are permitted.

```toml
[[rules."rust/layers"]]
name = "apps"
target = ["apps/*"]
dependencies = ["usecase", "packages"]

[[rules."rust/layers"]]
name = "usecase"
target = ["usecase/*"]
dependencies = ["usecase", "packages"]

[[rules."rust/layers"]]
name = "packages"
target = ["packages/*"]
dependencies = ["packages"]
```

Apps may depend on use cases and packages, but never other apps. Use cases may depend on peers and packages. Packages may depend only on other packages. Paths are root-relative directory globs, not package names. `.` matches a package at the repository root. Unknown layer names, duplicate names, and malformed globs are configuration errors. Unclassified or multiply classified packages produce findings. Ordering does not resolve ambiguous ownership.

## Multiple Cargo packages in one layer

Given this repository:

```text
apps/api/Cargo.toml
packages/crypto/Cargo.toml
packages/http/Cargo.toml
packages/json_rpc/Cargo.toml
packages/storage/Cargo.toml
packages/testing/Cargo.toml
```

Use directory globs to assign all five packages without listing them individually:

```toml
[[rules."rust/layers"]]
name = "packages"
target = ["packages/*"]
dependencies = ["packages"]

[[rules."rust/layers"]]
name = "apps"
target = ["apps/*"]
dependencies = ["packages"]
```

Allowed: `apps/api -> packages/http` and `packages/http -> packages/json_rpc`.
Forbidden: `packages/http -> apps/api` and dependencies between two apps.
Adding `packages/cache/Cargo.toml` requires no layer configuration change.
The directory name may differ from the Cargo package name: matching uses the directory containing `Cargo.toml`.

Listing `"packages"` in its own `dependencies` permits dependencies between packages in that layer. Set `dependencies = []` to forbid local dependencies from that layer, including dependencies between its packages. This does not ban registry or Git dependencies.

## Nested package directories

For packages such as `packages/storage/postgres/Cargo.toml`, replace the packages layer's path selector with:

```toml
[[rules."rust/layers"]]
name = "packages"
target = ["packages/**"]
dependencies = ["packages"]
```

`*` matches one directory segment; `**` also matches deeper directories. This replaces the preceding packages block; do not append a second layer with the same name. Ordinary directories without a Cargo package are not assigned layers.

## Separate groups of packages

To allow HTTP packages to depend on crypto packages while preventing the reverse direction, split their ownership into disjoint layers:

```toml
[[rules."rust/layers"]]
name = "crypto"
target = ["packages/crypto/*"]
dependencies = ["crypto"]

[[rules."rust/layers"]]
name = "http"
target = ["packages/http/*"]
dependencies = ["http", "crypto"]

[[rules."rust/layers"]]
name = "apps"
target = ["apps/*"]
dependencies = ["http", "crypto"]
```

This example expects packages under paths such as `packages/crypto/hashing/Cargo.toml` and `packages/http/client/Cargo.toml`. Do not retain a broad `packages/**` layer alongside these layers: packages would match multiple owners. Each example is an alternative configuration, not a block to append indiscriminately.

## This repository

```toml
[[rules."rust/layers"]]
name = "linter"
target = ["linter"]
dependencies = []

[[rules."rust/layers"]]
name = "rust"
target = ["linter-rust"]
dependencies = ["linter"]

[[rules."rust/layers"]]
name = "apps"
target = ["apps/*"]
dependencies = ["linter", "rust"]
```

This permits `linter-rust -> linter` and apps depending on either library. Neither library may depend on an app, and `linter -> linter-rust` is forbidden.

Analysis loads Cargo declarations with `cargo metadata --no-deps --offline`, without building code or running build scripts. It checks local path dependencies, including renamed, inherited workspace, optional, development, build, and target-specific declarations regardless of active features or host target. Registry and Git dependencies have no project layer and are outside this rule. Local path dependencies outside analyzed packages produce findings rather than silently passing. Cargo must be installed and manifests must be valid; resolution errors fail analysis.

All discovered `.rs` files are parsed once per validation run into Tree-sitter syntax trees in `linter-rust::Analysis`. Enabled, configured Rust rules share that input; each new run reloads it. The shared linter library owns no Rust syntax or Cargo model. Syntax errors fail analysis rather than appearing as a clean check. This rule uses Cargo edges, not syntax heuristics: `use` statements do not establish package dependencies. Macro expansion, compiler name resolution, module-level direction, cycle detection, and registry patch resolution are not implemented by this rule.

The rule is enabled by default but unconfigured until layers are supplied. Disabling it skips analysis when no other configured rule needs Rust input. Discovery obeys `[files].exclude`; keep reference sources and build output excluded in repository configuration.
