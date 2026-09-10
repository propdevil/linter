# rust/redundant-wrapper

Finds private wrappers around a local nominal struct that only forward at least three methods with identical names, receiver ownership, argument types, return types, const behavior, and visibility. Arguments must be passed unchanged and in order to the wrapped field. Every method must either forward exactly or construct the wrapper directly from its one inner value.

```toml
[[rules."rust/redundant-wrapper"]]
target = "**/*.rs"
exclude = "generated/**"
scope = "production"
min_methods = 3
```

```rust
struct Storage { inner: Store }
impl Storage {
    fn new(inner: Store) -> Self { Self { inner } }
    fn read(&self, id: u64) -> usize { self.inner.read(id) }
    fn write(&mut self, id: u64) -> bool { self.inner.write(id) }
    fn remove(&mut self, id: u64) -> bool { self.inner.remove(id) }
}
```

This is an error when local Store defines the same three method contracts. Adding validation, translation, instrumentation, a meaningful additional method, or a trait implementation preserves Storage. Public and restricted-visible wrappers are retained as boundaries. Attributes preserve contracts except ordinary standard value derives such as Debug and Clone. Generic, async, unsafe, and ABI-specific methods are conservatively retained. Signature differences, including nominally distinct newtype parameters, preserve the wrapper.

No primitive storage unwrapping occurs. WalletId(String), TransferId(String), and wrappers around external types are not compared structurally. A named or tuple field must resolve to exactly one local struct in the same package; aliases preserve that nominal identity. Tuple wrappers may qualify through direct self.0 forwarding, while tuple constructor syntax is conservatively retained. Trait implementations and non-method associated items prevent a finding.

Target and optional exclude accept root-relative globs or nonempty lists. Scope supports production (default), tests, and all; min_methods defaults to 3 and must be at least 3. Discovered files outside the target still supply inner-method and trait evidence. Test-only methods do not alter production wrapper evidence; test wrappers can use production inner methods. Each finding spans the wrapper and records every forwarder and matching inner method. Common reasoned disable directives apply.

Migrates the forwarding-wrapper branch in Husklet `ceremony/wrapper.rs` and both design-lint copies, including public/trait/validation/serialization negatives and the Payment-SDK test-helper regression. Shared Tree-sitter syntax and nominal declaration resolution replace the donor's package/name method database. Unknown or ambiguous types and signatures are retained. Macros are not expanded and compiler configuration is not evaluated beyond shared test classification.
