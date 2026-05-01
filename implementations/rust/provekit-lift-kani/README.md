# provekit-lift-kani

ProvekIt lift adapter for [Kani](https://model-checking.github.io/kani/),
AWS's bounded model checker for Rust.

## Strategic positioning

ProvekIt does not compete with Kani. Kani is the verifier; we sit
beneath it.

If your codebase already has `#[kani::requires]` and `#[kani::ensures]`
annotations, this adapter promotes each of them into a content-addressed,
ed25519-signed contract memento that ships in a `.proof` catalog. Your
Kani harness keeps running unchanged. The annotations you already wrote
are now also a portable, verifiable artifact: a CID anyone can re-derive
and a signature anyone can re-check.

The adoption order is intentional:

1. Lift first. Whatever annotations you already have become contracts.
2. Mint via `provekit-macros` only when greenfield (no existing
   annotation library to read from).

ProvekIt does not generate Kani's proof obligations and does not
duplicate Kani's bounded model checking. We content-address what Kani
already taught your code to say.

## What gets lifted (v0)

| Kani attribute            | Lift target              |
|---------------------------|--------------------------|
| `#[kani::requires(expr)]` | `pre`                    |
| `#[kani::ensures(expr)]`  | `post`                   |
| `#[kani::should_panic]`   | skipped with warning (v1)|
| `#[kani::proof]`          | skipped (entry marker)   |
| `#[kani::unwind(N)]`      | skipped (loop bound)     |

Within `requires`/`ensures`, the v0 expression whitelist is the same
as the proptest and contracts adapters:

```
<var | int-lit | str-lit | single-arg-call> <binop> <same>
```

with `binop` in `>`, `>=`, `<`, `<=`, `==`, `!=`. Anything outside
(method calls, field access, indexing, multi-arg calls, complex
nesting, logical connectives) is skipped with a warning. Honest
under-coverage beats lattice pollution.

## Kani's `result` binding

Kani's `ensures` uses the identifier `result` for the return value.
Our canonical out-binding is `out`. The adapter rewrites `result`
references inside `ensures` predicates to `out()` so the lifted IR is
uniform across adapters.

```rust
#[kani::ensures(result >= 0)]
fn sqrt(x: i64) -> i64 { x }
```

lifts to (forall x: Int. out >= 0).

## v1 plans

- `#[kani::should_panic]` will lift to an `inv` flagged with a
  kit-defined `should_panic` ctor so negation markers are captured
  rather than dropped.
- Conjunction in predicates (`x > 0 && x < 100`) will decompose into a
  conjunction of liftable atoms.
