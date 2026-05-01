# provekit-lift-verus

ProvekIt lift adapter for [verus](https://github.com/verus-lang/verus).

## Strategic positioning

ProvekIt consumes verus's existing annotations; we sit beneath, not against. Verus is a Rust-embedded verifier with its own pre-processor: `requires`, `ensures`, `decreases`, `invariant` clauses and `spec fn` / `proof fn` items appear inside `verus! { ... }` blocks and are not standard Rust expressions. Parsing them requires verus's own syntax extension layer.

## v0 status: documented gap

This adapter does not lift verus contracts. It detects every `verus! { ... }` macro invocation, scans the inner TokenStream for `fn` / `spec fn` / `proof fn` identifiers (best-effort, for naming only), and emits one structured `LiftWarning` per detected item.

```
verus! { ... } block detected; v0 does not lift verus syntax
(gap documented in README; revisit in v1.2)
```

The honest log is the right shape for v0. Pretending coverage we don't have pollutes the lattice.

## What v1.2 will need

Verus runs the contents of `verus! { ... }` through `verus_macro`, which translates the embedded language to standard Rust before the compiler sees it. To lift:

1. Either depend on `verus-macro` and let it normalize the block first, then re-parse with `syn` and treat the standardized `requires`/`ensures` calls as candidates.
2. Or hand-implement a verus-aware sub-parser that peels `requires <expr>, ...` and `ensures <expr>, ...` clauses off the start of each function body.

Both options carry real cost. v0 takes the cut.

## Recognized patterns (v0)

- `verus! { ... }` macro invocation: emits warning(s); lifts nothing.
- Item names inside the block (`fn name`, `spec fn name`, `proof fn name`): captured for diagnostic context, no IR translation.

## v0 whitelist for future work

When implemented, the same v0 whitelist as `provekit-lift-contracts` applies: binop comparison with var/literal/single-arg-call operands. Anything fancier skips with a logged warning.

## Hash + signature stack

BLAKE3-512 + ed25519. Same as every other adapter in the kit.
