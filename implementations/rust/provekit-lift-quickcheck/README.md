# provekit-lift-quickcheck

ProvekIt lift adapter for [quickcheck](https://crates.io/crates/quickcheck).

## Strategic positioning

ProvekIt consumes quickcheck's existing annotations; we sit beneath, not against. Developers keep their existing `#[quickcheck]` properties; this adapter walks the AST, pulls the function-as-predicate, and promotes it to a content-addressed signed contract.

## Recognized shapes (v0)

A quickcheck property is a function attributed with `#[quickcheck]` whose parameters are the universally-quantified domain and whose body is the predicate.

```rust
#[quickcheck]
fn prop_nonneg(a: i64) -> bool {
    a >= -1
}
```

This adapter recognizes:

- `#[quickcheck]`
- `#[quickcheck::quickcheck]`

The function must return `bool`. Multi-statement bodies, `TestResult` returns, and bodies outside the v0 whitelist skip with a logged warning.

## v0 whitelist

- Single tail expression body.
- Comparison: one of `>`, `>=`, `<`, `<=`, `==`, `!=`.
- `&&` connecting two comparisons.
- Each side of a comparison: var (ident), integer/string literal, or single-arg call (treated as ctor).

Anything fancier (arithmetic, method calls, field access, indexing, multi-arg calls, `||`) skips with a warning. Honest under-coverage beats polluting the lattice with unverifiable atoms.

## Each lifted property maps to

A `ContractDecl` with:
- `pre = None`
- `post = None`
- `inv = Some(forall p1 ... forall pN. body)`
- `out_binding = "out"`

## Hash + signature stack

BLAKE3-512 + ed25519. Same as every other adapter in the kit.
