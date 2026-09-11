# rust/broad-trait-responsibilities

Rejects large traits whose methods form several distinct capability clusters with supporting signature or subject evidence. Method count alone never triggers this rule; use `rust/trait-method-count` for a strict size limit.

```toml
# Optional policy for a repository with these specific domain operations.
[[rules."rust/broad-trait-responsibilities"]]
target = "packages/fulfillment/**/*.rs"
min_methods = 6
min_clusters = 3
min_methods_per_cluster = 2
capabilities = [
  { name = "invoicing", verbs = ["invoice", "refund"] },
  { name = "inventory", verbs = ["reserve", "restock"] },
  { name = "shipping", verbs = ["dispatch", "track"] },
]
```

This rule is not configured in the bundled presets. Prefer `rust/trait-method-count` for a portable size gate. The example is an optional project policy, not a universal capability dictionary. Capability configuration is required; no hidden repository vocabulary is supplied by code. `target` and optional `exclude` accept a glob or nonempty list. Scope defaults to `production`; `tests` and `all` are supported. Threshold defaults are 8/3/2. Minimum clusters must be at least two; all thresholds must be positive, supported by the vocabulary, and mutually coherent. Duplicate vocabulary within the verb or noun namespace is rejected rather than resolved by table order.

A method's first configured noun after its leading word takes precedence over its leading verb. Retain clusters with at least `min_methods_per_cluster` methods. At least `min_clusters` clusters and half the trait's methods must remain classified. At least `min_clusters - 1` clusters must participate in a pair having different signature type-word profiles or disjoint nonempty method-noun profiles. These are lexical signature evidence, not assertions of resolved type identity.

Under the example policy, `invoice_order`/`refund_invoice`, `reserve_stock`/`restock_product`, and `dispatch_parcel`/`track_shipment` are candidates for three clusters when their signature or subject evidence also separates them. Nine persistence methods on a repository pass this rule, even though the separate strict trait-size rule rejects them.

A configured cohesive suffix is exempt only when a nonignored signature word occurs in at least three quarters of its methods. Unsafe traits and explicitly sealed supertraits preserve the source boundary exemptions. Generated exclusions apply only to configured markers in the first eight lines or configured attribute names. No generated filename convention is embedded.

Findings include method spans, signature word evidence, cluster membership, and resolved implementor locations. A same-spelled trait implementation from another namespace is not attached. Reasoned directives apply to the trait. The shared AST and declaration index are reused without another full-file parser.

Migration preserves Husklet/Prop cluster, noun-priority, coverage, separation, and cohesive-protocol checks. Payment-SDK test-only method exclusion is retained. Findings become errors. Weak suffix labels such as Manager/Service are omitted because they were explanatory metadata, not necessary evidence. The strict three-method gate remains an independent rule.
