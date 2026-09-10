# c/interface-breadth

Limits externally visible function declarations in selected `.h` headers.
Each direct function declarator counts, including multiple declarators in one
statement. Static functions, definitions, local prototypes, function-pointer
variables, typedefs and struct fields do not count. Functions returning pointers
or function pointers still count as functions.

```toml
[[rules."c/interface-breadth"]]
target = "native/**/*.h"
exclude = "native/generated/**"
max_functions = 24
```

`target` is required; `exclude` is optional. Both accept a root-relative pattern
or nonempty list. `max_functions` defaults to 24 and must be positive. Exactly
24 declarations pass; 25 produce one error per matching block, with evidence for
each counted declaration. `.c` files never contribute even when selected.

Header guards and conditional-preprocessor branches are traversed syntactically;
all declared alternatives count without evaluating build flags. Repeated
prototypes count as separate declarations. Macro replacement text, comments and
strings do not manufacture declarations. Analysis reuses the shared C syntax tree.

```c
// linter:disable c/interface-breadth -- Generated protocol surface has one externally fixed contract.
int first(void);
int second(void);
```

A reasoned directive on the first counted declaration suppresses the header
finding. If the configured limit is no longer exceeded, that directive is unused
and reported by the shared engine.

Migration: preserves Husklet `rule/c/interface.rs` and `interface_test.rs`
threshold, static-helper and suppression cases. Extends counting through header
guards and each declarator. Fixes false counts of callback variables/typedefs and
static declarations with reordered qualifiers. Findings are errors as required by
the unified linter, replacing the source rule's warning severity.
