# provekit-lift-flux

ProvekIt lift adapter for [flux](https://flux-rs.github.io/flux/).

## Strategic positioning

ProvekIt consumes flux's existing annotations; we sit beneath, not against. Flux is a refinement-type checker for Rust. Developers keep their existing flux annotations; this adapter reads what is already there and promotes it to a content-addressed signed contract.

## Recognized attribute (v0)

Only `#[flux::sig(...)]` (and the alias `#[flux_rs::sig(...)]`) is translated. The body is hand-parsed off the TokenStream because flux refinement syntax is not standard Rust.

```rust
#[flux::sig(fn(x: i32{x > 0}) -> i32{r: r >= 0})]
fn double(x: i32) -> i32 { x + x }
```

This produces:
- `pre = forall x: Int. x > 0`
- `post = forall x: Int. out >= 0` (binder `r` rewritten to `out`)

## v0 whitelist

- Argument types are simple integer-shaped idents (`i8` through `i64`, `u8` through `u64`, `usize`, `isize`).
- Refinement body is a binop comparison: one of `>`, `>=`, `<`, `<=`, `==`, `!=`.
- Each operand is a var (ident), an integer/string literal, or a single-arg call (treated as ctor).
- Return refinements may use an explicit binder (`r: r >= 0`); the binder is rewritten to `out` to match the contract envelope.

## Skipped with structured warnings (v0)

- All flux attributes other than `#[flux::sig]` (e.g. `#[flux::trusted]`, `#[flux::refined_by]`).
- Tuple, list, set, and otherwise non-numeric refinements.
- Refinement bodies outside the v0 binop whitelist.
- Non-ident type tokens in the signature (paths, generics).

## Hash + signature stack

BLAKE3-512 + ed25519. Same as every other adapter in the kit.
