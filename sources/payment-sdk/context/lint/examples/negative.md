# Negative design examples

## God trait

```rust
trait ChainService {
    fn wallet(&self);
    fn index(&self);
    fn build(&self);
    fn broadcast(&self);
}
```

This couples unrelated responsibilities and exceeds the three-function trait
limit. Use existing wallet, transaction, and indexing capabilities.

## Empty namespace type

```rust
struct TransactionCodec;
```

Use a module or functions unless the type owns meaningful state.

## Repeated chain vocabulary

Inside a concrete chain crate, use `Address` rather than repeating the chain
name in every type. Outside that crate, do not mention the concrete chain at
all except at the application composition boundary.

## Leaking Solana ownership

```rust
struct SolanaCheckpoint;
```

This name is invalid in base, indexing, wallets, packages, and sibling chain
crates. Use the chain-neutral `BlockRef` contract there, or move the concrete
concept into `sdk/chains/solana` or application composition.
