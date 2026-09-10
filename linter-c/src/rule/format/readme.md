# c/format

Compares selected C source/header bytes with a configured clang-format executable.
The command emits formatting to captured output; it never receives an edit flag.

## Configuration

```toml
[[rules."c/format"]]
target = "native/**/*.{c,h}"
executable = "clang-format"
style = "file"
fallback_style = "LLVM"
timeout_ms = 30000
max_output_bytes = 1048576
```

Executable, style and fallback style are explicit. A bare executable uses PATH;
a path containing directory components is relative to the project root. Optional
`exclude` filters selected files. An unavailable executable, timeout, excessive
output, nonzero exit or stderr output is an execution error. A formatting
mismatch is a finding. No arbitrary command arguments or shell expansion exist.

Output is captured in temporary files, polled every five milliseconds, and read
with a memory bound. The byte limit may be exceeded on disk between polls. The
child is killed and reaped on a limit error. The limit and timeout must be positive.
No output is forwarded onto the MCP transport. This rule does not require AST
analysis and does not support item-level directives for a whole-file comparison.

## Examples

Already formatted `int x;` passes. Different whitespace returned by the formatter
fails without changing the original file. Configure project-owned style files as
explained in the [clang-format documentation](https://clang.llvm.org/docs/ClangFormatStyleOptions.html).
