# Vendored source provenance — java-mt-strong

The STRONG-tier sibling of `java-mt-reference`. The same vendored Mersenne
Twister is here WALKED inter-procedurally: the kit derives each reference value
from the vendor's own seed→state→twist→temper pipeline (for the literal seed),
rather than only checking the per-draw assertions for within-test contradiction.

Every vendored file is verbatim from its upstream tag.

## Apache Commons RNG — tag `rel/commons-rng-1.7`

Source: https://github.com/apache/commons-rng, tag `rel/commons-rng-1.7`.
License: Apache-2.0.

| File (under `good/vendor/commons-rng/` and `bad/vendor/commons-rng/`) | Upstream path | sha256 |
|---|---|---|
| `MersenneTwister.java` | `commons-rng-core/src/main/java/org/apache/commons/rng/core/source32/MersenneTwister.java` | `7531257b30da0774738fba92f78128e2a96c6c563b3318e54534e1661c03b0ba` |

Raw URL:
- https://raw.githubusercontent.com/apache/commons-rng/rel/commons-rng-1.7/commons-rng-core/src/main/java/org/apache/commons/rng/core/source32/MersenneTwister.java

## Reference vector source (the sworn spec)

The GOOD suite's assertions are the vendor's OWN reference vectors from:

- `commons-rng-core/src/test/java/org/apache/commons/rng/core/source32/MersenneTwisterTest.java`
  method `testMakotoNishimura` (tag `rel/commons-rng-1.7`)
- Raw URL: https://raw.githubusercontent.com/apache/commons-rng/rel/commons-rng-1.7/commons-rng-core/src/test/java/org/apache/commons/rng/core/source32/MersenneTwisterTest.java

The reference values originate from Matsumoto's original output file:
  http://www.math.sci.hiroshima-u.ac.jp/~m-mat/MT/MT2002/CODES/mt19937ar.out

Lifted values (seed `{0x123, 0x234, 0x345, 0x456}`, draws 0-7):
  draw[0] = 0x3fa23623   draw[1] = 0x38fa935f   draw[2] = 0x1c72dc38   draw[3] = 0xf4cf2f5f
  draw[4] = 0xfc110f5c   draw[5] = 0xc75677aa   draw[6] = 0xc802152f   draw[7] = 0x0d9155da

## What is WALKED (the strong tier)

The kit (`MtSeedingWalker` in `implementations/java/sugar-lift-java-tests`) walks
the vendor's entire seed→state→draw pipeline inter-procedurally:

  constructor(int[] seed) → setSeedInternal(seed) → fillStateMersenneTwister(mt, seed)
    → initializeState(state)       [MersenneTwister.java L182-189, forward i++, bound state.length=624]
    → mixSeedAndState(state, seed) [L205-227, countdown k--, cursors i,j, Math.max(N, seed.length)]
    → mixState(state, nextIndex)   [L237-252, countdown k--, cursor i]
    → state[0] = UPPER_MASK        [L174]
  nextInt() → next() → the twist [L259-277, three k++ sweeps, MAG01 low-bit gate]
                       + tempering [L282-285]

Every constant (19650218, 1812433253L, 1664525L, 1566083941L, UPPER_MASK,
MAG01, N=624, M=397), operator, shift, mask, and array index is read from a
`com.sun.source` tree node in `MersenneTwister.java`; an uninterpretable node is
refused by name. The seed→state fold is re-verified against an independent
recompute; the twist+tempering is walked into the FOL, not faked.

The pinned contract is `mt32.eq-seeded(refValue, <walked recurrence>)`, carried
as an SSA `let`-chain so the 624-deep recurrence shares sub-terms.

## GOOD vs BAD

- GOOD: the 8 vendor-sworn reference values are each consistent with the walked
  recurrence → DISCHARGED by DERIVATION (z3 `unsat` on `(not (= ref walked))`).
- BAD (`MersenneTwisterWrongValueTest`): a SINGLE wrong-by-one-bit value
  (`0x3fa23624`, no second contradictory claim) → UNSATISFIED by the walked
  recurrence (z3 `sat`). The refutation is computation-driven (the real MT
  algorithm over the real seed), NOT a within-test contradiction — the
  qualitative leap over the FLOOR rung `java-mt-reference`.
