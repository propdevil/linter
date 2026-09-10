# Dependency budget

Limits distinct declared dependencies of selected Cargo packages. Local path,
registry, and git dependencies all count. Optional dependencies count even when
their features are disabled; no builds, lockfile resolution, or network calls are
needed.

```toml
[[rules."rust/dependency-budget"]]
target = "packages/*"
max_dependencies = 8
kinds = ["normal", "build"]
```

`max_dependencies` is required and positive. `target` and optional `exclude`
select crate directories. `kinds` defaults to normal/build; add `development` to
include development-only dependencies. Lists must be nonempty and unique.

```toml
[[rules."rust/dependency-budget"]]
target = "apps/*"
exclude = "apps/fixtures"
max_dependencies = 16
kinds = ["normal", "build", "development"]
```

The same package imported under two aliases or repeated across dependency kinds
or target conditions counts once. Local identity uses the canonical manifest;
external identity uses package name and source. Different registries or git
sources/revisions remain distinct. Version requirements and feature sets do not
split one package identity: this measures declared coupling, not the number of
resolved versions in Cargo.lock. Evidence lists all contributing declarations.

Workspace inheritance, renamed packages, target-specific tables, and strict
manifest validation use `CargoGraph`, shared with the dependency-cycle rule.
Dependencies outside discovered files still count by their canonical path source.
Excluding a crate skips its budget check; it does not subtract that crate from
other packages' dependency counts.
