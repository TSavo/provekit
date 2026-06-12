# Assertion-accounting ledger — total accounting over sugar's own Rust tree

The companion axis to [`GOAL-sugar-proves-sugar.md`](./GOAL-sugar-proves-sugar.md).
That doc tracks **K** (panic-safe call sites discharged via sound reasoning,
measured by `sugar self-check` with the rust-analyzer oracle). This ledger
tracks the other half of "sugar proves sugar": the **lift homomorphism's total
accounting** — of every assertion in sugar's own test surface, how many lift to
FOL, how many are refused **by name**, and how many are **silently dropped**.

The hard invariant is the same one the whole product rests on: **silent = 0,
structurally.** Every `assert*` macro is either lifted to a FOL contract or
refused with a named reason; none vanishes.

## Recompute it yourself (no oracle, pure source)

```
cargo build -p sugar-lift-rust-tests --bin coretests_sweep
for c in libsugar sugar-ir-compiler-smt-lib sugar-cli sugar-verifier sugar-walk; do
  ./implementations/rust/target/debug/coretests_sweep \
    implementations/rust/$c/src --json /tmp/sweep-$c.json
done
```

`unaccounted (net)` mixes two signs; the load-bearing number is
`genuinely unreached (SILENT)` — a positive per-file residual, the true silent
drop. A negative residual is *inlining inflation* (one textual assert lifted as
several point-wise instances because a helper was inlined at N call sites), not
a drop. The sweep separates them.

## Opening baseline (main `e15b0ac00`, 2026-06-11)

| crate | asserts | lifted→FOL | refused (named) | SILENT | assertion-multiset CID |
|---|---:|---:|---:|---:|---|
| `libsugar` | 202 | 173 | 29 | 0 | `blake3-512:7c3a076104eab26fc178a2c3aef4a25bc21aa5b1d9c10306f015cf24d2fdb811fdb4ed592ed56a0beaad51656a3e35a3ecf4f4ba9ee6c94c67b95707e711988c` |
| `sugar-ir-compiler-smt-lib` | 250 | 229 | 21 | 0 | `blake3-512:f0115fdfb05215a389fb7882393fb7e733d0d483eec479759d49e6742e2d69bc5a2bab667c132c18b264fa4c9d9c5137f7f0067fc9dde5d2138044b41e17e516` |
| `sugar-cli` | 640 | 576 | 84 | 0 | `blake3-512:e1b035c232ceaac79528fac7a01ac84e119d0a9fbc364d445c98d422688ebada0d7fb88d64aa990633408e916ef49d351d4cfb3ac8a024b77ea382ed1315e3e0` |
| `sugar-verifier` | 395 | 309 | 86 | 0 | `blake3-512:e7f8583506f29f9c47390f6f872dafef08ad61b8adc569fad27fef3944ef0568642790347c2a52c3fd22e15481151d62b29de4fd4615406a541f3ecbcf087991` |
| `sugar-walk` | 1172 | 998 | 212 | 0 | `blake3-512:052afe9e0d9f4426953a193c8096c686ef795531bd4465286e7c1955993dd6b9c10464837c9f95f0046205e29364010d18b01597c705fc1e70f903f57b88ce07` |
| **total** | **2659** | **2285 (85.9%)** | **432** | **0** | — |

The multiset CID pins the assertion *surface*: a count-preserving swap still
moves the CID, so a silent regression cannot hide behind an unchanged total.

## The 432 named-refused, decomposed (the bin-1 burndown)

The construction-semantics axiom ([[project_provekit_construction_semantics_axiom]])
sorts every refusal into **bin-1** (constructed from literals, but the walker
doesn't speak that constructor yet — *drains*) or **bin-2** (never constructed
by the source — IO/clock/allocator, or here, procedural meta-test scaffolding —
*named, never proved*).

| category | count | bin | meaning / drain path |
|---|---:|---|---|
| **drainable term-shape** | 165 | bin-1 | `only scalar equality is liftable` (72), `unsupported term` (91). Teach the assertion lifter structural/componentwise equality and the missing term shapes. |
| **control-flow-released** | 115 | bin-1 | `assertion under for/if/match context … released to later pass` (88), `unenumerated statement position` (27). Drains when the loop/conditional assertion pass matures. |
| **temporal identity** | 81 | bin-1/bin-2 edge | `ambiguous temporal identity for receiver` — a mutated receiver has no value allocated-at-formation. SSA/guard-lifter tracking drains the stabilizable ones; genuinely mutated state is bin-2 by the allocation axiom. |
| **meta-test scaffolding** | 71 | bin-2 | sugar's own `assert_panic_locus_lines`, `assert_*_fails_closed`, `assert_kit_declaration_mappings`, etc. — procedural tests of the tooling itself, asserting about lift internals, not value constructions. Never FOL; named forever. |

**The single tracking number for this axis: drainable bin-1 = 280** (term-shape
165 + control-flow-released 115), with 81 on the temporal edge to be
adjudicated per the allocation axiom, and 71 honest structural bin-2.
**silent = 0 (hard invariant, held).** Drive drainable bin-1 → 0.

## Drain order (M3 worklist)

1. **Structural equality** — `only scalar equality is liftable` (72): lift
   `assert_eq!(a, b)` where `a`/`b` are structs / tuples / collections as
   componentwise equality, not just scalars. Biggest single bucket.
2. **Unsupported term shapes** (91): enumerate the distinct `unsupported term`
   shapes (`reason_samples` in the JSON ledger) and teach them one family per
   slice (the convergence pattern — each slice teaches one constructor).
3. **Control-flow-released** (115): the loop/conditional assertion pass picks up
   `for`/`if`/`match`-bound assertions as guarded point-wise rows.
4. **Temporal identity** (81): apply the guard-lifter SSA discipline
   ([[project_provekit_guard_lifter_soundness_pattern]]) to stabilize
   non-mutated receivers; name the genuinely-mutated remainder as bin-2.

Each slice updates this table with the new number and a one-line why, exactly as
`GOAL-sugar-proves-sugar.md` requires for K.
