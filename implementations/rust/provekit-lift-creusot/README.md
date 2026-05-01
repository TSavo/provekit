# provekit-lift-creusot

ProvekIt lift adapter for [creusot](https://github.com/creusot-rs/creusot).

## Strategic positioning

ProvekIt consumes creusot's existing annotations; we sit beneath, not against. Creusot is a deductive verifier for Rust targeting Why3. Developers keep their existing creusot annotations; this adapter reads what is already there and promotes it to a content-addressed signed contract.

## Recognized attributes (v0)

Only the namespaced forms are matched, by design:

- `#[creusot::requires(<expr>)]` and `#[creusot_contracts::requires(...)]` -> `pre`
- `#[creusot::ensures(<expr>)]` and `#[creusot_contracts::ensures(...)]` -> `post`
- `#[creusot::invariant(<expr>)]` and `#[creusot_contracts::invariant(...)]` -> `inv`

The bare `#[requires(...)]` form is intentionally left to `provekit-lift-contracts` so the two adapters do not double-lift the same attribute.

In `#[creusot::ensures]`, creusot uses `result` as the return-value placeholder; we rewrite `result` to `out` to match the contract envelope's `outBinding`.

## Skipped with structured warnings (v0)

- `#[creusot::predicate]`, `#[creusot::law]`, `#[creusot::trusted]` items.
- `#[creusot::variant(...)]` termination measures (v0 does not model variants).
- Anything outside the v0 binop whitelist.

## v0 whitelist

Same as proptest/contracts: `<var|lit|single-arg-call> <binop> <var|lit|single-arg-call>` where `binop` is one of `>`, `>=`, `<`, `<=`, `==`, `!=`. Honest under-coverage beats polluting the lattice with unverifiable atoms.

## Hash + signature stack

BLAKE3-512 + ed25519. Same as every other adapter in the kit.
