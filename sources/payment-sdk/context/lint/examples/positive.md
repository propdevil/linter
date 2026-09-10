# Positive design examples

## Small reusable capability

```rust
trait Addresser {
    fn address(&self) -> Address;
}
```

The trait names one reusable capability and stays independent of concrete
chains.

## Chain-native execution

Concrete transaction construction remains in its owning chain. Generic wallet
code invokes a small transaction capability and never interprets scripts,
UTXOs, nonces, gas, envelopes, or signatures.

## One composition root

`apps/api` constructs concrete RPC clients, embedded indexers, storage, and
wallet providers once. Handlers depend on wallet and indexing abstractions.

## Concrete Solana ownership

`sdk/chains/solana` may depend only on generic packages, base values, indexing,
and wallets. Only the application and acceptance layers may depend on that
concrete chain, and `solana`/`sol` vocabulary stays in those composition or
chain-owned paths.
