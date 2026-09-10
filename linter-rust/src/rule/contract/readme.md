# rust/broad-trait-responsibilities

Rejects large traits whose methods form several distinct capability clusters with supporting signature or subject evidence. Method count alone never triggers this rule; use `rust/trait-method-count` for a strict size limit.

```toml
[[rules."rust/broad-trait-responsibilities"]]
target = "**/*.rs"
min_methods = 8
min_clusters = 3
min_methods_per_cluster = 2
ignored_type_words = ["result", "option", "vec", "box", "arc", "dyn", "impl", "where", "send", "sync", "static", "error", "bool", "str", "string", "usize", "isize", "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128"]
cohesive_suffixes = ["Protocol", "Codec", "Visitor", "Renderer", "Commands"]
generated_markers = ["@generated", "automatically generated"]
generated_attributes = ["automatically_derived", "proc_macro_derive"]
capabilities = [
  { name = "persistence", verbs = ["create", "open", "read", "write", "save", "load", "delete", "remove", "list", "find", "get", "put"] },
  { name = "lifecycle", verbs = ["start", "stop", "pause", "resume", "restart", "kill", "launch", "terminate"] },
  { name = "observation", verbs = ["inspect", "status", "stats", "health", "metrics", "describe", "query"], nouns = ["metric", "metrics", "stat", "stats", "status", "health"] },
  { name = "configuration", verbs = ["configure", "set", "update", "apply", "reset", "enable", "disable"] },
  { name = "events", verbs = ["subscribe", "unsubscribe", "watch", "emit", "notify", "poll"], nouns = ["event", "events", "notification", "notifications"] },
  { name = "transfer", verbs = ["upload", "download", "push", "pull", "import", "export", "copy"] },
  { name = "authorization", verbs = ["login", "logout", "authenticate", "authorize", "grant", "revoke"], nouns = ["auth", "permission", "permissions", "credential", "credentials"] },
  { name = "connection", verbs = ["connect", "disconnect", "bind", "listen", "accept", "send", "receive"] },
  { name = "rendering", verbs = ["render", "draw", "present", "commit", "frame", "paint"] },
  { name = "traversal", verbs = ["visit", "walk", "fold", "traverse"] },
  { name = "codec", verbs = ["encode", "decode", "serialize", "deserialize", "parse", "format"] },
  { name = "clipboard", nouns = ["clipboard"] },
  { name = "window", nouns = ["window", "windows", "surface", "interaction"] },
]
```

This usable preset reproduces the source vocabulary. Capability configuration is required; no hidden repository vocabulary is supplied by code. `target` and optional `exclude` accept a glob or nonempty list. Scope defaults to `production`; `tests` and `all` are supported. Threshold defaults are 8/3/2. Minimum clusters must be at least two; all thresholds must be positive, supported by the vocabulary, and mutually coherent. Duplicate vocabulary within the verb or noun namespace is rejected rather than resolved by table order.

A method's first configured noun after its leading word takes precedence over its leading verb. Retain clusters with at least `min_methods_per_cluster` methods. At least `min_clusters` clusters and half the trait's methods must remain classified. At least `min_clusters - 1` clusters must participate in a pair having different signature type-word profiles or disjoint nonempty method-noun profiles. These are lexical signature evidence, not assertions of resolved type identity.

For example, a trait containing `load_wallet`/`save_wallet`, `start_sync`/`stop_sync`, `inspect_checkpoint`/`query_height`, and `configure_rpc`/`update_rpc` fails. Nine persistence methods on a repository pass this rule, even though the separate strict trait-size rule rejects them.

A configured cohesive suffix is exempt only when a nonignored signature word occurs in at least three quarters of its methods. Unsafe traits and explicitly sealed supertraits preserve the source boundary exemptions. Generated exclusions apply only to configured markers in the first eight lines or configured attribute names. No generated filename convention is embedded.

Findings include method spans, signature word evidence, cluster membership, and resolved implementor locations. A same-spelled trait implementation from another namespace is not attached. Reasoned directives apply to the trait. The shared AST and declaration index are reused without another full-file parser.

Migration preserves Husklet/Prop cluster, noun-priority, coverage, separation, and cohesive-protocol checks. Payment-SDK test-only method exclusion is retained. Findings become errors. Weak suffix labels such as Manager/Service are omitted because they were explanatory metadata, not necessary evidence. The strict three-method gate remains an independent rule.
