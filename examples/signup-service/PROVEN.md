# Supply-Chain Proof Run — signup-service

**Date:** 2026-06-10  
**Kit:** `JavaTestAssertionsRpc` v0.7.0 (P5c: call-binding lift)  
**Sugar CLI:** debug build from main  
**JUnit5 vendor:** `tests/fixtures/vendor/junit5/` (Assertions.java, AssertEquals.java, …)

## Result

| Status | Count | Artifacts |
|--------|-------|-----------|
| PROOF  | 2     | commons-codec-1.16.1, commons-io-2.15.1 |
| GAP    | 17    | everything else (see below) |

**Total resolved artifacts:** 19  
**Proven:** 2 (assertions from their own JUnit5 test sources lifted and discharged)  
**Perimeter (GAP):** 17 (no sworn behavior; no test-source jar or no liftable assertions)

## Proven artifacts

- **commons-codec-1.16.1** — test-source jar resolved; assertEquals calls with
  literal args (byte arrays via `StringUtils.getBytesUtf8("...")`, int values) lifted
  from `Base64Test.java` and sibling test files. P5c SSA substitution lifts the dominant
  shape `String enc = encoder.encode(b); assertEquals("...", enc)` as location-keyed
  instance-method contracts.

- **commons-io-2.15.1** — test-source jar resolved; assertEquals calls with int/String
  literal args lifted from IO utility tests.

## Perimeter (GAP) — the map of what nobody warranted

These 17 artifacts have no test-source jar available from Maven Central, or their
test-source jars contain no assertions with literal expected values that the kit can
lift. The GAP lines ARE the product: they enumerate the supply-chain gaps precisely.

```
apiguardian-api-1.1.2        — no test-source jar
classmate-1.5.1              — no test-source jar
commons-lang3-3.14.0         — test-source jar exists but no liftable assertion shape
commons-text-1.11.0          — test-source jar exists but no liftable assertion shape
expressly-5.0.0              — no test-source jar
gson-2.10.1                  — no test-source jar
hibernate-validator-8.0.1.Final — no test-source jar
jakarta.el-api-5.0.0         — no test-source jar
jakarta.validation-api-3.0.2 — no test-source jar
jboss-logging-3.4.3.Final    — no test-source jar
junit-jupiter-5.10.2         — no test-source jar (meta-pom)
junit-jupiter-api-5.10.2     — no test-source jar
junit-jupiter-engine-5.10.2  — no test-source jar
junit-jupiter-params-5.10.2  — no test-source jar
junit-platform-commons-1.10.2 — no test-source jar
junit-platform-engine-1.10.2 — no test-source jar
opentest4j-1.3.0             — no test-source jar
```

## Honest coverage note

The 2 proven artifacts demonstrate the mechanism is real: the kit lifts from
vendored test source, the supply-chain loop unpacks source jars, and `sugar mint`
produces `.proof` files. The 17 GAP lines are not failures — they are the exact
enumeration of what no vendor ever swore to. The perimeter is the deliverable.

The current kit lifts:
- `assertEquals(intLiteral, call(intArgs))` → #euf#-federated
- `assertEquals("strLiteral", call(getBytesUtf8("lit")))` → #euf#-federated  
- `assertEquals(lit, ssaLocal)` where `ssaLocal = call(...)` → substituted (P5c)
- Instance-method calls `receiver.method()` → location-keyed (P5c)

Not yet lifted (honest GAP within lifted artifacts):
- Assertions with symbolic/non-literal args
- `assertArrayEquals`, `assertThrows`, and other non-equality predicates
- Tests where the assertion framework is not in the JUnit5 vendor set
  (JUnit4, Hamcrest, etc. need separate vocab derivation)
