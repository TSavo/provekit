# Supply-Chain Proof Run — signup-service

**Date:** 2026-06-10
**Kit:** `JavaTestAssertionsRpc` v0.7.0 (P5c: call-binding lift)
**Sugar CLI:** debug build from main
**JUnit5 vendor:** `tests/fixtures/vendor/junit5/` (Assertions.java, AssertEquals.java, …)

## Result

| Status | Count |
|--------|-------|
| PROOF  | 4     |
| GAP    | 15    |

**Total resolved artifacts:** 19
**Proven:** 4
**Perimeter (GAP):** 15

## Proven artifacts (per-artifact contract count)

These counts are the real number of contracts the kit lifts from each vendor's
own JUnit5 test sources — the exact `ir` array that `sugar mint` consumes. They
are reported by `prove.sh` itself (via `count_lifted_contracts`), not tallied by
hand.

| Artifact | Test files | Contracts proven | Refused-by-name |
|----------|-----------:|-----------------:|----------------:|
| commons-codec-1.16.1   |  69 |   81 | 2028 |
| commons-io-2.15.1      | 239 |  785 | 3385 |
| commons-lang3-3.14.0   | 229 | 3067 | 17594 |
| commons-text-1.11.0    | 106 | 1118 | 1496 |

**Total: 5051 contracts lifted across the 4 proven artifacts.**

The refused-by-name counts are honest and specific — each refusal names exactly
why that assertion was not lifted (e.g. "expected arg is not an int/String
literal", "call arg to join(...) is not a literal", "assertThrows — not an
assertion"). The refusals are the within-artifact perimeter: the assertions the
vendor wrote that the current kit cannot yet lift soundly.

## Perimeter (GAP) — the map of what nobody warranted

All 15 GAP artifacts publish **no `-test-sources.jar`** to Maven Central. There
is no vendor spec to lift — the GAP is the precise enumeration of supply-chain
nodes with no published test surface.

```
apiguardian-api-1.1.2            — no -test-sources.jar on Central
classmate-1.5.1                  — no -test-sources.jar on Central
expressly-5.0.0                  — no -test-sources.jar on Central
gson-2.10.1                      — no -test-sources.jar on Central
hibernate-validator-8.0.1.Final  — no -test-sources.jar on Central
jakarta.el-api-5.0.0             — no -test-sources.jar on Central
jakarta.validation-api-3.0.2     — no -test-sources.jar on Central
jboss-logging-3.4.3.Final        — no -test-sources.jar on Central
junit-jupiter-5.10.2             — no -test-sources.jar (meta-pom)
junit-jupiter-api-5.10.2         — no -test-sources.jar (the 3 *Test.java matches
                                   are the @Test/@RepeatedTest/@DynamicTest
                                   annotation interfaces, not assertion tests)
junit-jupiter-engine-5.10.2      — no -test-sources.jar on Central
junit-jupiter-params-5.10.2      — no -test-sources.jar on Central
junit-platform-commons-1.10.2    — no -test-sources.jar on Central
junit-platform-engine-1.10.2     — no -test-sources.jar on Central
opentest4j-1.3.0                 — no -test-sources.jar on Central
```

## What was wrong before (and what fixed it)

An earlier run reported **2 proven / 17 GAP** — a FALSE-LOW count.
`commons-lang3` and `commons-text` were marked GAP for "no liftable shape" when
they in fact lift 3067 and 1118 contracts respectively.

**Root cause:** a vendor test string literal containing a C0 control character
(form-feed, vertical-tab, NUL — common in commons-lang3's whitespace/separator
tests) leaked a raw control byte into the kit's JSON-RPC response, because the
`esc()` JSON escaper handled only `\n \r \t`. The rust mint then aborted parsing
the WHOLE artifact ("control character found while parsing a string"), zeroing
out thousands of valid contracts.

**Fix:** `esc()` now escapes the full C0 control range (U+0000–U+001F) per JSON
spec (RFC 8259 §7). Additionally, the kit's per-file lift loop is now isolated:
a single vendor file that throws is skip-and-diagnosed, never zeroing the
artifact (mirrors the rust coretests_sweep per-file tolerance).

## Honest coverage note

The kit lifts:
- `assertEquals(intLiteral, call(intArgs))` → #euf#-federated
- `assertEquals("strLiteral", call(getBytesUtf8("lit")))` → #euf#-federated
- `assertEquals(lit, ssaLocal)` where `ssaLocal = call(...)` → substituted (P5c)
- Instance-method calls `receiver.method()` → location-keyed (P5c)

Not yet lifted (the honest within-artifact perimeter, counted as refused-by-name):
- Assertions with symbolic/non-literal args
- `assertArrayEquals`, `assertThrows`, and other non-equality predicates
- Tests whose assertion framework is not JUnit5 (JUnit4, Hamcrest, etc.)
