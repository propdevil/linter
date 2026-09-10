# rust/unsafe-boundary

Confines unsafe blocks, functions, methods, traits, implementations and foreign
blocks to explicitly configured boundaries. Inside those boundaries, every
unsafe block still needs a real, attached, nonempty `SAFETY:` rationale.

```toml
[[rules."rust/unsafe-boundary"]]
target = "**/src/**/*.rs"
exclude = "generated/**"
scope = "production"
allowed_targets = ["native/**/*.rs", "apps/api/src/ffi.rs"]
allowed_modules = ["ffi", "platform::adapter"]
```

`target` is required; `exclude` and `allowed_targets` are optional selectors,
each accepting a pattern or nonempty list. `allowed_modules` defaults to empty;
entries are exact Rust module paths and include descendants. They are evaluated
within files selected by this block. Similar names such as `not_ffi` or
`ffi_adapter` do not match `ffi`. `scope` is `production` (default), `tests`, or
`all`. No repository paths, package names or approved module names are embedded.

```rust
mod ffi {
    unsafe fn caller_contract() {} // Item requires boundary, not block rationale.
    fn read() {
        // SAFETY: The pointer remains aligned and live throughout this read.
        unsafe { /* minimal operation */ }
    }
}
```

Rationales may appear immediately before the block/containing statement or at
the start of its body. Contiguous line/block comments and multiline explanations
count. Empty markers, string contents, blank gaps and intervening code do not.
Nested unsafe blocks each require their own rationale. Unsafe written inside
macro definitions and arguments is checked through shared syntax tokens, without
macro expansion; macro arguments do not receive a rationale exemption.

`allow`, `warn`, or `expect` of `unsafe_code` outside an approved boundary is an
error, including crate/module attributes and conditional `cfg_attr`. Such
attributes never authorize a boundary. `deny` and `forbid` are not weakening.
`unsafe_op_in_unsafe_fn` is a separate compiler lint and not treated as a change
to `unsafe_code`. Compiler-enforced workspace `forbid` remains authoritative even
inside a configured boundary. Current reasoned directives remain supported.

Migration: replaces Husklet `rule/rust/safety/mod.rs` and `test.rs`. Preserves all
unsafe construct forms, explicit source/module boundaries, block rationale,
long-comment, macro and similar-name cases. Replaces implicit `allow(unsafe_code)`
authorization, nearby-comment windows and macro-argument exemptions with the
stricter configuration and attached-comment policy. Declarations retain the
donor's boundary-only requirement; their contained unsafe blocks need rationale.
