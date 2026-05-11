# Cross-Language Port Demonstrator

This exhibit is the minimal real C11-to-Rust transport path through primitive concept hub nodes.
It is intentionally narrow: the accepted program is `foo.c`, and unsupported operations refuse instead of guessing.

## Command

```sh
menagerie/cross-language-port/driver.sh
```

The driver builds the existing C term projector, builds `provekit`, runs:

```sh
provekit transport menagerie/cross-language-port/foo.c --to rust --function foo --out menagerie/cross-language-port/artifacts --json
```

Then it checks that `artifacts/concept.term.json` and `artifacts/roundtrip.concept.term.json` are byte-identical.

## Worked Artifact

Input C:

```c
static int foo(int x) {
    if (x == 0)
        return -22;
    return x;
}
```

Produced Rust:

```rust
pub fn foo(x: i32) -> i32 {
    if x == 0 {
        return -22;
    }
    return x;
}
```

Artifacts:

- `artifacts/c11.term.json`: normalized C11 algebra term with op CIDs.
- `artifacts/concept.term.json`: transport image in the concept hub.
- `artifacts/rust.term.json`: Rust-algebra term.
- `artifacts/foo.rs`: minimal realized Rust source.
- `artifacts/roundtrip.concept.term.json`: Rust term transported back to the same concept term.
- `artifacts/transport-report.json`: command report, normalizations, and receipt CIDs.

The C projector emits `bop_eq` and `uop_neg(22)` for this source. The CLI normalizes only the safe subset used here: side-effect-free integer `bop_eq` to primitive `c11:eq`, and literal unary minus to the signed integer constant `-22`. Any other unmapped source op is a `Refusal`.

## Minted Concept Hubs

Primitive operation shape nodes added to `menagerie/concept-shapes/specs/`:

- `concept:conditional`: `blake3-512:46e8de67d7268a149d7429c115780cb286357fbed902dfd36618930c4ea9661e34e5e04a102f2864494e57c5db9410fa019dc85c39e2f8b431f616a05a010fdd`
- `concept:seq`: `blake3-512:3c4a1439095b9f4117290c71f63945331bca57ae1aa82ff0b7a3a97599615d8b8ea286658874e2a9ee811804d84a3febdb4538d9e0b84fbb8962b88034a90412`
- `concept:return`: `blake3-512:57981a2c7d25fd2afe840d6d30e503b6c04b98e5c909457dee1083f852fa746e0addb62c00b0d39e951193ce2a1fe75e36f7728625d53224f3a9adee3b690607`
- `concept:eq`: `blake3-512:17d64f7bce0c5aa8b7bc81afd57bcb2a54a5d53ac626166ff2203773c8fdc6171fce7ff059e9de042909097ffc9f24ee3055801596b523d5677d7ccd45048ef4`
- `concept:skip`: `blake3-512:a66930e06053bacfd74f398b69e199cbe5e35aa4f517bcbd2787b53a549966f6eb4ecca7b01d4a505b73893605ce5b09af843df8e0489d5181a2e9a633e0f90c`

Each has C11 and Rust `op -> concept` morphism specs plus unsigned `MorphismDischargeReceipt` artifacts in `menagerie/concept-shapes/receipts/` and `catalog/receipts/`. The receipts are unsigned because the local provenance signing key is not available; they are still content-addressed and discharged by canonicalizer alpha-equivalence plus the representation/operator map.

## CLI Surface

There is an existing `provekit migrate` protocol for catalog-version migration, not program ports. This slice therefore wires `provekit transport <src-file> --to rust`. A future protocol should reserve `provekit migrate <src> --to <lang>` for the program-port workflow.

## Specification Changes

`rust:if` now has the missing `arity_shape` named slots `cond`, `then_branch`, and `else_branch`. The same required slot policies were added to Rust `seq`, `return`, `eq`, and `skip` so their morphisms can discharge byte-identically.

The Rust language signature was re-minted. New Rust signature CID:

`blake3-512:6e96976ee181cc32de6dfb326b9b9a96e5f47b7ba8afef9606d93cee15984fc1c81de78491da094a788bf50725b26824e22b16d79a5b80dc76cd169c59aa844c`

No `.provekit/ci/accepted/*`, `.provekit/self-contracts-attestations/*`, or pins were touched. Any accepted-set refresh is left to CI or a separate CICP regeneration step.

## Deferred

- General C11 desugaring-collapse beyond this `foo` slice, including the architecture from `protocol/specs/2026-05-11-desugaring-and-the-core-compression.md`.
- Additional primitive op hubs and morphisms for Python, TypeScript, Go, C#, Zig, and the rest of Rust/C11.
- A signed receipt path using the provenance key.
- Rust source re-lift through `provekit-walk` as a stable `provekit transport` substep. I tried the existing `provekit-walk` Cargo manifest directly; Cargo currently refuses it because the crate believes it is in the Rust workspace while not listed as a workspace member. This exhibit therefore closes the checked roundtrip at the Rust-algebra term back to concept IR.
- Transporting the C `.proof` function contract to a Rust `.proof` under Lemma 4. The operation `wp` contracts are preserved by the op morphisms here, but proof envelope transport is not wired in this slice.
- Bytecode path: `ifz -> concept:conditional-jump -> concept:conditional`, with a reducibility precondition.
- Broader realize-side coverage beyond the tiny Rust subset generated for `foo`.
