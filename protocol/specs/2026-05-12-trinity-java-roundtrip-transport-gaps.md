# Trinity Round-Trip Transport Gaps: Java Leg

**Date:** 2026-05-12
**Status:** empirical finding (v0)
**Branch:** feat/trinity-java-roundtrip
**Companion spec:** 2026-05-14-transport-gap-and-partial-morphism-protocol.md
**Companion test:** implementations/java/provekit-lift-java-source/src/test/java/com/provekit/lift/java_source/TrinityRoundtripLiftTest.java

## Summary

The trinity round-trip fixture (11 catalog concepts + retry-loop) was run through
`provekit bind --rewrite=canonical --target-language=java` and the emitted Java was
re-lifted with `provekit-lift-java-source`. This document records the per-concept
verdicts and the gap reasons per the transport-gap spec trichotomy
(Exact / Loudly-Bounded-Lossy / Refuse).

**v0 result: 0 / 11 concepts exact.**

## How the bind was run

```
cargo run -p provekit-cli --quiet -- bind \
  --root implementations/rust/provekit-cli/tests/fixtures/trinity_roundtrip \
  --lang rust \
  --target-language java \
  --rewrite canonical \
  --mode monitor \
  --output /tmp/java-out-trinity
```

Output: `bind: 12 bindings (7 exact / 1 lossy / 4 refused)`

The bind-side verdicts (exact/lossy/refused) refer to the Rust-lift-to-concept transport,
not the Java leg. See `/tmp/java-out-trinity/index.json` and `/tmp/java-out-trinity/gaps.json`
for the bind-side records.

## Gap 1: `bind-stub-body-emitted` (applies to all 12 bindings)

**Kind:** `missing-target-construct` (v0 capability gap)

**Source:** bind v0 canonical rewrite emits stub bodies for all Java classes:
```java
throw new UnsupportedOperationException("provekit-bind canonical: <concept>");
```

The original Rust function bodies are not translated. No term graph representing
the source logic appears in the emitted Java.

**Effect on re-lift:** The lifter successfully parses and lifts each class. The
lifted post-condition for each function is:
```
eq(return_value, java:throw(java:new("UnsupportedOperationException", ...)))
```
with a `panics` effect. This is correct for the stub, but it does not encode the
original concept logic. The Rust-side IR cannot be recovered.

**Gap tags from bind:** `bind-stub-body-emitted` (recorded in `/tmp/java-out-trinity/gaps.json`)

**Resolution option:** `deferred` -- v1 body translation would require a Java
realizer that lowers concept IR to Java expressions. The bind-side ORP realizer
currently emits stubs for all canonical rewrites. This is the primary gap blocking
exact round-trip closure.

## Gap 2: `&i64` parameter type not valid Java (option, option-bind, list)

**Concepts affected:** `concept:option` (maybe_first), `concept:option-bind`
(option_bind_double), `concept:list` (list_sum)

**Kind:** `arity-shape-mismatch` (bind v0 emits Rust slice notation verbatim)

**Source:** bind v0 emits `&i64 items` as the parameter type for slice parameters.
This is not valid Java syntax. Example from emitted lib.java:
```java
public static long maybe_first(&i64 items) {
```

**Effect on re-lift:** The JDK compiler's error-recovery path processes the
`&i64` type token as an unresolvable reference and erases it to `<any>`. The
method IS attributed as an `ExecutableElement` and the lifter produces a
declaration. However, the formal sort is `Ref` (erased) instead of the intended
array sort. The lifted function name is `MaybeFirstTransported.maybe_first(<any>)`,
not a valid Java erased name. Type information is lost.

**Gap tags:** `bind-invalid-java-param-type`, `lift-any-erasure`

**Resolution option:** `deferred` -- bind v0 must emit valid Java array parameters
(`long[] items`) for slice types. This requires the Java realizer to know the
Java representation of `&[T]` (Java array or `long[]`).

## Gap 3: Concept misclassification (identity, bool-cell)

**Concepts affected:** `concept:identity` (wrap_identity), `concept:bool-cell` (toggle)

**Kind:** `wp-rule-mismatch` (bind v0 soft-match classification error)

**Source:** bind v0 classifies both `wrap_identity` and `toggle` as `concept:pair`
(confirmed in emitted lib.java annotations and `/tmp/java-out-trinity/index.json`).

```java
// @provekit_monitor(concept = "pair")
final class WrapIdentityTransported {
    // concept: pair
    public static long wrap_identity(long x) { ... }
}
```

The identity concept has `0 sites` in the bind output (recorded in gaps.json:
`below-threshold: concept:identity has 0 sites`). Same for bool-cell.

**Effect on re-lift:** The lifted contracts for `wrap_identity` and `toggle` carry
the `concept:pair` annotation, not `concept:identity` / `concept:bool-cell`. Even
if bodies were real, the round-trip IR would not match the Rust-lift IR for these
concepts because the concept CID would differ.

**Gap tags:** `bind-concept-misclassification`, `below-threshold`

**Resolution option:** `deferred` -- requires bind v0 classifier improvement for
single-parameter identity and boolean-negation patterns.

## Gap 4: `do_nothing` void return (unit concept)

**Concept:** `concept:unit`

**Kind:** `missing-target-construct` (lifter v1 slice excludes void returns)

**Source:** The emitted Java class is:
```java
final class DoNothingTransported {
    public static void do_nothing() {
        throw new UnsupportedOperationException("provekit-bind canonical: unit");
    }
}
```

**Effect on re-lift:** The lifter explicitly refuses void-returning methods with
`unsupported-return-sort`. The `do_nothing` method produces a `Refusal` record,
not a function-contract declaration.

**Gap tags:** `lift-void-return-refused`, `lift-v1-slice-limitation`

**Resolution option:** `deferred` -- the lifter v1 slice is value-returning only.
Lifting `unit`-concept functions requires either a wrapper return type or an
extension to the lifter slice to handle `void`.

## Per-concept verdict table

Two verdict columns: "Lifter" (what the lifter does with the emitted Java) and
"Round-trip" (whether the lifted IR recovers the original Rust concept IR).

| Concept      | Rust fn                  | Emitted class                   | Lifter verdict | Round-trip verdict | Primary gap                              |
|--------------|--------------------------|----------------------------------|----------------|--------------------|------------------------------------------|
| identity     | wrap_identity            | WrapIdentityTransported          | LIFTED         | REFUSE             | Gap 1 (stub body) + Gap 3 (misclassified as pair) |
| unit         | do_nothing               | DoNothingTransported             | REFUSED        | REFUSE             | Gap 4 (void return refused by lifter)    |
| bool-cell    | toggle                   | ToggleTransported                | LIFTED         | REFUSE             | Gap 1 (stub body) + Gap 3 (misclassified as pair) |
| assert       | assert_positive          | AssertPositiveTransported        | LIFTED         | REFUSE             | Gap 1 (stub body)                        |
| option       | maybe_first              | MaybeFirstTransported            | LIFTED (&any)  | REFUSE             | Gap 1 (stub body) + Gap 2 (&i64)         |
| option-bind  | option_bind_double       | OptionBindDoubleTransported      | LIFTED (&any)  | REFUSE             | Gap 1 (stub body) + Gap 2 (&i64)         |
| result       | safe_divide              | SafeDivideTransported            | LIFTED         | REFUSE             | Gap 1 (stub body)                        |
| result-bind  | safe_divide_then_double  | SafeDivideThenDoubleTransported  | LIFTED         | REFUSE             | Gap 1 (stub body)                        |
| pair         | swap_pair                | SwapPairTransported              | LIFTED         | REFUSE             | Gap 1 (stub body)                        |
| list         | list_sum                 | ListSumTransported               | LIFTED (&any)  | REFUSE             | Gap 1 (stub body) + Gap 2 (&i64)         |
| tagged-union | classify                 | ClassifyTransported              | LIFTED         | REFUSE             | Gap 1 (stub body)                        |
| retry-loop   | retry_until_success      | RetryUntilSuccessTransported     | LIFTED         | REFUSE             | Gap 1 (stub body)                        |

Note: retry-loop is the 12th binding (not one of the 11 trinity concepts). Included for completeness.

**Round-trip EXACT: 0 / 11 trinity concepts.**
**Round-trip REFUSE: 11 (stub bodies lift but do not recover original Rust IR; see below)**
**Lifter REFUSE: 1 (do_nothing: void return refused by lifter v1 slice)**

### Why REFUSE and not LOUDLY-BOUNDED-LOSSY for stub bodies

Per transport-gap spec §0.1, LOUDLY-BOUNDED-LOSSY is the legitimate case when the
transformation is "correct *except* on a precisely-characterized failure set" --
i.e., there is a non-empty domain of agreement.

Stub bodies (`throw new UnsupportedOperationException(...)`) agree with the source
Rust logic on the **empty set**: the stub never returns normally, so no input
witnesses agreement. The domain-of-agreement is empty. This is total loss, not
bounded loss. Per Supra omnia rectum, REFUSE is the honest verdict.

The distinction: LOUDLY-BOUNDED-LOSSY would mean "we know where it is wrong and we
ship the bridge anyway." REFUSE means "we cannot characterize a domain where it is
right." Gap 1 is in the second category until the bind realizer emits real bodies.

### What the lifted IR does capture (non-zero recoverable information)

The round-trip is not entirely without value. Each lifted method's post-condition is:
```
post: eq(return_value, java:throw(java:new("UnsupportedOperationException", "provekit-bind canonical: <concept>")))
effects: [panics]
```

The recoverable information from the round-trip:
1. Function signature (parameter names and types, modulo &i64 erasure for slice params)
2. Class name (e.g. `MaybeFirstTransported`)
3. Concept annotation (from `@provekit_monitor` comment -- parsed as a comment, not a declaration)
4. Memento-CID comment (for algebra-synthesis origin classes; the CID is in the comment)

The original Rust function bodies are absent. No concept logic is preserved.

## What closes the gaps

All four gaps are upstream of the lifter. The lifter correctly processes the
emitted Java. The path to exact round-trip is:

1. **Gap 1 (stub bodies):** Implement a Java body realizer in the bind pipeline
   that lowers concept IR to Java expressions. This is the primary v1 milestone.

2. **Gap 2 (&i64 params):** Fix bind v0 to emit `long[]` (or `List<Long>`) for
   slice parameters instead of `&i64`.

3. **Gap 3 (misclassification):** Fix bind v0 classifier for identity and bool-cell
   single-parameter patterns.

4. **Gap 4 (void lift):** Extend the lifter v1 slice to handle void-returning
   methods (or emit a return type wrapper for unit-concept functions).

Gaps 2, 3, 4 are individually closeable; Gap 1 is the high-value milestone.
None of these require changes to the realizer pipeline or the trinity fixture.
