# Goal

Build a repository linter specifically for coding agents. It validates whether
an agent's changes follow the repository's configured expectations and returns
precise findings, evidence, and instructions the agent can act on.

The linter checks observable code and repository structure. It does not claim
to prove that a design is correct. Measurable policy violations and design
concerns requiring judgment must remain distinguishable.

## Everything is a rule

Directory layout, dependency direction, ownership, naming, and language-specific
checks are all rules. There is no separate architecture policy system.

Every rule must:

- Be enabled by default, independently configurable, and possible to disable.
- Have a stable identifier and typed settings with documented, portable defaults.
- Own its implementation, configuration, and behavioral tests. Keep short rule
  descriptions in each rule folder’s `readme.md`.
- Explain the violated expectation and what the agent should inspect or change.
- Report limitations honestly instead of treating incomplete analysis as success.

Rules must not embed Payment-SDK, Prop, or Husklet's project-specific assumptions.
Repository-specific paths, boundaries, and names belong in configuration.
Layout policies describe directory categories with globs, not an inventory of
every current app, rule, or preset. Adding an instance of an existing category
must not require editing its layout policy.
If a rule needs facts that have not been configured, report that explicitly;
do not invent a repository's intended structure.

## Repository configuration

The repository's root `linter.toml` selects settings for its rules. Omitted rules
remain enabled with their defaults. Unknown rule names, misspelled settings,
invalid values, and malformed globs are errors, not ignored input.

Root `configs/` contains project presets such as `default.toml` and `rust.toml`.
The CLI embeds these files. `linter init rust` creates `linter.toml` without
overwriting an existing file; `linter config rust` prints the same preset.
Sane exclusions, such as Git metadata and build output, live in presets rather
than scanner code. An explicit empty exclusion list means nothing is ignored.

Keep common execution settings small. File discovery can be shared, but whether
a file, directory, dependency, or declaration is acceptable belongs to a rule.

## Agent workflow

1. Read the repository's configured expectations.
2. Change repository files.
3. Validate through the CLI or MCP server.
4. Inspect findings and instructions, then fix the code or document a justified
   exception.
5. Validate again.

CLI and MCP use the same library and return equivalent findings. MCP supplies
validation results and instructions; the agent performs edits. Checking must
not modify the repository or require building the checked application.

Source-level exceptions will use ordinary comments:

```rust
// linter:disable rust/trait-method-count -- Required by an external contract.
```

Each directive names an exact rule and includes a nonempty reason. Local
directives apply to the next declaration or statement; a separate
`linter:disable-file` directive provides explicit file scope. Suppressed findings
remain inspectable. Layout checks use configuration because missing paths have
no source declaration to annotate. Comment directives are not part of the first
layout-only milestone.

## Workspace and ownership

Use one Cargo workspace:

- `linter/`: reusable library, configuration, and language-independent rules.
- `apps/cli/`: terminal adapter, JSON output, and exit status.
- `apps/mcp/`: local stdio MCP adapter over the same library.
- `configs/`: declarative presets embedded in the CLI executable.

`linter-rust` owns Rust syntax, Cargo analysis, and Rust rules.
`linter-c` owns C analysis and rules. Both use Tree-sitter.
`linter-markdown` owns CommonMark parsing and document-content rules. Share parsing mechanics where useful;
keep language semantics in the owning package. The shared library must not
depend on concrete language packages.

Each rule implements the public `Rule` trait: stable ID, typed configuration,
constructor validation, and a check over project entries plus typed analysis.
The registry prepares each analysis type once per run for enabled, configured
rules. Rust analysis and syntax trees stay in `linter-rust`, not `Project`. Each app explicitly
registers its rules in `Registry` at startup. Other crates use the same trait.
Duplicate IDs and settings for unregistered rules are errors, even when disabled.
Runtime plugin loading remains outside the first milestone.

## First milestone: enforce directory layouts

Start with configuration, the apps, and `layout`. Markdown path restrictions belong
to layout; Markdown content contracts belong to `linter-markdown`. Use the CLI to validate this repository throughout.

The layout rule must let a repository:

- Select directories using root-relative globs.
- Require files and directories relative to every selected directory.
- Use `permissive` mode to assert required paths while allowing extras.
- Use `restrictive` mode to reject unlisted immediate entries.
- Configure files and directories consistently with `required` paths and
  optional `allowed` glob objects with nonempty purpose descriptions. Required entries are implicitly allowed.
- Apply every matching structural assertion. For file permissions, the last
  matching block explicitly decides whether the file is allowed.
- Receive findings for missing paths, wrong entry types, unexpected entries,
  and configured globs matching no directories.

Do not follow symlinks or let them satisfy required paths. Validate relative
paths and reject parent traversal. The rule defaults to enabled with no assumed
layout; an empty configuration is explicitly reported as unconfigured.

Ordered `[[rules.layout]]` file permission blocks use `target` globs and `allow`.
The last matching permission wins; allowances require a purpose description.
Structural and naming checks apply independently of permission decisions. Presets ban Markdown except `docs/*.md`;
extensions and exceptions live in TOML, not code. Descriptions record intent,
not content validation. Preserved sources remain excluded.

Layout controls whether empty files and directories are allowed (allowed by
default). An opt-in `directories.allow_single_file = false` rejects directories
containing exactly one regular file and no other entries. Scope it to meaningful
source areas, preserving intentional boundaries such as crate source roots.
Case and word limits belong to `filename`; repeated sibling prefixes and suffixes
belong to `shared-affix`; prohibited path vocabulary belongs to `forbidden-words`.

Enforce the same structure for every rule, including the layout rule itself:

```text
rule/<name>/
  mod.rs
  config.rs
  readme.md
```

Unit tests live inline beside implementation under `#[cfg(test)] mod tests`.
Integration tests live in crate-level `tests/` directories; repository-wide
cases live in root `tests/`. Layout bans standalone test filenames elsewhere.
Allow an optional `fixtures/` directory. Keep rule descriptions short: purpose
and a configuration example in each rule’s `readme.md`. Layout requires this
file and allows it through a documented category glob. Use concise comments for
non-obvious contracts. Do not add a root README. This goal file is the agreed
repository-level description.

## Acceptance criteria

- The workspace builds and its tests, formatting, and Clippy checks pass.
- `cargo run --locked -p linter-cli -- check .` validates this repository using
  its own `linter.toml`, including the enforced rule structure.
- Removing a required file in a disposable fixture produces the exact finding
  and CLI exit status: 0 for no findings, 1 for violations, 2 for invalid
  configuration or execution failure.
- CLI supports human-readable and JSON output. MCP exposes one working `check`
  tool with equivalent findings and instructions.
- Embedded presets initialize valid configuration from any working directory
  without needing the source tree or overwriting existing configuration.
- Markdown outside allowed globs produces an error; every glob requires a
  description. New documents matching a glob need no configuration changes.
- Tests cover configured defaults, disabling, globs, nested requirements, wrong
  types, extra entries, overlapping layouts, exclusions, and symlinks.
- Original sources remain unchanged; active code does not import them.

## Later work

Inventory and migrate the original Rust and C rules with their behavioral tests.
Do not equate matching rule IDs with matching behavior. Make every migrated rule
follow the enforced structure and configuration contract.

Language servers, compiler-level semantics, automatic fixes, external analyzer
execution, runtime plugins, and persistent indexing are outside the first
milestone. None should delay a useful, self-validating layout linter.

## Targeted path checks

Every configurable input selector uses `target`, a root-relative glob or list.
Layout owns existence, permissions, and directory structure. The independent
`filename` rule checks file and directory name case and word counts, `shared-affix`
checks repeated words among selected sibling filenames, and `forbidden-words`
checks selected paths including ancestor components. Vocabulary lives in TOML.
These checks remain language-independent; Rust analysis stays in its package.
