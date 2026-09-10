# c/cppcheck

Runs configured Cppcheck categories against a compilation database filtered to
selected, discovered C translation units. Uses the bounded C tool runner.

## Configuration

```toml
[[rules."c/cppcheck"]]
target = "native/**/*.c"
executable = "cppcheck"
compilation_database = "build/compile_commands.json"
checks = ["warning", "performance", "portability"]
standard = "c11"
inconclusive = true
suppressions = ["missingIncludeSystem"]
timeout_ms = 30000
max_output_bytes = 1048576
```

Executable, database, nonempty check categories and C standard are required.
Suppressions default to an empty list and inconclusive analysis defaults false.
No suppression vocabulary is embedded. Optional `exclude` narrows the selection.
Paths resolve like `c/tidy`; excluded compile entries cannot be restored by the
database. A selected project with no matching compilation entries fails analysis.

The linter passes arguments directly, without a shell or source-editing flags.
Tool diagnostics retain their file, line, column, category, message and diagnostic
ID in the finding message; the containing finding is attached to the configured
database. All enabled categories produce errors. Execution failures are distinct
from diagnostic findings. Tool output is captured, never forwarded to MCP stdout.

## Examples

A memory-leak diagnostic fails with its Cppcheck ID. A clean run passes. An invalid
tool option or unavailable executable fails analysis. Configure checks and
suppressions using the [Cppcheck manual](https://cppcheck.sourceforge.io/manual.html).
