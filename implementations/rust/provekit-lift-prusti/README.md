# provekit-lift-prusti

ProvekIt lift adapter for [prusti](https://www.pm.inf.ethz.ch/research/prusti.html).

## Strategic positioning

ProvekIt consumes prusti's existing annotations; we sit beneath, not against. Prusti is an ETH-Zurich verifier built on Viper. Developers keep their existing prusti annotations; this adapter reads what is already there and promotes it to a content-addressed signed contract.

## Recognized attributes (v0)

Only the namespaced forms are matched, by design:

- `#[prusti::requires(<expr>)]` and `#[prusti_contracts::requires(...)]` -> `pre`
- `#[prusti::ensures(<expr>)]` and `#[prusti_contracts::ensures(...)]` -> `post`
- `#[prusti::invariant(<expr>)]` and `#[prusti_contracts::invariant(...)]` -> `inv`

The bare `#[requires(...)]` form is intentionally left to `provekit-lift-contracts` so the two adapters do not double-lift the same attribute.

In `#[prusti::ensures]`, prusti uses `result` as the return-value placeholder; we rewrite `result` to `out` to match the contract envelope's `outBinding` (default `"out"`).

## Skipped with structured warnings (v0)

- `#[prusti::predicate]`, `#[prusti::trusted]`, `#[prusti::pure]`, `#[prusti::ghost]` items.
- `forall!` / `exists!` quantifier macros inside contract expressions.
- Anything outside the v0 binop whitelist (method calls, indexing, field access, multi-arg non-ctor calls, complex nesting).

## v0 whitelist

Same as proptest/contracts: `<var|lit|single-arg-call> <binop> <var|lit|single-arg-call>` where `binop` is one of `>`, `>=`, `<`, `<=`, `==`, `!=`. Honest under-coverage beats polluting the lattice with unverifiable atoms.

## Hash + signature stack

BLAKE3-512 + ed25519. Same as every other adapter in the kit.
