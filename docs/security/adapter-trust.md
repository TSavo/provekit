# Adapter trust: lift adapters as TCB

Lift adapters walk source-library annotations and emit canonical IR. The IR is the protocol's lingua franca; the adapter is the translation layer.

If the translation is wrong, the canonical IR doesn't reflect what the source-library annotation actually means. Subsequent contracts derived from the wrong IR are wrong, and every downstream consumer who trusts those contracts inherits the wrongness.

Adapters are TCB. This doc walks the implications.

## The translation layer

A typical lift:

```
source-library annotation     →     intermediate representation     →     canonical IR
(@Min(0), @Max(150))                 (parsed, structured)                  (forall x: Int. x >= 0 and x <= 150)
```

The "intermediate representation" is the adapter's internal data structure; it's not the protocol. The canonical IR is the protocol. The adapter's correctness claim is: "for every source-library annotation I claim to support, my output canonical IR captures the annotation's full semantics."

If the adapter's claim is wrong, the canonical IR misrepresents the annotation. The misrepresentation propagates.

## Common failure modes

### Mode 1: missing operator precedence handling

JML annotation:
```
//@ requires x >= 0 && y >= 0 || z != null;
```

Without explicit parenthesization, this is `(x >= 0 && y >= 0) || z != null` (per Java operator precedence). An adapter that lifts to `(x >= 0) && (y >= 0 || z != null)` is wrong.

This mode is rare in adapters that use the source library's own parser (which gets precedence right). It's common in adapters that hand-roll a parser.

### Mode 2: wrong canonical predicate name

Bean Validation `@NotNull` should canonicalize to `not_null`. An adapter that emits `is_not_null` or `nonNull` produces canonical IR that doesn't match other adapters' IR for the same semantics.

The Bean Validation `@NotNull` and JML `//@ requires x != null` should produce byte-identical canonical IR. If one adapter emits `not_null` and another emits `nonNull`, they don't match. Cross-adapter parity tests catch this.

### Mode 3: incorrect sort inference

Adapter encounters `@Min(0)` on a field of type `BigDecimal`. Adapter emits IR with sort `Int`. Wrong: `BigDecimal` has decimal precision; the constraint behaves differently on `2.5 - 0.0001` than on `2`. The canonical IR loses precision.

Adapters that lift to a coarser sort than the source library's actual type cause subtle wrongness. The IR should match the source's semantics; if your IR's sort is `Int` but the source's type is `BigDecimal`, the IR is wrong.

### Mode 4: undefined-behavior coercion

Source-library annotation says "validates as URL." The adapter encodes a regex for URL validation. The regex is approximate. The canonical IR claims `matches_pattern(x, "...")` for the approximate regex. The semantics drift.

This is unavoidable when the source library doesn't have crisp semantics. Adapters should pick a documented, conservative interpretation, not a "best-guess" one. Better to lift fewer annotations correctly than more annotations approximately.

### Mode 5: missing context

Source-library annotation `@AssertTrue` on a method. The IR should encode "the method's return value is true." The adapter encodes "the field's value is true" (mistakenly treating method as field).

Adapters that don't propagate enough context (function vs. field, method receiver, parameter binding) emit IR with the wrong scope.

## Cross-adapter parity tests as a defense

The strongest defense against adapter mis-translation is cross-adapter parity. If two adapters claim to handle equivalent constraints, run both adapters on equivalent inputs and verify the canonical IR bytes are identical.

Worked example: the Java kit ships parity tests for:

- `@NotNull` (Bean Validation) ↔ `//@ requires x != null` (JML) ↔ `@RequestParam(required=true)` (Spring Web).
- `@Min(0) @Max(100)` (Bean Validation) ↔ `//@ requires score >= 0 && score <= 100` (JML).

If any adapter emits canonical IR that diverges, the parity test fails. Drift is caught at CI time.

These tests are real engineering work; not all kits have them yet. They are listed in the adapter coverage rubric ([`../contributing/adapter-coverage-rubric.md`](../contributing/adapter-coverage-rubric.md)) as a strong-coverage requirement.

## The adapter audit problem

Every adapter is a piece of software. It has bugs. The bugs may be:

- **Detected by adapter unit tests** (best case).
- **Detected by conformance fixtures** (good).
- **Detected by cross-adapter parity tests** (better).
- **Not detected** (worst case — silent wrongness).

A user's TCB includes every adapter their dependencies' `.proof` files invoked. If a dependency's `.proof` was produced by `provekit-lift-zod` v0.3.1, and v0.3.1 has a bug in handling `z.string().email()`, the user's verifier discharges call sites against the buggy IR.

The protocol does not detect this. Adapter audit is out-of-band: the user must trust the adapter author to have tested adequately, and the adapter version to not have known bugs.

Mitigation: pin adapter versions, watch for adapter advisories, prefer adapters with strong test coverage and active maintenance.

## What the protocol does provide for adapter trust

- **Tamper-evidence.** The adapter's output is signed; modification is detectable.
- **Traceability.** Each `.proof`'s metadata records the adapter and version that produced it. A user investigating "where did this contract come from?" can trace to the adapter.
- **Reproducibility.** Re-running the same adapter on the same source produces byte-identical canonical IR. Adapter behavior is deterministic.

What the protocol does NOT provide:

- Adapter correctness validation.
- Adapter version verification (the adapter itself isn't required to be content-addressed, though packages typically are).
- Cross-adapter parity enforcement (this is per-kit policy).

## Operational recommendation

For users:

1. **Pin adapter versions** in your build configuration. A newer adapter version may handle annotations differently.
2. **Read coverage manifests.** Each shipping adapter ships a `COVERAGE.md` listing what it handles, skips, and unrecognizes. Trust the manifest as the contract.
3. **Watch for cross-adapter parity tests** in the kit's repository. Their absence is a yellow flag; their presence is a green flag.
4. **Report mis-translations** to the adapter's repository. Adapter bugs are real bugs; bug reports help.

For adapter authors:

5. **Write parity tests** if a sibling adapter exists.
6. **Use the source library's own parser** rather than hand-rolling.
7. **Be conservative on coverage.** Lift less, lift correctly. See [`../contributing/adapter-coverage-rubric.md`](../contributing/adapter-coverage-rubric.md).
8. **Document semantics.** Each annotation handled should have a documented canonical IR; reviewers can sanity-check.

## The protocol-vs-framework distinction restated

The protocol is the substrate. The adapter is one of the things that ships verifications via the substrate. Adapter correctness is upstream of protocol correctness; the protocol carries whatever the adapter produced.

This means trust in a specific `.proof` decomposes as:

- **Trust in the protocol's primitives** (cryptography, canonicalization, signing).
- **Trust in the kit's implementation** of the protocol (canonicalizer correctness, etc.).
- **Trust in the lift adapter** that produced the contracts inside the `.proof`.
- **Trust in the solver** (if the `.proof` includes implication mementos signed by a prover).
- **Trust in the signer** of the `.proof` itself.

Each layer has its own audit story. The protocol provides tools for auditing; users do the auditing.

## Read next

- [solver-trust.md](solver-trust.md) — solver as TCB.
- [signature-and-non-repudiation.md](signature-and-non-repudiation.md) — what signatures buy.
- [`../contributing/adapter-coverage-rubric.md`](../contributing/adapter-coverage-rubric.md) — what counts as good coverage.
- [`../contributing/writing-a-lift-adapter/04-conformance-test.md`](../contributing/writing-a-lift-adapter/04-conformance-test.md) — the fixture-based defense.
