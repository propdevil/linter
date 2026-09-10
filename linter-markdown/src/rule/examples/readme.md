# markdown/examples

Validates example documents using CommonMark structure. Markdown inventory and
required paths belong to `layout`.

## Configuration

```toml
[[rules."markdown/examples"]]
target = "docs/examples/*.md"
require_title = true
min_cases = 1
require_closed_fences = true
```

Requires one leading top-level H1, at least the configured number of top-level
H2 cases, and closed code fences. These values are the defaults. Set `min_cases`
to zero for documents without case sections. At least one check must remain
enabled. Optional `exclude` accepts the same selectors as `target`.

Setext headings count. Heading-like lines inside code or quotes do not count.
Backticks and tildes use matching fence characters and sufficient closing length.
The parser runs once per document per registry invocation. File exclusion and
symlink policy come from the generic project. There are no comment suppressions
for document-wide requirements; narrow selectors define intended documents.

## Example

A title followed by `## Parsing` and a closed Rust code fence passes. An open
fence with no case headings produces two errors. Two document titles fail even
if the rest of the document is valid.
