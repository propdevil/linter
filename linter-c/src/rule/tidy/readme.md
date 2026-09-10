# c/tidy

Runs configured clang-tidy checks over selected C translation units using a
filtered compilation database. Headers are analyzed through their compiled units.

## Configuration

```toml
[[rules."c/tidy"]]
target = "native/**/*.c"
executable = "clang-tidy"
compilation_database = "build/compile_commands.json"
checks = "clang-analyzer-*,bugprone-*"
extra_args = ["-std=c11"]
timeout_ms = 30000
max_output_bytes = 1048576
```

The executable, database file and checks are required. Bare executable names use
PATH; executable paths and the database resolve from the project root. Optional
`exclude` narrows selected files. Compilation entries outside the discovered
selection cannot reintroduce excluded/generated sources. Relative compilation
working directories resolve beside the original database. Compile command
metadata is preserved; the linter never evaluates its command strings itself.

Clang-tidy receives warning promotion and no fix flags. Compiler-only extra args
are passed individually without a shell. Diagnostics become findings. Execution
failures with no recognizable diagnostics fail analysis. Captured output uses
the same timeout and output limits as `c/format` and never reaches MCP stdout.

## Examples

A selected `native/file.c` with a compile entry is checked. An excluded vendor
entry is removed from the temporary database. Selected C files with no matching
compilation entry produce an analysis error. A warning is an error-level finding;
a missing executable is an execution error. Header locations remain in the tool
message; the finding is attached to the selected translation unit.

Tool configuration follows the [clang-tidy documentation](https://clang.llvm.org/extra/clang-tidy/).
