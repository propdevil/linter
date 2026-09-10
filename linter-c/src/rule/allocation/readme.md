# c/unchecked-allocation

Reports configured nullable allocation results dereferenced before a proven
non-null guard. Tracks local declarations, assignments, casts and direct pointer
aliases. Recognizes `*p`, `p[index]` and `p->field`; `sizeof` is unevaluated.
One error per allocation and block links the allocation to its first unsafe use.

```toml
[[rules."c/unchecked-allocation"]]
target = ["native/**/*.c", "native/**/*.h"]
exclude = "native/generated/**"
functions = ["malloc", "calloc", "realloc"]
```

`target` is required; `exclude` is optional. Each accepts a root-relative pattern
or nonempty list. `functions` is a required nonempty list of exact C identifiers;
there is no built-in allocator vocabulary. Missing blocks are unconfigured.
Only selected discovered C sources are checked, using the shared syntax tree.

```c
int *p = malloc(sizeof *p);
if (!p) return; // All continuing paths have established p is non-null.
*p = 1;
```

A non-null branch such as `if (p) { *p = 1; }` passes. Short-circuit checks honor
which operand executes: `p && *p` passes, while `p || *p` is unsafe. Reassignment
from a configured allocator invalidates previous proof. Checks after a use or
inside unrelated branches do not protect that use. `if (requested && !p) return`
does not establish `p` outside the branch. Passing an allocation to another
function, without a visible dereference, is not reported.

This is bounded local syntax analysis. It does not prove contracts through
callee bodies, pointer arithmetic, object fields, macro expansion, or arbitrary
jumps. Loop merges conservatively retain possible nullable state. It does not
claim whole-program memory safety. Complex equivalent guards may require a
simpler explicit null guard or a narrowly reasoned directive.

```c
// linter:disable c/unchecked-allocation -- This configured allocator aborts on exhaustion.
int *p = allocate(4);
*p = 1;
```

Migration: replaces Husklet `rule/c/allocation.rs` and `allocation_test.rs`.
Preserves direct nullable-use, sizeof, prior-check and late-check cases; extends
aliases and assignments. Replaces substring-based guard acceptance with prior
branch evidence, correcting the old conditional-guard false negative. Historical
`hl-lint` suppression tests now exercise common reasoned directives and unused
suppression diagnostics.
