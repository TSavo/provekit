# Vendored source provenance — java-mt-reference

Every vendored file is verbatim from its upstream tag. The point contracts
lifted by the kit trace, character by character, to `LiteralTree` nodes in
the vendor's own test assertions.

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
  method `testMakotoNishimura`, lines 32-39 (tag `rel/commons-rng-1.7`)
- Raw URL: https://raw.githubusercontent.com/apache/commons-rng/rel/commons-rng-1.7/commons-rng-core/src/test/java/org/apache/commons/rng/core/source32/MersenneTwisterTest.java
- sha256 of `MersenneTwisterTest.java` at that tag:
  `6c849428f8eae282effedb6db64262f214477dd8382e0396eab948bb43e1f911`
  (cited, not vendored — the GOOD suite carries the assertions itself)

The reference values originate from Matsumoto's original output file:
  http://www.math.sci.hiroshima-u.ac.jp/~m-mat/MT/MT2002/CODES/mt19937ar.out
converted to hexadecimal by the vendor. The vendor's `testMakotoNishimura`
asserts these values via `RandomAssert.assertEquals(expectedSequence, rng)`,
which internally calls `Assertions.assertEquals(expected[i], rng.nextInt())`
for each position — the sworn per-draw equality contracts we lift here.

Lifted values (seed `{0x123, 0x234, 0x345, 0x456}`, draws 0-7):
  draw[0] = 0x3fa23623
  draw[1] = 0x38fa935f
  draw[2] = 0x1c72dc38
  draw[3] = 0xf4cf2f5f
  draw[4] = 0xfc110f5c
  draw[5] = 0xc75677aa
  draw[6] = 0xc802152f
  draw[7] = 0x0d9155da

## Honest scope

FLOOR only. This showcase proves:
  "for seed {0x123, 0x234, 0x345, 0x456}, draw[N] = REF_VALUE"
as a point contract sworn by the vendor (bin-1, deterministic theorem).

It does NOT prove:
  "the output is derivable from the seed via the MT algorithm"
That requires the tempering universe + seed-state walk (rungs 2/3, following
the base64 strong-tier campaign). Do NOT claim derivation here.

## JUnit framework source (assertion vocabulary)

`good/vendor/junit5/` and `bad/vendor/junit5/` are copied from
`examples/java-callbind-consistency/*/vendor/junit5/` — see the
`PROVENANCE.md` inside those directories (junit5 tag `r5.10.2`).
