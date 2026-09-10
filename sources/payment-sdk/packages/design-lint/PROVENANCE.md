# Source adoption provenance

Donor: local `husklet/husklet`, revision
`1a51b7853fa71f63804cbf59de0ecb2d316c77e8`,
`src/packages/hl-design-lint`. This port uses the inspected 23-rule Rust
implementation. No source comes from the newer remote C/analyzer version.
Husklet's copyright and MIT terms are retained below.

## Copied source and tests

All paths below are relative to the donor package unless marked SDK.
Each listed donor rule folder is copied to SDK `src/rule/adopted/<folder>/`.

| Donor source | SDK destination / test evidence |
|---|---|
| `src/rule/receiver/mod.rs` | `receiver/mod.rs`; root donor regressions in `receiver/tests.rs` |
| `src/rule/catchall/mod.rs` | `catchall/mod.rs`; root donor regressions in `catchall/tests.rs` |
| `src/rule/naming/mod.rs` | `naming/mod.rs`; root donor regressions in `naming/tests.rs` |
| `src/rule/function/mod.rs` | `function/mod.rs`; root donor regressions in `function/tests.rs` |
| `src/rule/single/mod.rs` | `single/mod.rs`; root donor regressions in `single/tests.rs` |
| `src/rule/nesting/mod.rs` | `nesting/mod.rs`; root donor regressions in `nesting/tests.rs` |
| `src/rule/environment/{mod,tests}.rs` | Same files, plus root donor and SDK boundary fixtures |
| `src/rule/command/mod.rs` | `command/mod.rs`; root donor regressions in `command/tests.rs` |
| `src/rule/result/{mod,tests}.rs` | Same files plus SDK scope fixtures |
| `src/rule/blocking/{mod,support,tests}.rs` | Same files plus SDK scope fixtures |
| `src/rule/boolean/{mod,tests}.rs` | Same files plus SDK scope fixtures |
| `src/rule/state/{mod,syntax,tests}.rs` | Same files plus SDK scope fixtures |
| `src/rule/object/{mod,origin,tests}.rs` | Same files plus SDK scope fixtures |
| `src/rule/contract/{mod,tests}.rs` | Same files; evidence enriches SDK `trait-method-count` |
| `src/rule/accessor/{mod,tests}.rs` | Same files plus SDK encapsulation fixtures |
| `src/rule/duplicate/mod.rs`, `src/lib.rs` duplicate test | `duplicate/{mod,tests}.rs` |
| `src/rule/model/{mod,tests}.rs` | Same files; donor `dependencies.rs` replaced by SDK's shared Cargo graph |
| `src/rule/ceremony/{mod,namespace,marker,wrapper,tests}.rs` | Same files plus SDK mandatory chain layout fixtures |
| `src/rule/references.rs`, `src/rule/syntax.rs` | SDK `src/rule/{references,syntax}.rs` |
| `src/source.rs::snake_case` | SDK `src/source/scope.rs::snake_case` |

Root donor tests refer to the relevant tests in donor `src/lib.rs`; other
folder tests are retained beside their detector. Additional SDK fixtures cover
chain-specific layout, wallet/address models, configuration ownership,
Cargo aliases, transport boundaries, and exact reasoned exceptions.

## Intentional adaptations

- Algorithms run on SDK's single parsed `Workspace` and `SourceFile`. Named
  rule types implement `Rule` and use Husklet's chained registration style;
  the existing detector functions retain their SDK policy inputs.
- `Related`, `Review` metadata/dependencies/questions and warning severity are
  adapted to SDK's finding model. Review evidence never hides a finding.
- `hl_design` naming/classification/visual annotations become narrowly reasoned
  SDK comment exceptions. There is no `ReviewState::Check` bypass. Recognized
  framework signatures remain evidence; they do not automatically exempt code.
- SDK's original eleven rules retain their identifiers and error severities.
  Its three-method trait limit, 500 production-line limit, directory policy,
  and dependency layers remain authoritative. Broad-trait analysis only adds
  explanatory evidence to an existing trait-method-count error.
- The shared Cargo graph resolves local path identity, renames, workspace
  inheritance, target-specific edges and dependency kinds. External crates
  sharing a local package name are not treated as local dependencies.
- Production scope uses SDK's parsed cfg classifier, including external module
  ownership and file attributes. Test code remains available to rules that
  inspect it deliberately, such as unsafe dynamic-shell construction.
- Environment/process ownership and catch-all vocabulary come from policy.
  No Husklet package/domain topology is embedded in these checks. Mandatory
  SDK chain module files are not classified as ceremonial namespaces.
- SDK reporters stage owned case replacement and preserve user files. Case
  ownership markers and deterministic names are SDK additions.

## Added dependencies

Only the original naming algorithm's `english-pos-tagger` 0.1.0 and
`wordnet-lemmatizer` 0.1.0 are new lockfile packages. `quote` was already locked
and is now a direct dependency for AST signature comparison. Existing `syn`
and TOML dependencies were retained.

The installed tagger declares MIT; its notice is retained as
`LICENSE-ENGLISH-POS-TAGGER`. The lemmatizer declares Apache-2.0 and embeds
Princeton WordNet 3.0 data; the supplied database notice is retained as
`LICENSE-WORDNET`. These crates are Cargo dependencies, not copied source.
Preserve their applicable notices when distributing a built linter.

## Husklet license notice

```text
MIT License

Copyright (c) 2026 Richard Huttar

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
