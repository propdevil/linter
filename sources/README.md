# Linter source collection

Source snapshots gathered for a future combined linter. All package source, tests, and package documentation are copied unchanged. No merged executable has been implemented yet.

| Source | Packages |
|---|---|
| Payment-SDK | `payment-sdk/packages/design-lint` |
| Prop | `prop/packages/design-lint` |
| Husklet | `husklet/src/packages/hl-design-lint`, companion `husklet/src/packages/hl-design` |

[Rule inventory](RULES.md) links the collected rule IDs to their implementations. All implemented rules are retained, including rules not enabled by the original project policies.

Each source's `context/` contains its available policy, lint examples, repository instructions, original workspace manifest and lockfile, and toolchain configuration. Prop's lint scripts are also preserved there. These files provide original configuration context; they do not establish a buildable workspace here. Payment-SDK and Husklet manifests still inherit their original workspace settings. Prop design-lint has its own standalone manifest and lockfile.

Build outputs, Git metadata, and generated repository-specific findings are excluded. `SOURCES.json` records every copied file's original absolute path and SHA-256 checksum. Copies were verified byte-for-byte; no compilation or behavior changes were made.
