# rust/god-object-growth

Rejects large state-owning types only when method count, independent field capabilities, and a crossing workflow all supply evidence.

```toml
[[rules."rust/god-object-growth"]]
target = "**/*.rs"
excluded_suffixes = ["Builder"]
```

Defaults are `max_methods = 20`, `min_fields = 3`, `min_clusters = 3`, and `min_methods_per_cluster = 2`. Counts must be positive, and minimum clusters must be at least two. Exactly 20 methods passes; more than 20 is only a candidate. Target and optional exclude accept a glob or nonempty list. Scope defaults to production, with tests/all alternatives.

Count inherent methods with receivers across all resolved impl blocks. Associated functions and trait implementations do not inflate the budget. Group methods by the exact set of owned fields they touch, requiring calls on a field capability and at least the configured number of methods per group. Remove broader overlapping groups, then require independent groups from distinct owning namespaces. Finally require a method that both contains control flow or assignment and calls capabilities from at least two distinct origins.

An application with seven workspace methods, seven settings methods, seven terminal methods, and a conditional workflow across those services fails. A codec with many cohesive methods, a thin forwarding facade without workflow logic, or several stores within one protocol namespace passes.

Type ownership comes from resolved nominal identities. The analyzer automatically unwraps standard Box, Option, Arc, Rc, Mutex, RwLock, RefCell, and sync/rc Weak containers; user-defined wrappers retain their identity. Unresolved types do not invent distinct domains. `excluded_suffixes` defaults to empty; it is a project naming policy. The removed `unwrap_types` option is rejected. C-layout representations and automatically derived implementations preserve donor exemptions. Test-only code is filtered consistently.

Diagnostics include the owner span, representative field-group methods, and crossing workflow. Directives attach to the owner struct. All syntax comes from shared analysis.

Migration preserves Husklet/Prop independent field groups, origin diversity, receiver counts, and workflow requirements. Payment-SDK origin evidence remains present; warnings become errors. Unlike the donor's blanket exclusion of repeated type names, resolved module ownership distinguishes unrelated same-named declarations. Nested functions and deferred closure/async bodies do not supply a method's immediate workflow evidence.
