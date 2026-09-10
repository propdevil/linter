# c/test-only-state

Reports production predicates reading file-scope state whose recorded writers
exist only in test code or helpers reachable exclusively from test code. Such
predicates otherwise observe initial state forever in production despite tests
exercising assigned state.

```toml
[[rules."c/test-only-state"]]
target = "native/**/*.c"
exclude = "native/generated/**"
macros = ["PROJECT_TEST_HOOKS"]
```

`target` and nonempty exact macro names are required; `exclude` is optional.
Selectors accept one root-relative pattern or a nonempty list. Targets select
reported readers, while all discovered C files contribute writer/call evidence.
Project file exclusions remove evidence from analysis altogether.

```c
static int ready;
#ifdef PROJECT_TEST_HOOKS
void arm(void) { ready = 1; }
#endif
int use(void) { return ready != 0; } // Error: production never sets ready.
```

Configured test macros are absent in production. `#ifdef`, `#ifndef`, `defined`,
negation, conjunction, disjunction and alternative branches honor that fact.
Unknown macros/conditions remain potentially production; an unknown condition
alone never establishes test-only code. Initial zero/null values are not writers;
nonzero production initializers are. Assignments, increments, writes through
members/subscripts and taking a state's address count as possible writers.

Helper classification propagates only when every known call is test-only.
Functions without known calls remain production entrypoints. Readers in test-only
helpers are excluded too. Static state/functions are file-local; external names
can connect across files. Parameters and lexical locals shadow file-scope state.
No compiler expansion, indirect-call resolution, pointer alias analysis or
whole-program linkage proof is claimed. Missing external callers limit evidence;
use explicit production entrypoints rather than relying on hidden call paths.

Each error links the production read, state declaration and test-only writes.
Current directives attach to the containing predicate statement and must explain
an intentional exception.

Migration: transfers Husklet `rule/c/hook.rs` and all twelve `hook_test.rs` cases.
Corrects the cross-file fixtures to use external definitions instead of linking
unrelated `static` objects. Extends static identity, lexical shadowing, inverted
conditional handling, source evidence and current directives. Preserves the
source rule's conservative treatment of address-taking as a possible write.
